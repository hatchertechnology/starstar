//! PostgreSQL-specific integration and independent-connection concurrency tests.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::Request;
use starstar_api::auth::AuthStore;
use starstar_api::auth::clock::Clock;
use starstar_api::auth::crypto::pkce_s256;
use starstar_api::auth::password::Hasher;
use starstar_api::auth::store::codes::CodeParams;
use starstar_api::auth::store::{AuthStoreError, GrantError};
use starstar_api::config::Config;
use starstar_api::prusalink::http_client;
use starstar_api::store::test_db::TestSchema;
use starstar_api::store::{Db, PrinterStore};
use starstar_api::{AppState, router};
use starstar_shared::Role;
use tokio::sync::Barrier;

mod common;

async fn store_in(schema: &TestSchema, clock: Clock) -> AuthStore {
    let db = schema.connect_with_max_connections(1).await.unwrap();
    AuthStore::new(db, clock, Hasher::fast_for_tests())
        .await
        .unwrap()
}

#[tokio::test]
async fn health_endpoint_returns_503_when_database_is_unavailable() {
    let db = Db::connect_lazy_unmigrated(
        "postgres://starstar:secret-pw@127.0.0.1:1/starstar",
        Duration::from_millis(150),
    )
    .unwrap();
    let state = AppState {
        library: Arc::new(
            starstar_api::library::store::LibraryStore::temporary(db.pool().clone()).unwrap(),
        ),
        store: Arc::new(PrinterStore::from_db(db.clone())),
        auth: Arc::new(
            AuthStore::connect_test(Clock::manual(common::TEST_START))
                .await
                .unwrap(),
        ),
        http: http_client().unwrap(),
        config: Arc::new(Config::default()),
    };
    let app = router(state);
    let r = common::send(
        &app,
        Request::get("/api/health").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(r.status, 503);
    assert!(!r.body.contains("secret-pw"), "{}", r.body);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn independent_connections_create_first_admin_once() {
    let schema = TestSchema::create().await;
    // Initialize schema once before racing independent connection pools.
    let seed = store_in(&schema, Clock::manual(1_000)).await;
    let barrier = Arc::new(Barrier::new(4));

    let mut tasks = Vec::new();
    for i in 0..4 {
        let schema = schema.clone();
        let barrier = Arc::clone(&barrier);
        tasks.push(tokio::spawn(async move {
            let s = store_in(&schema, Clock::manual(1_000)).await;
            barrier.wait().await;
            s.create_first_admin(&format!("admin{i}"), "hash").await
        }));
    }

    let mut ok = 0;
    let mut closed = 0;
    for t in tasks {
        match t.await.unwrap() {
            Ok(_) => ok += 1,
            Err(AuthStoreError::SetupClosed) => closed += 1,
            Err(other) => panic!("unexpected error: {other}"),
        }
    }
    assert_eq!((ok, closed), (1, 3));
    assert_eq!(seed.user_count().await.unwrap(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn independent_connections_guard_last_enabled_admin() {
    // 1. Concurrent remove_user on two admins from independent pools.
    let schema = TestSchema::create().await;
    let s1 = Arc::new(store_in(&schema, Clock::manual(1_000)).await);
    let s2 = Arc::new(store_in(&schema, Clock::manual(1_000)).await);
    s1.create_user("a1", "h", Role::Admin).await.unwrap();
    s1.create_user("a2", "h", Role::Admin).await.unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let t1 = {
        let s1 = Arc::clone(&s1);
        let b = Arc::clone(&barrier);
        tokio::spawn(async move {
            b.wait().await;
            s1.remove_user("a1").await
        })
    };
    let t2 = {
        let s2 = Arc::clone(&s2);
        let b = Arc::clone(&barrier);
        tokio::spawn(async move {
            b.wait().await;
            s2.remove_user("a2").await
        })
    };
    let (r1, r2) = tokio::time::timeout(Duration::from_secs(10), async { tokio::join!(t1, t2) })
        .await
        .expect("concurrent remove_user hung");
    let results = [r1.unwrap(), r2.unwrap()];
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(AuthStoreError::LastAdmin)))
            .count(),
        1
    );
    let remaining: Vec<_> = s1
        .list_users()
        .await
        .unwrap()
        .into_iter()
        .filter(|u| u.role == Role::Admin && !u.disabled)
        .collect();
    assert_eq!(remaining.len(), 1);

    // 2. Concurrent disable and demote on two remaining admins from independent pools.
    let schema2 = TestSchema::create().await;
    let u1 = Arc::new(store_in(&schema2, Clock::manual(1_000)).await);
    let u2 = Arc::new(store_in(&schema2, Clock::manual(1_000)).await);
    u1.create_user("a1", "h", Role::Admin).await.unwrap();
    u1.create_user("a2", "h", Role::Admin).await.unwrap();

    let barrier2 = Arc::new(Barrier::new(2));
    let p1 = {
        let u1 = Arc::clone(&u1);
        let b = Arc::clone(&barrier2);
        tokio::spawn(async move {
            b.wait().await;
            u1.update_user("a1", None, Some(true)).await
        })
    };
    let p2 = {
        let u2 = Arc::clone(&u2);
        let b = Arc::clone(&barrier2);
        tokio::spawn(async move {
            b.wait().await;
            u2.update_user("a2", Some(Role::Operator), None).await
        })
    };
    let (pr1, pr2) = tokio::time::timeout(Duration::from_secs(10), async { tokio::join!(p1, p2) })
        .await
        .expect("concurrent update_user hung");
    let p_results = [pr1.unwrap(), pr2.unwrap()];
    assert_eq!(p_results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        p_results
            .iter()
            .filter(|r| matches!(r, Err(AuthStoreError::LastAdmin)))
            .count(),
        1
    );
    let remaining2: Vec<_> = u1
        .list_users()
        .await
        .unwrap()
        .into_iter()
        .filter(|u| u.role == Role::Admin && !u.disabled)
        .collect();
    assert_eq!(remaining2.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn independent_connections_serialize_code_exchange_and_revoke_on_replay() {
    let schema = TestSchema::create().await;
    let s1 = Arc::new(store_in(&schema, Clock::manual(1_000)).await);
    let s2 = Arc::new(store_in(&schema, Clock::manual(1_000)).await);
    let user = s1.create_user("kerry", "h", Role::Admin).await.unwrap();
    let verifier = "verifier-with-sufficient-entropy-for-pkce-s256";
    let challenge = pkce_s256(verifier);
    let code = s1
        .create_code(&CodeParams {
            client_id: "starstar-cli",
            user_id: user.id,
            role: Role::Operator,
            redirect_uri: "http://127.0.0.1/callback",
            code_challenge: &challenge,
            resource: None,
        })
        .await
        .unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let ex = |store: Arc<AuthStore>, code: String, barrier: Arc<Barrier>| {
        tokio::spawn(async move {
            barrier.wait().await;
            store
                .exchange_code(&code, "starstar-cli", "http://127.0.0.1/callback", verifier)
                .await
        })
    };
    let (r1, r2) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(
            ex(Arc::clone(&s1), code.clone(), Arc::clone(&barrier)),
            ex(Arc::clone(&s2), code, barrier)
        )
    })
    .await
    .expect("concurrent exchange_code hung");

    let mut issued = Vec::new();
    let mut invalid = 0;
    for res in [r1.unwrap(), r2.unwrap()] {
        match res {
            Ok(tok) => issued.push(tok),
            Err(GrantError::Invalid(_)) => invalid += 1,
            Err(other) => panic!("unexpected error: {other}"),
        }
    }
    assert_eq!((issued.len(), invalid), (1, 1));
    assert!(
        s1.access_token_caller(&issued[0].access_token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(s1.list_grants(None).await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn independent_connections_serialize_refresh_rotation_grace_and_reuse() {
    // 1. Two independent connections refreshing concurrently both succeed on one grant.
    let schema = TestSchema::create().await;
    let s1 = Arc::new(store_in(&schema, Clock::manual(1_000)).await);
    let s2 = Arc::new(store_in(&schema, Clock::manual(1_000)).await);
    let u = s1.create_user("kerry", "h", Role::Admin).await.unwrap();
    let g = s1
        .create_grant(u.id, "starstar-cli", Role::Operator, None)
        .await
        .unwrap();
    let t = s1.issue_tokens(&g).await.unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let r1 = {
        let s1 = Arc::clone(&s1);
        let rt = t.refresh_token.clone();
        let b = Arc::clone(&barrier);
        tokio::spawn(async move {
            b.wait().await;
            s1.refresh(&rt, "starstar-cli").await
        })
    };
    let r2 = {
        let s2 = Arc::clone(&s2);
        let rt = t.refresh_token.clone();
        let b = Arc::clone(&barrier);
        tokio::spawn(async move {
            b.wait().await;
            s2.refresh(&rt, "starstar-cli").await
        })
    };
    let (a, b) = tokio::time::timeout(Duration::from_secs(10), async { tokio::join!(r1, r2) })
        .await
        .expect("concurrent refresh hung");
    let a = a.unwrap().unwrap();
    let b = b.unwrap().unwrap();
    assert_ne!(a.access_token, b.access_token);
    assert!(
        s1.access_token_caller(&a.access_token)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        s1.access_token_caller(&b.access_token)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(s1.list_grants(None).await.unwrap().len(), 1);

    // 2. Three independent connections refreshing concurrently trigger reuse detection and revoke.
    let s3 = Arc::new(store_in(&schema, Clock::manual(1_000)).await);
    let g2 = s1
        .create_grant(u.id, "starstar-cli", Role::Operator, None)
        .await
        .unwrap();
    let t2 = s1.issue_tokens(&g2).await.unwrap();
    let barrier3 = Arc::new(Barrier::new(3));
    let spawn_ref = |store: Arc<AuthStore>, b: Arc<Barrier>| {
        let rt = t2.refresh_token.clone();
        tokio::spawn(async move {
            b.wait().await;
            store.refresh(&rt, "starstar-cli").await
        })
    };
    let (x1, x2, x3) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(
            spawn_ref(Arc::clone(&s1), Arc::clone(&barrier3)),
            spawn_ref(Arc::clone(&s2), Arc::clone(&barrier3)),
            spawn_ref(Arc::clone(&s3), barrier3),
        )
    })
    .await
    .expect("three-way concurrent refresh hung");

    let mut ok_tokens = Vec::new();
    let mut invalid = 0;
    for res in [x1.unwrap(), x2.unwrap(), x3.unwrap()] {
        match res {
            Ok(tok) => ok_tokens.push(tok),
            Err(GrantError::Invalid(_)) => invalid += 1,
            Err(other) => panic!("unexpected error: {other}"),
        }
    }
    assert_eq!((ok_tokens.len(), invalid), (2, 1));
    for tok in ok_tokens {
        assert!(
            s1.access_token_caller(&tok.access_token)
                .await
                .unwrap()
                .is_none()
        );
    }
    // Only the first grant `g` remains; `g2` was revoked.
    let grants = s1.list_grants(None).await.unwrap();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].id, g);
}
