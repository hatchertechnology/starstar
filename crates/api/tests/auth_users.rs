//! Users API, `/api/me` and password change.

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use serde_json::{Value, json};
use starstar_api::AppState;
use starstar_shared::Role;

mod common;
use common::*;

async fn as_role(
    app: &Router,
    state: &AppState,
    role: Role,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> TestResponse {
    let b = authorize(Request::builder().method(method).uri(uri), state, role)
        .header("content-type", "application/json");
    let body = body.map_or_else(Body::empty, |v| Body::from(v.to_string()));
    send(app, b.body(body).unwrap()).await
}

#[tokio::test]
async fn admin_manages_users() {
    let (app, state) = test_app();
    let a = |m: &'static str, u: &'static str, b: Option<Value>| {
        let (app, state) = (app.clone(), state.clone());
        async move { as_role(&app, &state, Role::Admin, m, u, b).await }
    };
    let r = a(
        "POST",
        "/api/users",
        Some(json!({"username":"op","password":"pw-op-12345","role":"operator"})),
    )
    .await;
    assert_eq!(r.status, 201);
    assert_eq!(
        r.json(),
        json!({"username":"op","role":"operator","disabled":false,"created_at":1800000000})
    );
    let r = a("GET", "/api/users", None).await;
    assert_eq!(r.status, 200);
    assert!(
        r.json()
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["username"] == "op")
    );
    assert!(!r.body.contains("pw-op-12345") && !r.body.contains("$argon2"));

    let r = a("PATCH", "/api/users/op", Some(json!({"role":"viewer"}))).await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["role"], "viewer");
    let r = a("PATCH", "/api/users/op", Some(json!({"disabled":true}))).await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["disabled"], true);
    assert_eq!(
        a("PATCH", "/api/users/op", Some(json!({}))).await.status,
        400
    );

    let dup = json!({"username":"op","password":"pw-op-12345","role":"operator"});
    assert_eq!(a("POST", "/api/users", Some(dup)).await.status, 409);
    assert_eq!(
        a("PATCH", "/api/users/ghost", Some(json!({"disabled":true})))
            .await
            .status,
        404
    );
    let bad_name = json!({"username":"Bad","password":"pw-op-12345","role":"viewer"});
    assert_eq!(a("POST", "/api/users", Some(bad_name)).await.status, 400);
    let bad_pw = json!({"username":"ok","password":"short","role":"viewer"});
    assert_eq!(a("POST", "/api/users", Some(bad_pw)).await.status, 400);

    assert_eq!(a("DELETE", "/api/users/op", None).await.status, 204);
    assert_eq!(a("DELETE", "/api/users/op", None).await.status, 404);
}

#[tokio::test]
async fn last_admin_guard_via_api() {
    // A guard that deadlocks must fail the test, not hang it.
    tokio::time::timeout(std::time::Duration::from_secs(10), last_admin_guard())
        .await
        .expect("last-admin guard hung");
}

async fn last_admin_guard() {
    let (app, state) = test_app();
    let root = token_for(&state, "root", Role::Admin);
    let call = |m: &'static str, body: Option<Value>| {
        let req = Request::builder()
            .method(m)
            .uri("/api/users/root")
            .header("authorization", format!("Bearer {root}"))
            .header("content-type", "application/json")
            .body(body.map_or_else(Body::empty, |v| Body::from(v.to_string())))
            .unwrap();
        send(&app, req)
    };
    assert_eq!(
        call("PATCH", Some(json!({"role":"operator"}))).await.status,
        409
    );
    assert_eq!(
        call("PATCH", Some(json!({"disabled":true}))).await.status,
        409
    );
    assert_eq!(call("DELETE", None).await.status, 409);
}

