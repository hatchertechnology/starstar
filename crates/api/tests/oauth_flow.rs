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

const COPY: &str = "http://127.0.0.1:8080/oauth/code";

/// A real code issued to `starstar-cli` for `username`, through authorize + consent.
async fn cli_code(
    app: &Router,
    state: &starstar_api::AppState,
    username: &str,
) -> (Browser, String) {
    create_user(state, username, "pw-copy-1234", Role::Admin);
    let b = login(app, username, "pw-copy-1234").await;
    let (_, ch) = pkce();
    let code = authorize_code(
        app,
        &b,
        "starstar-cli",
        COPY,
        &ch,
        "starstar:viewer",
        Role::Viewer,
    )
    .await;
    (b, code)
}

#[tokio::test]
async fn copy_code_page() {
    let (app, state) = test_app();
    let (b, code) = cli_code(&app, &state, "boss").await;
    let r = get_as(&app, &b, &format!("/oauth/code?code={code}&state=x")).await;
    assert_eq!(r.status, 200);
    assert!(
        r.headers["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    assert!(r.body.contains(&code));
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

#[tokio::test]
async fn copy_code_page_shows_nothing_for_foreign_or_unknown_codes() {
    let (app, state) = test_app();
    let (_, code) = cli_code(&app, &state, "boss").await;
    create_user(&state, "other", "pw-other-123", Role::Admin);
    let other = login(&app, "other", "pw-other-123").await;
    // Another user's code.
    let r = get_as(&app, &other, &format!("/oauth/code?code={code}")).await;
    assert!(!r.body.contains(&code), "{}", r.body);
    // A made-up value, with markup.
    let r = get_as(&app, &other, "/oauth/code?code=ss_ac_abc%3Cb%3E").await;
    assert!(
        !r.body.contains("ss_ac_abc") && !r.body.contains("<b>"),
        "{}",
        r.body
    );

    // A code that belongs to another client, even for the same user.
    let id = register_client(&state, "http://127.0.0.1:1/cb");
    let (_, ch) = pkce();
    let b = login(&app, "boss", "pw-copy-1234").await;
    let foreign = authorize_code(
        &app,
        &b,
        &id,
        "http://127.0.0.1:2/cb",
        &ch,
        "starstar:viewer",
        Role::Viewer,
    )
    .await;
    let r = get_as(&app, &b, &format!("/oauth/code?code={foreign}")).await;
    assert!(!r.body.contains(&foreign), "{}", r.body);
}

#[tokio::test]
async fn copy_code_page_hides_used_and_expired_codes() {
    let (app, state) = test_app();
    let (b, code) = cli_code(&app, &state, "boss").await;
    state.auth.clock().advance(61);
    let r = get_as(&app, &b, &format!("/oauth/code?code={code}")).await;
    assert!(!r.body.contains(&code), "{}", r.body);

    let (verifier, ch) = pkce();
    let code = authorize_code(
        &app,
        &b,
        "starstar-cli",
        COPY,
        &ch,
        "starstar:viewer",
        Role::Viewer,
    )
    .await;
    let t = token(
        &app,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("client_id", "starstar-cli"),
            ("redirect_uri", COPY),
            ("code_verifier", &verifier),
        ],
    )
    .await;
    assert_eq!(t.status, 200, "{}", t.body);
    let r = get_as(&app, &b, &format!("/oauth/code?code={code}")).await;
    assert!(!r.body.contains(&code), "{}", r.body);
}

#[tokio::test]
async fn authorize_rejects_loopback_aliases_of_own_origin() {
    let (app, state) = test_app();
    let (_, ch) = pkce();
    for host in ["localhost", "[::1]"] {
        // A hostile client cannot get a redirect onto our listener under another loopback name.
        let id = register_client(&state, &format!("http://{host}:1/oauth/code"));
        let r = get(
            &app,
            &authorize_uri(&id, &format!("http://{host}:8080/oauth/code"), &ch, ""),
        )
        .await;
        assert_eq!(r.status, 400, "{host}: {}", r.body);
        assert!(r.headers.get("location").is_none());
        // Not even on its own port: the copy-code path is reserved for starstar-cli.
        let r = get(
            &app,
            &authorize_uri(&id, &format!("http://{host}:1/oauth/code"), &ch, ""),
        )
        .await;
        assert_eq!(r.status, 400, "{host}: {}", r.body);
        // The CLI's loopback callback on our own port is refused too.
        let r = get(
            &app,
            &authorize_uri(
                "starstar-cli",
                &format!("http://{host}:8080/callback"),
                &ch,
                "",
            ),
        )
        .await;
        assert_eq!(r.status, 400, "{host}: {}", r.body);
        // And it cannot be registered.
        let r = post_json(
            &app,
            "/oauth/register",
            serde_json::json!({"redirect_uris": [format!("http://{host}:8080/cb")]}),
        )
        .await;
        assert_eq!(r.status, 400, "{host}: {}", r.body);
    }
}

// ---- token endpoint, refresh and revocation ----

use std::time::Duration;

async fn token(app: &Router, form: &[(&str, &str)]) -> TestResponse {
    post_form(app, "/oauth/token", form).await
}

async fn bearer_me(app: &Router, at: &str) -> TestResponse {
    send(
        app,
        Request::get("/api/me")
            .header("authorization", format!("Bearer {at}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

async fn refresh(app: &Router, rt: &str) -> TestResponse {
    refresh_as(app, rt, "starstar-cli").await
}

async fn refresh_as(app: &Router, rt: &str, client: &str) -> TestResponse {
    token(
        app,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", rt),
            ("client_id", client),
        ],
    )
    .await
}

fn pair(r: &TestResponse) -> (String, String) {
    let j = r.json();
    (
        j["access_token"].as_str().unwrap().to_owned(),
        j["refresh_token"].as_str().unwrap().to_owned(),
    )
}

/// Signs in an admin "kerry", runs the CLI flow and returns (app, state,
/// browser, access, refresh).
async fn cli_pair() -> (Router, starstar_api::AppState, Browser, String, String) {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Admin);
    let b = login(&app, "kerry", "pw-kerry-123").await;
    let (verifier, challenge) = pkce();
    let code = authorize_code(
        &app,
        &b,
        "starstar-cli",
        CB,
        &challenge,
        "starstar:operator",
        Role::Operator,
    )
    .await;
    let r = token(
        &app,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", CB),
            ("client_id", "starstar-cli"),
            ("code_verifier", &verifier),
        ],
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.body);
    let (at, rt) = pair(&r);
    (app, state, b, at, rt)
}

#[tokio::test]
async fn pkce_flow_end_to_end() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Admin);
    let b = login(&app, "kerry", "pw-kerry-123").await;
    let (verifier, challenge) = pkce();
    let code = authorize_code(
        &app,
        &b,
        "starstar-cli",
        CB,
        &challenge,
        "starstar:operator",
        Role::Operator,
    )
    .await;
    let r = token(
        &app,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", CB),
            ("client_id", "starstar-cli"),
            ("code_verifier", &verifier),
        ],
    )
    .await;
    assert_eq!(r.status, 200);
    assert_eq!(r.headers["cache-control"], "no-store");
    assert_eq!(r.headers["pragma"], "no-cache");
    let j = r.json();
    assert_eq!(j["token_type"], "Bearer");
    assert_eq!(j["expires_in"], 3600);
    assert_eq!(j["scope"], "starstar:operator");
    assert!(j["access_token"].as_str().unwrap().starts_with("ss_at_"));
    assert!(j["refresh_token"].as_str().unwrap().starts_with("ss_rt_"));
    assert_eq!(
        bearer_me(&app, j["access_token"].as_str().unwrap())
            .await
            .json()["role"],
        "operator"
    );
}

