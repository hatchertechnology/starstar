//! Bearer-token authentication: effective role, expiry, WWW-Authenticate.

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use serde_json::json;
use starstar_api::auth::crypto::hash_token;
use starstar_shared::Role;

mod common;
use common::*;

async fn me(app: &Router, token: Option<&str>) -> TestResponse {
    let mut b = Request::get("/api/me");
    if let Some(t) = token {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    send(app, b.body(Body::empty()).unwrap()).await
}

#[tokio::test]
async fn me_with_valid_token() {
    let (app, state) = test_app();
    let token = token_for(&state, "op", Role::Operator);
    let r = me(&app, Some(&token)).await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json(), json!({"username": "op", "role": "operator"}));
}

#[tokio::test]
async fn missing_or_bad_token_is_401_with_resource_metadata() {
    let (app, _) = test_app();
    let r = me(&app, None).await;
    assert_eq!(r.status, 401);
    assert_eq!(
        r.headers["www-authenticate"],
        r#"Bearer resource_metadata="http://127.0.0.1:8080/.well-known/oauth-protected-resource""#
    );
    // A token was sent and rejected: RFC 6750 section 3 adds the error code.
    let r = me(&app, Some("ss_at_nope")).await;
    assert_eq!(r.status, 401);
    assert_eq!(
        r.headers["www-authenticate"],
        r#"Bearer error="invalid_token", resource_metadata="http://127.0.0.1:8080/.well-known/oauth-protected-resource""#
    );
    let r = send(
        &app,
        Request::get("/api/me")
            .header("authorization", "Basic Zm9vOmJhcg==")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, 401);
}

#[tokio::test]
async fn too_low_a_role_is_403_with_an_insufficient_scope_challenge() {
    let (app, state) = test_app();
    let viewer = token_for(&state, "v", Role::Viewer);
    let req = |path: &str| {
        Request::get(path)
            .header("authorization", format!("Bearer {viewer}"))
            .body(Body::empty())
            .unwrap()
    };
    let r = send(&app, req("/api/users")).await;
    assert_eq!(r.status, 403);
    assert_eq!(
        r.headers["www-authenticate"],
        r#"Bearer error="insufficient_scope", resource_metadata="http://127.0.0.1:8080/.well-known/oauth-protected-resource""#
    );
    // A 403 that is not about the role carries no challenge.
    let r = send(
        &app,
        Request::put("/api/me/password")
            .header("authorization", format!("Bearer {viewer}"))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"current_password": "wrong-one", "new_password": "another-pass-1"})
                    .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, 403);
    assert!(r.headers.get("www-authenticate").is_none());
}

