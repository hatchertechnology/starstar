//! Cookie sessions, CSRF and the sign-in rate limit.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use serde_json::json;
use starstar_api::config::Config;
use starstar_api::router;
use starstar_shared::Role;

mod common;
use common::*;

async fn post_login(app: &Router, user: &str, pw: &str) -> TestResponse {
    post_json(
        app,
        "/api/session",
        json!({"username": user, "password": pw}),
    )
    .await
}

fn get(b: Browser, path: &str) -> Request<Body> {
    b.cookie_only(Request::get(path))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn login_sets_cookies_and_get_session_returns_user() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Operator);
    let b = login(&app, "kerry", "pw-kerry-123").await;
    let r = send(
        &app,
        b.cookie_only(Request::get("/api/session"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["username"], "kerry");
    assert_eq!(r.json()["role"], "operator");
    assert_eq!(r.json()["csrf_token"], b.csrf.as_str());
    let r = send(&app, get(b, "/api/me")).await;
    assert_eq!(r.status, 200);
}

#[tokio::test]
async fn session_cookie_attributes() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Viewer);
    let r = post_login(&app, "kerry", "pw-kerry-123").await;
    assert_eq!(r.status, 200);
    assert_eq!(r.headers["cache-control"], "no-store");
    assert!(!r.body.contains("pw-kerry-123"));
    let cookies: Vec<String> = r
        .headers
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().to_owned())
        .collect();
    let session = cookies
        .iter()
        .find(|c| c.starts_with("starstar_session="))
        .unwrap();
    assert!(session.contains("HttpOnly") && session.contains("SameSite=Lax"));
    assert!(!session.contains("Secure"));
    let csrf = cookies
        .iter()
        .find(|c| c.starts_with("starstar_csrf="))
        .unwrap();
    assert!(!csrf.contains("HttpOnly"));

    // https public URL: both cookies are Secure.
    let mut state = test_state();
    state.config = Arc::new(Config {
        public_url: "https://print.example.com".into(),
        ..Config::default()
    });
    create_user(&state, "kerry", "pw-kerry-123", Role::Viewer);
    let app = router(state);
    let r = post_login(&app, "kerry", "pw-kerry-123").await;
    assert_eq!(r.status, 200);
    for c in r.headers.get_all("set-cookie") {
        assert!(c.to_str().unwrap().contains("Secure"));
    }
}

#[tokio::test]
async fn unsafe_cookie_requests_need_csrf() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Viewer);
    let b = login(&app, "kerry", "pw-kerry-123").await;
    let req = b
        .cookie_only(Request::delete("/api/session"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, req).await.status, 403);
    let wrong = b
        .cookie_only(Request::delete("/api/session"))
        .header("x-csrf-token", "ss_cs_wrong");
    assert_eq!(
        send(&app, wrong.body(Body::empty()).unwrap()).await.status,
        403
    );
    let ok = send(
        &app,
        b.apply(Request::delete("/api/session"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(ok.status, 204);
    assert!(
        ok.headers
            .get_all("set-cookie")
            .iter()
            .all(|v| v.to_str().unwrap().contains("Max-Age=0"))
    );
    assert_eq!(send(&app, get(b, "/api/session")).await.status, 401);
}

#[tokio::test]
async fn bearer_callers_cannot_delete_a_session() {
    let (app, state) = test_app();
    let req = authorize(Request::delete("/api/session"), &state, Role::Viewer)
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, req).await.status, 400);
}

#[tokio::test]
async fn login_failures_are_uniform_and_rate_limited() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Viewer);
    create_user(&state, "off", "pw-off-1234", Role::Viewer);
    create_user(&state, "boss", "pw-boss-123", Role::Admin);
    state.auth.update_user("off", None, Some(true)).unwrap();
    let unknown = post_login(&app, "ghost", "whatever-123").await;
    let wrong = post_login(&app, "kerry", "not-the-password").await;
    let disabled = post_login(&app, "off", "pw-off-1234").await;
    let malformed = post_login(&app, "Not A User!", "whatever-123").await;
    for r in [&unknown, &wrong, &disabled, &malformed] {
        assert_eq!(r.status, 401);
    }
    assert_eq!(unknown.body, wrong.body);
    assert_eq!(wrong.body, disabled.body);
    assert_eq!(wrong.body, malformed.body);
    for _ in 0..4 {
        assert_eq!(post_login(&app, "kerry", "nope-nope-1").await.status, 401);
    }
    assert_eq!(post_login(&app, "kerry", "pw-kerry-123").await.status, 429);
    // A different user from the same address is not blocked.
    assert_eq!(post_login(&app, "boss", "pw-boss-123").await.status, 200);
    state.auth.clock().advance(901);
    assert_eq!(post_login(&app, "kerry", "pw-kerry-123").await.status, 200);
}

#[tokio::test]
async fn login_limiter_does_not_grow_for_malformed_usernames() {
    let (app, _) = test_app();
    for i in 0..50 {
        let r = post_login(&app, &format!("bad name {i}"), "whatever-123").await;
        assert_eq!(r.status, 401);
    }
    let long = "a".repeat(5000);
    assert_eq!(post_login(&app, &long, "whatever-123").await.status, 401);
    // None of those blocked a real user.
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Viewer);
    for _ in 0..10 {
        post_login(&app, "Not A User!", "x").await;
    }
    assert_eq!(post_login(&app, "kerry", "pw-kerry-123").await.status, 200);
}