#[tokio::test]
async fn code_exchange_rejections() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Admin);
    let b = login(&app, "kerry", "pw-kerry-123").await;
    let other = register_client(&state, "http://127.0.0.1:1/cb");
    let fresh = || async {
        let (v, c) = pkce();
        let code = authorize_code(&app, &b, "starstar-cli", CB, &c, "", Role::Operator).await;
        (v, code)
    };
    let form = |code: &str, redirect: &str, client: &str, verifier: &str| {
        let f: Vec<(String, String)> = vec![
            ("grant_type".into(), "authorization_code".into()),
            ("code".into(), code.into()),
            ("redirect_uri".into(), redirect.into()),
            ("client_id".into(), client.into()),
            ("code_verifier".into(), verifier.into()),
        ];
        f
    };
    let post = |f: Vec<(String, String)>| {
        let app = app.clone();
        async move {
            let refs: Vec<(&str, &str)> = f.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
            token(&app, &refs).await
        }
    };

    let (_, code) = fresh().await;
    let wrong_verifier = "b".repeat(43);
    let r = post(form(&code, CB, "starstar-cli", &wrong_verifier)).await;
    assert_eq!(r.status, 400);
    assert_eq!(r.json()["error"], "invalid_grant");

    let (v, code) = fresh().await;
    let r = post(form(
        &code,
        "http://127.0.0.1:49153/callback",
        "starstar-cli",
        &v,
    ))
    .await;
    assert_eq!(r.json()["error"], "invalid_grant");

    let (v, code) = fresh().await;
    let r = post(form(&code, CB, &other, &v)).await;
    assert_eq!(r.json()["error"], "invalid_grant");

    let (v, code) = fresh().await;
    let r = post(form(&code, CB, "ss_cl_unknown", &v)).await;
    assert_eq!(r.status, 401);
    assert_eq!(r.json()["error"], "invalid_client");

    let (v, code) = fresh().await;
    state.auth.clock().advance(61);
    let r = post(form(&code, CB, "starstar-cli", &v)).await;
    assert_eq!(r.json()["error"], "invalid_grant");

    let r = token(
        &app,
        &[("grant_type", "password"), ("client_id", "starstar-cli")],
    )
    .await;
    assert_eq!(r.json()["error"], "unsupported_grant_type");
    let r = token(&app, &[("grant_type", "authorization_code")]).await;
    assert_eq!(r.json()["error"], "invalid_request");
}