#[tokio::test]
async fn bearer_scheme_is_case_insensitive() {
    let (app, state) = test_app();
    let token = token_for(&state, "v", Role::Viewer);
    let r = send(
        &app,
        Request::get("/api/me")
            .header("authorization", format!("bEaReR {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, 200);
}

#[tokio::test]
async fn mcp_challenge_points_at_the_mcp_metadata_document() {
    let (app, _) = test_app();
    for path in ["/mcp", "/mcp/anything"] {
        let r = send(&app, Request::get(path).body(Body::empty()).unwrap()).await;
        assert_eq!(r.status, 401, "{path}");
        assert_eq!(
            r.headers["www-authenticate"],
            r#"Bearer resource_metadata="http://127.0.0.1:8080/.well-known/oauth-protected-resource/mcp""#
        );
    }
}

#[tokio::test]
async fn disabled_user_token_fails_on_next_request() {
    let (app, state) = test_app();
    create_user(&state, "boss", "pw-boss-123", Role::Admin);
    let token = token_for(&state, "v", Role::Viewer);
    assert_eq!(me(&app, Some(&token)).await.status, 200);
    state.auth.update_user("v", None, Some(true)).unwrap();
    assert_eq!(me(&app, Some(&token)).await.status, 401);
}

#[tokio::test]
async fn disabled_user_token_fails_on_any_guarded_path_including_mcp() {
    let (app, state) = test_app();
    create_user(&state, "boss", "pw-boss-123", Role::Admin);
    let token = token_for(&state, "v", Role::Viewer);
    let get = |path: &str| {
        Request::get(path.to_owned())
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    };
    // An enabled user gets past the layer (no route yet is a 404, not a 401).
    assert_ne!(send(&app, get("/mcp")).await.status, 401);
    state.auth.update_user("v", None, Some(true)).unwrap();
    for path in ["/mcp", "/api/me", "/api/printers"] {
        assert_eq!(send(&app, get(path)).await.status, 401, "{path}");
    }
}

#[tokio::test]
async fn demoted_user_token_is_capped() {
    let (app, state) = test_app();
    create_user(&state, "boss", "pw-boss-123", Role::Admin);
    let token = token_for(&state, "a2", Role::Admin);
    state
        .auth
        .update_user("a2", Some(Role::Viewer), None)
        .unwrap();
    assert_eq!(me(&app, Some(&token)).await.json()["role"], "viewer");
}

#[tokio::test]
async fn grant_role_caps_user_role() {
    let (app, state) = test_app();
    create_user(&state, "a", "pw-a-12345", Role::Admin);
    let user = state.auth.user_by_name("a").unwrap().unwrap();
    let g = state
        .auth
        .create_grant(user.id, "starstar-cli", Role::Viewer, None)
        .unwrap();
    let t = state.auth.issue_tokens(&g).unwrap();
    assert_eq!(t.expires_in, 3600);
    assert_eq!(
        me(&app, Some(&t.access_token)).await.json()["role"],
        "viewer"
    );
}

#[tokio::test]
async fn access_token_expires_after_an_hour_and_grant_after_90_days() {
    let (app, state) = test_app();
    let token = token_for(&state, "v", Role::Viewer);
    state.auth.clock().advance(3599);
    assert_eq!(me(&app, Some(&token)).await.status, 200);
    state.auth.clock().advance(2);
    assert_eq!(me(&app, Some(&token)).await.status, 401);
    let user = state.auth.user_by_name("v").unwrap().unwrap();
    let g = state
        .auth
        .create_grant(user.id, "starstar-cli", Role::Viewer, None)
        .unwrap();
    state.auth.clock().advance(90 * 86400 - 10);
    let t = state.auth.issue_tokens(&g).unwrap();
    assert_eq!(me(&app, Some(&t.access_token)).await.status, 200);
    state.auth.clock().advance(11);
    assert_eq!(me(&app, Some(&t.access_token)).await.status, 401);
}

#[tokio::test]
async fn tokens_are_stored_hashed() {
    let (_, state) = test_app();
    let token = token_for(&state, "v", Role::Viewer);
    let hashes = state.auth.token_hashes();
    assert!(hashes.iter().all(|h| h.as_slice() != token.as_bytes()));
    assert!(hashes.contains(&hash_token(&token).to_vec()));
}

#[tokio::test]
async fn every_response_forbids_framing() {
    let (app, _) = test_app();
    for path in ["/api/health", "/api/me", "/nothing-here"] {
        let r = send(&app, Request::get(path).body(Body::empty()).unwrap()).await;
        assert_eq!(r.headers["x-frame-options"], "DENY", "{path}");
        assert_eq!(
            r.headers["content-security-policy"], "frame-ancestors 'none'",
            "{path}"
        );
    }
}

#[tokio::test]
async fn deleting_a_user_cascades_to_token_rows() {
    let (_, state) = test_app();
    create_user(&state, "boss", "pw-boss-123", Role::Admin);
    token_for(&state, "v", Role::Viewer);
    assert_eq!(state.auth.token_hashes().len(), 2); // access + refresh
    state.auth.remove_user("v").unwrap();
    assert!(state.auth.token_hashes().is_empty());
}
