//! Authorize, consent, code page, token endpoint, refresh and revoke.

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use starstar_shared::Role;

mod common;
use common::*;

const CB: &str = "http://127.0.0.1:49152/callback";

fn authorize_uri(client: &str, redirect: &str, challenge: &str, extra: &str) -> String {
    format!(
        "/oauth/authorize?response_type=code&client_id={client}&redirect_uri={}&code_challenge={challenge}&code_challenge_method=S256&state=xyz{extra}",
        urlencode(redirect)
    )
}

async fn get(app: &Router, uri: &str) -> TestResponse {
    send(app, Request::get(uri).body(Body::empty()).unwrap()).await
}

async fn get_as(app: &Router, b: &Browser, uri: &str) -> TestResponse {
    send(
        app,
        b.cookie_only(Request::get(uri))
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

fn location(r: &TestResponse) -> String {
    r.headers["location"].to_str().unwrap().to_owned()
}

fn request_id(r: &TestResponse) -> String {
    location(r)
        .strip_prefix("/consent?request=")
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn authorize_without_session_goes_to_login_then_consent() {
    let (app, _) = test_app();
    let (_, ch) = pkce();
    let r = get(&app, &authorize_uri("starstar-cli", CB, &ch, "")).await;
    assert_eq!(r.status, 303);
    let loc = location(&r);
    assert!(
        loc.starts_with("/login?next=%2Fconsent%3Frequest%3Dss_rq_"),
        "{loc}"
    );
}

#[tokio::test]
async fn consent_request_shows_defaults() {
    let (app, state) = test_app();
    create_user(&state, "boss", "pw-boss-123", Role::Admin);
    create_user(&state, "v", "pw-v-123456", Role::Viewer);
    let (_, ch) = pkce();

    let admin = login(&app, "boss", "pw-boss-123").await;
    let r = get_as(
        &app,
        &admin,
        &authorize_uri("starstar-cli", CB, &ch, "&scope=starstar:admin"),
    )
    .await;
    let id = request_id(&r);
    let c = get_as(&app, &admin, &format!("/oauth/authorize/requests/{id}")).await;
    assert_eq!(c.status, 200);
    let j = c.json();
    assert_eq!(j["client_id"], "starstar-cli");
    assert_eq!(j["client_name_self_declared"], false);
    assert_eq!(j["redirect_host"], "127.0.0.1:49152");
    assert_eq!(j["requested_role"], "admin");
    assert_eq!(j["user_role"], "admin");
    assert_eq!(j["default_role"], "operator");
    assert_eq!(j["max_role"], "admin");

    // A dynamic client is self-declared; a viewer is capped at viewer.
    let dynamic = register_client(&state, "https://app.example.com/cb");
    let viewer = login(&app, "v", "pw-v-123456").await;
    let r = get_as(
        &app,
        &viewer,
        &authorize_uri(&dynamic, "https://app.example.com/cb", &ch, ""),
    )
    .await;
    let c = get_as(
        &app,
        &viewer,
        &format!("/oauth/authorize/requests/{}", request_id(&r)),
    )
    .await;
    let j = c.json();
    assert_eq!(j["client_name_self_declared"], true);
    assert_eq!(j["redirect_host"], "app.example.com");
    assert_eq!(j["default_role"], "viewer");
    assert_eq!(j["max_role"], "viewer");

    // No session: 401. Unknown id: 404.
    assert_eq!(
        get(&app, &format!("/oauth/authorize/requests/{id}"))
            .await
            .status,
        401
    );
    assert_eq!(
        get_as(&app, &admin, "/oauth/authorize/requests/ss_rq_nope")
            .await
            .status,
        404
    );
}

#[tokio::test]
async fn allow_redirects_with_code_and_state() {
    let (app, state) = test_app();
    create_user(&state, "boss", "pw-boss-123", Role::Admin);
    let b = login(&app, "boss", "pw-boss-123").await;
    let (_, ch) = pkce();
    let id = request_id(&get_as(&app, &b, &authorize_uri("starstar-cli", CB, &ch, "")).await);
    let r = browser_form(
        &app,
        &b,
        "/oauth/authorize/decision",
        &[
            ("request", &id),
            ("decision", "allow"),
            ("role", "operator"),
        ],
    )
    .await;
    assert_eq!(r.status, 303);
    let loc = location(&r);
    assert!(loc.starts_with(CB), "{loc}");
    let url = url::Url::parse(&loc).unwrap();
    let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert!(q["code"].starts_with("ss_ac_"));
    assert_eq!(q["state"], "xyz");
    // The request is single use.
    let again = browser_form(
        &app,
        &b,
        "/oauth/authorize/decision",
        &[
            ("request", &id),
            ("decision", "allow"),
            ("role", "operator"),
        ],
    )
    .await;
    assert_eq!(again.status, 404);
}

#[tokio::test]
async fn deny_redirects_with_access_denied() {
    let (app, state) = test_app();
    create_user(&state, "boss", "pw-boss-123", Role::Admin);
    let b = login(&app, "boss", "pw-boss-123").await;
    let (_, ch) = pkce();
    let id = request_id(&get_as(&app, &b, &authorize_uri("starstar-cli", CB, &ch, "")).await);
    let r = browser_form(
        &app,
        &b,
        "/oauth/authorize/decision",
        &[("request", &id), ("decision", "deny")],
    )
    .await;
    assert_eq!(r.status, 303);
    assert_eq!(
        location(&r),
        "http://127.0.0.1:49152/callback?error=access_denied&state=xyz"
    );
}

#[tokio::test]
async fn decision_needs_csrf_and_caps_role() {
    let (app, state) = test_app();
    create_user(&state, "op", "pw-op-123456", Role::Operator);
    let b = login(&app, "op", "pw-op-123456").await;
    let (_, ch) = pkce();
    let new_request = |extra: &'static str| {
        let (app, b, ch) = (app.clone(), b.copy(), ch.clone());
        async move {
            request_id(&get_as(&app, &b, &authorize_uri("starstar-cli", CB, &ch, extra)).await)
        }
    };
    let form_post = |req: Request<Body>| send(&app, req);

    // No CSRF at all: 403.
    let id = new_request("").await;
    let body = format!("request={id}&decision=allow&role=operator");
    let req = b
        .cookie_only(Request::post("/oauth/authorize/decision"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap();
    assert_eq!(form_post(req).await.status, 403);
    // CSRF as a form field (no header): accepted.
    let body = format!(
        "request={id}&decision=allow&role=operator&csrf_token={}",
        urlencode(&b.csrf)
    );
    let req = b
        .cookie_only(Request::post("/oauth/authorize/decision"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap();
    assert_eq!(form_post(req).await.status, 303);

    // An operator cannot grant admin, even when asked for it.
    let id = new_request("&scope=starstar:admin").await;
    let r = browser_form(
        &app,
        &b,
        "/oauth/authorize/decision",
        &[
            ("request", &id),
            ("decision", "allow"),
            ("role", "admin"),
            ("allow_admin", "true"),
        ],
    )
    .await;
    assert_eq!(r.status, 400);
    // A viewer-scoped request cannot be widened to operator.
    let id = new_request("&scope=starstar:viewer").await;
    let r = browser_form(
        &app,
        &b,
        "/oauth/authorize/decision",
        &[
            ("request", &id),
            ("decision", "allow"),
            ("role", "operator"),
        ],
    )
    .await;
    assert_eq!(r.status, 400);
}

#[tokio::test]
async fn admin_needs_explicit_allow_admin() {
    let (app, state) = test_app();
    create_user(&state, "boss", "pw-boss-123", Role::Admin);
    let b = login(&app, "boss", "pw-boss-123").await;
    let (_, ch) = pkce();
    let id = request_id(
        &get_as(
            &app,
            &b,
            &authorize_uri("starstar-cli", CB, &ch, "&scope=starstar:admin"),
        )
        .await,
    );
    let r = browser_form(
        &app,
        &b,
        "/oauth/authorize/decision",
        &[("request", &id), ("decision", "allow"), ("role", "admin")],
    )
    .await;
    assert_eq!(r.status, 400);
    let id = request_id(
        &get_as(
            &app,
            &b,
            &authorize_uri("starstar-cli", CB, &ch, "&scope=starstar:admin"),
        )
        .await,
    );
    let r = browser_form(
        &app,
        &b,
        "/oauth/authorize/decision",
        &[
            ("request", &id),
            ("decision", "allow"),
            ("role", "admin"),
            ("allow_admin", "true"),
        ],
    )
    .await;
    assert_eq!(r.status, 303);
}

#[tokio::test]
async fn authorize_rejects_unregistered_redirect_without_redirecting() {
    let (app, state) = test_app();
    let id = register_client(&state, "http://127.0.0.1:1/cb");
    let (_, ch) = pkce();
    let r = get(
        &app,
        &authorize_uri(&id, "http://127.0.0.1:5/other", &ch, ""),
    )
    .await;
    assert_eq!(r.status, 400);
    assert!(r.headers.get("location").is_none());
    let r = get(&app, &authorize_uri("ss_cl_unknown", CB, &ch, "")).await;
    assert_eq!(r.status, 400);
    assert!(r.headers.get("location").is_none());
    // Even the CLI client cannot use a loopback path it did not register.
    let r = get(
        &app,
        &authorize_uri("starstar-cli", "http://127.0.0.1:5/other", &ch, ""),
    )
    .await;
    assert_eq!(r.status, 400);
}

#[tokio::test]
async fn authorize_rejects_redirect_onto_own_origin() {
    let (app, state) = test_app();
    let id = register_client(&state, "http://127.0.0.1:1/oauth/code");
    let (_, ch) = pkce();
    let r = get(
        &app,
        &authorize_uri(&id, "http://127.0.0.1:8080/oauth/code", &ch, ""),
    )
    .await;
    assert_eq!(r.status, 400);
    assert!(r.headers.get("location").is_none());
    let r = get(
        &app,
        &authorize_uri("starstar-cli", "http://127.0.0.1:8080/oauth/code", &ch, ""),
    )
    .await;
    assert_eq!(r.status, 303);
}

#[tokio::test]
async fn pkce_and_response_type_errors_redirect() {
    let (app, _) = test_app();
    let (_, ch) = pkce();
    let base = |q: &str| {
        format!(
            "/oauth/authorize?client_id=starstar-cli&redirect_uri={}&state=xyz{q}",
            urlencode(CB)
        )
    };

    let r = get(
        &app,
        &base(&format!(
            "&response_type=code&code_challenge={ch}&code_challenge_method=plain"
        )),
    )
    .await;
    assert_eq!(r.status, 303);
    assert!(location(&r).contains("error=invalid_request") && location(&r).contains("state=xyz"));
    let r = get(
        &app,
        &base("&response_type=code&code_challenge_method=S256"),
    )
    .await;
    assert!(location(&r).contains("error=invalid_request"));
    let r = get(
        &app,
        &base(&format!(
            "&response_type=code&code_challenge={}&code_challenge_method=S256",
            "a".repeat(10)
        )),
    )
    .await;
    assert!(location(&r).contains("error=invalid_request"));
    let r = get(
        &app,
        &base(&format!(
            "&response_type=token&code_challenge={ch}&code_challenge_method=S256"
        )),
    )
    .await;
    assert!(location(&r).contains("error=unsupported_response_type"));
    let r = get(
        &app,
        &base(&format!("&code_challenge={ch}&code_challenge_method=S256")),
    )
    .await;
    assert!(location(&r).contains("error=invalid_request"));
    let ok = format!("&response_type=code&code_challenge={ch}&code_challenge_method=S256");
    let r = get(&app, &base(&format!("{ok}&resource=https://evil.example"))).await;
    assert!(location(&r).contains("error=invalid_target"));
    let r = get(
        &app,
        &base(&format!("{ok}&resource=http://127.0.0.1:8080/mcp")),
    )
    .await;
    assert!(location(&r).starts_with("/login"));
    let r = get(
        &app,
        &base(&format!("{ok}&resource=http://127.0.0.1:8080.evil.example")),
    )
    .await;
    assert!(location(&r).contains("error=invalid_target"));
}

#[tokio::test]
async fn pending_request_expires() {
    let (app, state) = test_app();
    create_user(&state, "boss", "pw-boss-123", Role::Admin);
    let b = login(&app, "boss", "pw-boss-123").await;
    let (_, ch) = pkce();
    let id = request_id(&get_as(&app, &b, &authorize_uri("starstar-cli", CB, &ch, "")).await);
    state.auth.clock().advance(601);
    assert_eq!(
        get_as(&app, &b, &format!("/oauth/authorize/requests/{id}"))
            .await
            .status,
        404
    );
}

#[tokio::test]
async fn pending_requests_are_capped() {
    let (app, state) = test_app();
    let (_, ch) = pkce();
    for _ in 0..1000 {
        assert_eq!(
            get(&app, &authorize_uri("starstar-cli", CB, &ch, ""))
                .await
                .status,
            303
        );
    }
    let r = get(&app, &authorize_uri("starstar-cli", CB, &ch, "")).await;
    assert_eq!(r.status, 503);
    assert_eq!(r.json()["error"], "temporarily_unavailable");
    state.auth.clock().advance(601);
    assert_eq!(
        get(&app, &authorize_uri("starstar-cli", CB, &ch, ""))
            .await
            .status,
        303
    );
}

#[tokio::test]
async fn copy_code_page() {
    let (app, state) = test_app();
    create_user(&state, "boss", "pw-boss-123", Role::Admin);
    let b = login(&app, "boss", "pw-boss-123").await;
    let r = get_as(&app, &b, "/oauth/code?code=ss_ac_abc%3Cb%3E&state=x").await;
    assert_eq!(r.status, 200);
    assert!(
        r.headers["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    assert!(r.body.contains("ss_ac_abc&lt;b&gt;") && !r.body.contains("<b>"));
    assert!(r.body.contains("<code id=\"code\">"));
    assert_eq!(r.headers["cache-control"], "no-store");
    assert_eq!(r.headers["referrer-policy"], "no-referrer");
    assert!(
        r.headers["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("frame-ancestors 'none'")
    );
    assert_eq!(get(&app, "/oauth/code?code=x&state=x").await.status, 401);
}