#[tokio::test]
async fn removing_a_user_kills_their_tokens() {
    let (app, state) = test_app();
    let v = token_for(&state, "v", Role::Viewer);
    let r = as_role(&app, &state, Role::Admin, "DELETE", "/api/users/v", None).await;
    assert_eq!(r.status, 204);
    let req = Request::get("/api/me")
        .header("authorization", format!("Bearer {v}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, req).await.status, 401);
}

#[tokio::test]
async fn change_own_password() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Viewer);
    let b1 = login(&app, "kerry", "pw-kerry-123").await;
    let b2 = login(&app, "kerry", "pw-kerry-123").await;
    let put = |b: &Browser, body: Value| {
        let req = b
            .apply(Request::put("/api/me/password"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        send(&app, req)
    };
    let wrong = json!({"current_password":"nope-nope-1","new_password":"new-pw-12345"});
    assert_eq!(put(&b1, wrong).await.status, 403);
    let short = json!({"current_password":"pw-kerry-123","new_password":"short"});
    assert_eq!(put(&b1, short).await.status, 400);
    let ok = json!({"current_password":"pw-kerry-123","new_password":"new-pw-12345"});
    assert_eq!(put(&b1, ok).await.status, 204);

    let session = |b: &Browser| {
        send(
            &app,
            b.cookie_only(Request::get("/api/session"))
                .body(Body::empty())
                .unwrap(),
        )
    };
    assert_eq!(session(&b1).await.status, 200);
    assert_eq!(session(&b2).await.status, 401);
    let old = post_json(
        &app,
        "/api/session",
        json!({"username":"kerry","password":"pw-kerry-123"}),
    )
    .await;
    assert_eq!(old.status, 401);
    let new = post_json(
        &app,
        "/api/session",
        json!({"username":"kerry","password":"new-pw-12345"}),
    )
    .await;
    assert_eq!(new.status, 200);
}

#[tokio::test]
async fn password_change_revokes_grants_and_other_sessions() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Viewer);
    let app_token = token_for(&state, "kerry", Role::Viewer);
    let b = login(&app, "kerry", "pw-kerry-123").await;
    let other = login(&app, "kerry", "pw-kerry-123").await;
    let body = json!({"current_password":"pw-kerry-123","new_password":"new-pw-12345"});
    let req = b
        .apply(Request::put("/api/me/password"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    assert_eq!(send(&app, req).await.status, 204);
    let bearer = Request::get("/api/me")
        .header("authorization", format!("Bearer {app_token}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, bearer).await.status, 401);
    let old_browser = other
        .cookie_only(Request::get("/api/me"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, old_browser).await.status, 401);
    assert!(state.auth.list_grants(None).unwrap().is_empty());
}

#[tokio::test]
async fn wrong_current_password_is_rate_limited() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Viewer);
    let b = login(&app, "kerry", "pw-kerry-123").await;
    let attempt = |current: &str| {
        let body = json!({"current_password": current, "new_password": "new-pw-12345"});
        let req = b
            .apply(Request::put("/api/me/password"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        send(&app, req)
    };
    for _ in 0..5 {
        assert_eq!(attempt("wrong-wrong-1").await.status, 403);
    }
    assert_eq!(attempt("pw-kerry-123").await.status, 429);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_wrong_current_passwords_are_limited_to_five_evaluations() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Operator);
    let token = token_for(&state, "kerry", Role::Operator);
    let mut tasks = Vec::new();
    for _ in 0..50 {
        let app = app.clone();
        let token = token.clone();
        tasks.push(tokio::spawn(async move {
            let req = Request::put("/api/me/password")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"current_password": "nope-nope", "new_password": "another-pass-1"})
                        .to_string(),
                ))
                .unwrap();
            send(&app, req).await.status.as_u16()
        }));
    }
    let mut evaluated = 0;
    for t in tasks {
        match t.await.unwrap() {
            403 => evaluated += 1,
            429 => {}
            other => panic!("unexpected status {other}"),
        }
    }
    assert!(evaluated <= 5, "{evaluated} guesses were evaluated");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn user_creation_and_password_change_wait_for_an_argon2_permit() {
    let (app, state) = test_app();
    let admin = token_for(&state, "boss", Role::Admin);
    let sem = state.auth.argon_semaphore();
    let n = sem.available_permits() as u32;
    let held = sem.acquire_many_owned(n).await.unwrap();
    let task = {
        let app = app.clone();
        tokio::spawn(async move {
            let req = Request::post("/api/users")
                .header("authorization", format!("Bearer {admin}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"username": "newbie", "password": "pw-newbie-123", "role": "viewer"})
                        .to_string(),
                ))
                .unwrap();
            send(&app, req).await.status.as_u16()
        })
    };
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    assert!(!task.is_finished());
    drop(held);
    assert_eq!(task.await.unwrap(), 201);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_last_admin_deletes_leave_one_enabled_admin() {
    let (_app, state) = test_app();
    create_user(&state, "a1", "pw-a1-12345", Role::Admin);
    create_user(&state, "a2", "pw-a2-12345", Role::Admin);
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));

    let remove = |target: &'static str, barrier: std::sync::Arc<tokio::sync::Barrier>| {
        let state = state.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            state.auth.remove_user(target)
        })
    };

    let (r1, r2) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(remove("a1", barrier.clone()), remove("a2", barrier))
    })
    .await
    .expect("concurrent deletes hung");
    let results = [r1.unwrap(), r2.unwrap()];
    let ok_count = results.iter().filter(|r| r.is_ok()).count();
    let last_admin_count = results
        .iter()
        .filter(|r| matches!(r, Err(starstar_api::auth::store::AuthStoreError::LastAdmin)))
        .count();
    assert_eq!((ok_count, last_admin_count), (1, 1));

    let admins: Vec<_> = state
        .auth
        .list_users()
        .unwrap()
        .into_iter()
        .filter(|u| u.role == Role::Admin && !u.disabled)
        .collect();
    assert_eq!(admins.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_last_admin_disables_and_demotions_leave_one_enabled_admin() {
    let (app, state) = test_app();
    let t1 = token_for(&state, "a1", Role::Admin);
    let t2 = token_for(&state, "a2", Role::Admin);
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));

    let patch = |token: String,
                 target: &'static str,
                 body: Value,
                 barrier: std::sync::Arc<tokio::sync::Barrier>| {
        let app = app.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            let req = Request::patch(format!("/api/users/{target}"))
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap();
            send(&app, req).await.status.as_u16()
        })
    };

    let (r1, r2) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(
            patch(t1, "a1", json!({"disabled": true}), barrier.clone()),
            patch(t2, "a2", json!({"role": "operator"}), barrier),
        )
    })
    .await
    .expect("concurrent patches hung");
    let mut statuses = [r1.unwrap(), r2.unwrap()];
    statuses.sort();
    assert_eq!(statuses, [200, 409]);

    let admins: Vec<_> = state
        .auth
        .list_users()
        .unwrap()
        .into_iter()
        .filter(|u| u.role == Role::Admin && !u.disabled)
        .collect();
    assert_eq!(admins.len(), 1);
}

#[tokio::test]
async fn password_change_via_bearer_deletes_all_browser_sessions() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Viewer);
    let b1 = login(&app, "kerry", "pw-kerry-123").await;
    let b2 = login(&app, "kerry", "pw-kerry-123").await;
    let bearer = token_for(&state, "kerry", Role::Viewer);

    let req = Request::put("/api/me/password")
        .header("authorization", format!("Bearer {bearer}"))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"current_password": "pw-kerry-123", "new_password": "new-pw-12345"}).to_string(),
        ))
        .unwrap();
    assert_eq!(send(&app, req).await.status, 204);

    for b in [&b1, &b2] {
        let me_req = b
            .cookie_only(Request::get("/api/me"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(send(&app, me_req).await.status, 401);
    }
}