#[tokio::test]
async fn a_wrong_verifier_burns_the_code() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Admin);
    let b = login(&app, "kerry", "pw-kerry-123").await;
    let (v, c) = pkce();
    let code = authorize_code(&app, &b, "starstar-cli", CB, &c, "", Role::Operator).await;
    let bad = "b".repeat(43);
    let attempt = |verifier: String| {
        let (app, code) = (app.clone(), code.clone());
        async move {
            token(
                &app,
                &[
                    ("grant_type", "authorization_code"),
                    ("code", &code),
                    ("redirect_uri", CB),
                    ("client_id", "starstar-cli"),
                    ("code_verifier", &verifier),
                ],
            )
            .await
        }
    };
    assert_eq!(attempt(bad).await.status, 400);
    // The right verifier no longer works: the code was spent.
    assert_eq!(attempt(v).await.status, 400);
}

#[tokio::test]
async fn code_reuse_revokes_the_grant() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Admin);
    let b = login(&app, "kerry", "pw-kerry-123").await;
    let (v, c) = pkce();
    let code = authorize_code(&app, &b, "starstar-cli", CB, &c, "", Role::Operator).await;
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", CB),
        ("client_id", "starstar-cli"),
        ("code_verifier", v.as_str()),
    ];
    let first = token(&app, &form).await;
    assert_eq!(first.status, 200);
    let (at, _) = pair(&first);
    assert_eq!(bearer_me(&app, &at).await.status, 200);
    assert_eq!(token(&app, &form).await.json()["error"], "invalid_grant");
    assert_eq!(bearer_me(&app, &at).await.status, 401);
}

