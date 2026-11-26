//! Cookie parsing and `Set-Cookie` building.

use axum::http::{HeaderMap, HeaderValue, header};
use starstar_shared::auth::{CSRF_COOKIE, SESSION_COOKIE};

use super::store::sessions::{NewSession, SESSION_TTL};
use crate::config::Config;

/// The value of cookie `name`, looking through every `Cookie` header.
pub fn get_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_owned())
}

fn build(config: &Config, name: &str, value: &str, max_age: i64, http_only: bool) -> HeaderValue {
    let mut cookie = format!("{name}={value}; Path=/");
    if http_only {
        cookie.push_str("; HttpOnly");
    }
    cookie.push_str(&format!("; SameSite=Lax; Max-Age={max_age}"));
    if url::Url::parse(&config.public_url).is_ok_and(|u| u.scheme() == "https") {
        cookie.push_str("; Secure");
    }
    HeaderValue::from_str(&cookie).expect("cookie values are visible ASCII")
}

/// The session cookie (HttpOnly) and the CSRF cookie (readable by script).
pub fn session_cookies(config: &Config, session: &NewSession) -> [HeaderValue; 2] {
    [
        build(config, SESSION_COOKIE, &session.token, SESSION_TTL, true),
        build(config, CSRF_COOKIE, &session.csrf_token, SESSION_TTL, false),
    ]
}

/// Both cookies emptied, with `Max-Age=0`.
pub fn clear_session_cookies(config: &Config) -> [HeaderValue; 2] {
    [
        build(config, SESSION_COOKIE, "", 0, true),
        build(config, CSRF_COOKIE, "", 0, false),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_session() -> NewSession {
        NewSession {
            token: "ss_se_abc".into(),
            csrf_token: "ss_cs_def".into(),
        }
    }

    #[test]
    fn get_cookie_finds_values() {
        let mut h = HeaderMap::new();
        h.append(
            header::COOKIE,
            "a=1; starstar_session=x; b=2".parse().unwrap(),
        );
        assert_eq!(get_cookie(&h, "starstar_session").as_deref(), Some("x"));
        let mut h = HeaderMap::new();
        h.append(header::COOKIE, "a=1".parse().unwrap());
        h.append(header::COOKIE, "starstar_session=y".parse().unwrap());
        assert_eq!(get_cookie(&h, "starstar_session").as_deref(), Some("y"));
        assert_eq!(get_cookie(&h, "nope"), None);
    }

    #[test]
    fn secure_follows_the_public_url_scheme() {
        let http = Config::default();
        let https = Config {
            public_url: "https://print.example.com".into(),
            ..Config::default()
        };
        for c in session_cookies(&http, &new_session()) {
            assert!(!c.to_str().unwrap().contains("Secure"));
        }
        for c in session_cookies(&https, &new_session()) {
            assert!(c.to_str().unwrap().contains("; Secure"));
        }
    }

    #[test]
    fn secure_ignores_the_case_of_the_scheme() {
        let https = Config {
            public_url: "HTTPS://print.example.com".into(),
            ..Config::default()
        };
        for c in session_cookies(&https, &new_session()) {
            assert!(c.to_str().unwrap().contains("; Secure"));
        }
    }

    #[test]
    fn only_the_session_cookie_is_http_only() {
        let [session, csrf] = session_cookies(&Config::default(), &new_session());
        assert!(session.to_str().unwrap().contains("HttpOnly"));
        assert!(!csrf.to_str().unwrap().contains("HttpOnly"));
        assert!(session.to_str().unwrap().contains("SameSite=Lax"));
    }

    #[test]
    fn cleared_cookies_expire_immediately() {
        for c in clear_session_cookies(&Config::default()) {
            assert!(c.to_str().unwrap().contains("Max-Age=0"));
        }
    }
}