#[tokio::test]
async fn disabled_user_session_fails_on_next_request() {
    let (app, state) = test_app();
    create_user(&state, "boss", "pw-boss-123", Role::Admin);
    create_user(&state, "v", "pw-v-123456", Role::Viewer);
    let b = login(&app, "v", "pw-v-123456").await;
    state.auth.update_user("v", None, Some(true)).unwrap();
    assert_eq!(send(&app, get(b, "/api/me")).await.status, 401);
}

#[tokio::test]
async fn session_expires_after_seven_days() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Viewer);
    let b = login(&app, "kerry", "pw-kerry-123").await;
    state.auth.clock().advance(7 * 86400 - 1);
    assert_eq!(send(&app, get(b.copy(), "/api/session")).await.status, 200);
    state.auth.clock().advance(2);
    assert_eq!(send(&app, get(b, "/api/session")).await.status, 401);
}

#[tokio::test]
async fn bearer_header_wins_over_cookie() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Viewer);
    let b = login(&app, "kerry", "pw-kerry-123").await;
    let req = b
        .cookie_only(Request::get("/api/me"))
        .header("authorization", "Bearer ss_at_bad")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, req).await.status, 401);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_bad_logins_are_limited_to_five_evaluations() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Operator);
    let mut tasks = Vec::new();
    for _ in 0..50 {
        let app = app.clone();
        tasks.push(tokio::spawn(async move {
            post_login(&app, "kerry", "wrong-password").await.status
        }));
    }
    let mut unauthorized = 0;
    let mut limited = 0;
    for t in tasks {
        match t.await.unwrap().as_u16() {
            401 => unauthorized += 1,
            429 => limited += 1,
            other => panic!("unexpected status {other}"),
        }
    }
    assert!(unauthorized <= 5, "{unauthorized} guesses were evaluated");
    assert_eq!(unauthorized + limited, 50);
}

/// Holds every argon2 permit, so any request that needs argon2 must wait.
async fn hold_all_permits(state: &starstar_api::AppState) -> tokio::sync::OwnedSemaphorePermit {
    let sem = state.auth.argon_semaphore();
    let n = sem.available_permits();
    assert!((1..=4).contains(&n), "{n} permits");
    sem.acquire_many_owned(n as u32).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn argon2_work_waits_for_a_global_permit() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Operator);
    let held = hold_all_permits(&state).await;
    let task = {
        let app = app.clone();
        tokio::spawn(async move { post_login(&app, "kerry", "pw-kerry-123").await.status })
    };
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    assert!(!task.is_finished(), "login ran argon2 without a permit");
    drop(held);
    assert_eq!(task.await.unwrap(), 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn waiting_too_long_for_a_permit_is_a_503_with_retry_after() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Operator);
    state
        .auth
        .set_argon_wait(std::time::Duration::from_millis(100));
    let held = hold_all_permits(&state).await;
    let started = std::time::Instant::now();
    let r = post_login(&app, "kerry", "pw-kerry-123").await;
    assert_eq!(r.status, 503, "{}", r.body);
    assert_eq!(r.headers.get("retry-after").unwrap(), "5");
    assert!(started.elapsed() < std::time::Duration::from_secs(3));
    drop(held);
    // The timed-out attempt counted against the limiter, but no password
    // work ran; with a permit free the same login now succeeds.
    assert_eq!(post_login(&app, "kerry", "pw-kerry-123").await.status, 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unknown_users_also_wait_for_a_permit() {
    let (app, state) = test_app();
    let held = hold_all_permits(&state).await;
    let task = {
        let app = app.clone();
        tokio::spawn(async move { post_login(&app, "ghost", "whatever-pw").await.status })
    };
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    assert!(!task.is_finished());
    drop(held);
    assert_eq!(task.await.unwrap(), 401);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn overlong_passwords_are_refused_before_any_argon2_work() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Operator);
    let _held = hold_all_permits(&state).await;
    let long = "x".repeat(257);
    let r = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        post_login(&app, "kerry", &long),
    )
    .await
    .expect("must not wait for argon2");
    assert_eq!(r.status, 401);
    let ok = "x".repeat(256);
    let r = tokio::time::timeout(
        std::time::Duration::from_millis(150),
        post_login(&app, "kerry", &ok),
    )
    .await;
    assert!(r.is_err(), "a 256-char password does need argon2");
}

#[tokio::test]
async fn per_ip_login_limiter_stops_username_rotation() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Operator);
    for i in 0..30 {
        let r = post_login(&app, &format!("ghost{i}"), "whatever-pw").await;
        assert_eq!(r.status, 401, "attempt {i}");
    }
    let r = post_login(&app, "kerry", "pw-kerry-123").await;
    assert_eq!(r.status, 429);
    state.auth.clock().advance(901);
    assert_eq!(post_login(&app, "kerry", "pw-kerry-123").await.status, 200);
}

#[tokio::test]
async fn successful_logins_do_not_use_up_the_per_ip_budget() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Operator);
    for _ in 0..40 {
        assert_eq!(post_login(&app, "kerry", "pw-kerry-123").await.status, 200);
    }
}