#[tokio::test]
async fn concurrent_exchanges_of_one_code_yield_exactly_one_success() {
    let (app, state) = test_app();
    create_user(&state, "kerry", "pw-kerry-123", Role::Admin);
    let b = login(&app, "kerry", "pw-kerry-123").await;
    let (v, c) = pkce();
    let code = authorize_code(&app, &b, "starstar-cli", CB, &c, "", Role::Operator).await;
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", CB),
        ("client_id", "starstar-cli"),
        ("code_verifier", v.as_str()),
    ];
    let both = async { tokio::join!(token(&app, &form), token(&app, &form)) };
    let (a, b2) = tokio::time::timeout(Duration::from_secs(5), both)
        .await
        .expect("exchange hung");
    let ok = [a.status.as_u16(), b2.status.as_u16()]
        .iter()
        .filter(|s| **s == 200)
        .count();
    assert_eq!(ok, 1);
}

#[tokio::test]
async fn refresh_rotates_tokens() {
    let (app, _, _, at1, rt1) = cli_pair().await;
    let r = refresh(&app, &rt1).await;
    assert_eq!(r.status, 200);
    let (at2, rt2) = pair(&r);
    assert_ne!(rt1, rt2);
    assert_eq!(bearer_me(&app, &at2).await.status, 200);
    assert_eq!(bearer_me(&app, &at1).await.status, 200); // old access token lives until it expires
    assert_eq!(refresh(&app, &rt2).await.status, 200);
}

#[tokio::test]
async fn concurrent_refreshes_both_succeed() {
    let (app, state, _, _, rt) = cli_pair().await;
    let both = async { tokio::join!(refresh(&app, &rt), refresh(&app, &rt)) };
    let (a, b) = tokio::time::timeout(Duration::from_secs(5), both)
        .await
        .expect("refresh hung");
    assert_eq!((a.status.as_u16(), b.status.as_u16()), (200, 200));
    let (at_a, rt_a) = pair(&a);
    let (at_b, rt_b) = pair(&b);
    assert_ne!(at_a, at_b);
    assert_eq!(bearer_me(&app, &at_a).await.status, 200);
    assert_eq!(bearer_me(&app, &at_b).await.status, 200);
    assert_eq!(refresh(&app, &rt_a).await.status, 200);
    assert_eq!(refresh(&app, &rt_b).await.status, 200);
    assert_eq!(state.auth.list_grants(None).unwrap().len(), 1);
}

#[tokio::test]
async fn refresh_reuse_within_grace_mints_new_pair() {
    let (app, state, _, _, rt1) = cli_pair().await;
    let (at_b, _) = pair(&refresh(&app, &rt1).await);
    state.auth.clock().advance(30);
    let c = refresh(&app, &rt1).await;
    assert_eq!(c.status, 200);
    let (at_c, _) = pair(&c);
    assert_ne!(at_b, at_c);
    assert_eq!(bearer_me(&app, &at_b).await.status, 200);
    assert_eq!(bearer_me(&app, &at_c).await.status, 200);
}

#[tokio::test]
async fn a_second_grace_replay_revokes_the_grant() {
    let (app, state, _, _, rt1) = cli_pair().await;
    let (at_b, _) = pair(&refresh(&app, &rt1).await);
    let (at_c, _) = pair(&refresh(&app, &rt1).await); // the one allowed replay
    let r = refresh(&app, &rt1).await; // a second replay
    assert_eq!(r.json()["error"], "invalid_grant");
    assert_eq!(bearer_me(&app, &at_b).await.status, 401);
    assert_eq!(bearer_me(&app, &at_c).await.status, 401);
    assert!(state.auth.list_grants(None).unwrap().is_empty());
}

#[tokio::test]
async fn refresh_reuse_after_grace_revokes_grant() {
    let (app, state, _, _, rt1) = cli_pair().await;
    let (at_b, rt_b) = pair(&refresh(&app, &rt1).await);
    state.auth.clock().advance(31);
    let r = refresh(&app, &rt1).await;
    assert_eq!(r.status, 400);
    assert_eq!(r.json()["error"], "invalid_grant");
    assert_eq!(bearer_me(&app, &at_b).await.status, 401);
    assert_eq!(refresh(&app, &rt_b).await.json()["error"], "invalid_grant");
}

#[tokio::test]
async fn refresh_respects_user_state_and_client() {
    let (app, state, _, _, rt) = cli_pair().await;
    let other = register_client(&state, "http://127.0.0.1:1/cb");
    assert_eq!(
        refresh_as(&app, &rt, &other).await.json()["error"],
        "invalid_grant"
    );
    // The wrong client did not hurt the original one.
    let (_, rt2) = pair(&refresh(&app, &rt).await);
    create_user(&state, "second", "pw-second-12", Role::Admin);
    state.auth.update_user("kerry", None, Some(true)).unwrap();
    assert_eq!(refresh(&app, &rt2).await.json()["error"], "invalid_grant");
}

#[tokio::test]
async fn refresh_token_expires_after_30_days_unused() {
    let (app, state, _, _, rt) = cli_pair().await;
    state.auth.clock().advance(30 * 86400 + 1);
    assert_eq!(refresh(&app, &rt).await.json()["error"], "invalid_grant");
}

#[tokio::test]
async fn grant_absolute_lifetime_caps_sliding_refresh() {
    let (app, state, _, _, mut rt) = cli_pair().await;
    for day in [29, 29, 29] {
        state.auth.clock().advance(day * 86400);
        let r = refresh(&app, &rt).await;
        assert_eq!(r.status, 200, "{}", r.body);
        rt = pair(&r).1;
    }
    // 87 days in. Three days and a second later the 90-day grant is over.
    state.auth.clock().advance(3 * 86400 + 1);
    assert_eq!(refresh(&app, &rt).await.json()["error"], "invalid_grant");
}

#[tokio::test]
async fn token_endpoint_is_rate_limited() {
    let (app, state, _, _, rt) = cli_pair().await;
    for _ in 0..20 {
        assert_eq!(
            refresh(&app, "ss_rt_bogus").await.json()["error"],
            "invalid_grant"
        );
    }
    assert_eq!(refresh(&app, &rt).await.status, 429);
    state.auth.clock().advance(901);
    assert_eq!(refresh(&app, &rt).await.status, 200);
}

#[tokio::test]
async fn revoke_refresh_kills_grant_and_unknown_is_200() {
    let (app, state, _, at, rt) = cli_pair().await;
    let rev = |tok: &str, client: Option<&str>| {
        let mut f = vec![("token".to_owned(), tok.to_owned())];
        if let Some(c) = client {
            f.push(("client_id".to_owned(), c.to_owned()));
        }
        let app = app.clone();
        async move {
            let refs: Vec<(&str, &str)> = f.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
            post_form(&app, "/oauth/revoke", &refs).await
        }
    };
    // Unknown token and another client's id: 200 and nothing happens.
    assert_eq!(rev("ss_rt_unknown", None).await.status, 200);
    let other = register_client(&state, "http://127.0.0.1:1/cb");
    assert_eq!(rev(&rt, Some(&other)).await.status, 200);
    assert_eq!(bearer_me(&app, &at).await.status, 200);
    // Revoking an access token kills only it.
    let (at2, rt2) = pair(&refresh(&app, &rt).await);
    assert_eq!(rev(&at2, Some("starstar-cli")).await.status, 200);
    assert_eq!(bearer_me(&app, &at2).await.status, 401);
    assert_eq!(refresh(&app, &rt2).await.status, 200);
    // Revoking a refresh token kills the grant.
    assert_eq!(rev(&rt2, Some("starstar-cli")).await.status, 200);
    assert_eq!(bearer_me(&app, &at).await.status, 401);
    assert_eq!(refresh(&app, &rt2).await.json()["error"], "invalid_grant");
    assert_eq!(
        post_form(&app, "/oauth/revoke", &[]).await.json()["error"],
        "invalid_request"
    );
}
