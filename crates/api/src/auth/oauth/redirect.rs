//! Redirect URI rules. Pure functions, so they are easy to test.
//!
//! Loopback URIs match on everything except the port (RFC 8252 section 7.3).
//! No redirect may ever land on starstar's own origin: with the default
//! public URL, a loopback "port-agnostic" match would otherwise reach the
//! copy-code page and let a hostile client harvest codes.

use starstar_shared::auth::{CLI_CLIENT_ID, COPY_CODE_PATH};
use url::{Host, Url};

use crate::auth::store::ClientRecord;

/// `http` with host exactly `127.0.0.1`, `localhost` or `[::1]`.
pub fn is_loopback_http(url: &Url) -> bool {
    url.scheme() == "http"
        && match url.host() {
            Some(Host::Ipv4(ip)) => ip.is_loopback() && ip.octets() == [127, 0, 0, 1],
            Some(Host::Ipv6(ip)) => ip.is_loopback(),
            Some(Host::Domain(d)) => d == "localhost",
            None => false,
        }
}

/// Scheme, host and effective port are all equal to the public URL's.
pub fn same_origin(a: &Url, public_url: &str) -> bool {
    let Ok(b) = Url::parse(public_url) else {
        return false;
    };
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

/// Whether `uri` may be registered by a dynamic client.
pub fn is_registrable(uri: &str, public_url: &str) -> bool {
    let Ok(url) = Url::parse(uri) else {
        return false;
    };
    url.fragment().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && (is_loopback_http(&url) || url.scheme() == "https")
        && !same_origin(&url, public_url)
}

/// Exact match, or both loopback with the same host, path and query.
pub fn redirect_matches(registered: &[String], requested: &str) -> bool {
    let Ok(req) = Url::parse(requested) else {
        return false;
    };
    registered.iter().any(|reg| {
        if reg == requested {
            return true;
        }
        let Ok(reg) = Url::parse(reg) else {
            return false;
        };
        is_loopback_http(&reg)
            && is_loopback_http(&req)
            && req.username().is_empty()
            && req.password().is_none()
            && reg.host_str() == req.host_str()
            && reg.path() == req.path()
            && reg.query() == req.query()
            && req.fragment().is_none()
    })
}

/// Whether `requested` may be used as the redirect for `client`.
pub fn redirect_allowed(client: &ClientRecord, requested: &str, public_url: &str) -> bool {
    if client.id == CLI_CLIENT_ID && requested == format!("{public_url}{COPY_CODE_PATH}") {
        return true;
    }
    let Ok(url) = Url::parse(requested) else {
        return false;
    };
    redirect_matches(&client.redirect_uris, requested) && !same_origin(&url, public_url)
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: &str = "http://127.0.0.1:8080";

    fn client(id: &str, uris: &[&str]) -> ClientRecord {
        ClientRecord {
            id: id.to_owned(),
            name: "x".to_owned(),
            redirect_uris: uris.iter().map(|s| (*s).to_owned()).collect(),
            seeded: id == CLI_CLIENT_ID,
            created_at: 0,
        }
    }

    #[test]
    fn registrable_uris() {
        for ok in [
            "http://127.0.0.1:3000/cb",
            "http://localhost/cb",
            "http://[::1]:9/cb",
            "https://app.example.com/cb",
        ] {
            assert!(is_registrable(ok, P), "{ok}");
        }
        for bad in [
            "http://192.168.1.5/cb",
            "http://localhost.evil.com/cb",
            "https://x/cb#frag",
            "javascript:alert(1)",
            "http://127.0.0.1:8080/oauth/code",
            "http://u:p@127.0.0.1/cb",
            "not a url",
        ] {
            assert!(!is_registrable(bad, P), "{bad}");
        }
        assert!(!is_registrable(
            "https://print.example.com/oauth/code",
            "https://print.example.com"
        ));
    }

    #[test]
    fn loopback_matching_ignores_only_the_port() {
        let reg = vec!["http://127.0.0.1:1234/callback".to_string()];
        assert!(redirect_matches(&reg, "http://127.0.0.1:5555/callback"));
        assert!(!redirect_matches(&reg, "http://127.0.0.1:5555/other"));
        assert!(!redirect_matches(&reg, "http://localhost:5555/callback"));
        assert!(!redirect_matches(
            &reg,
            "http://127.0.0.1:5555/callback?x=1"
        ));
        let https = vec!["https://app.example.com/cb".to_string()];
        assert!(redirect_matches(&https, "https://app.example.com/cb"));
        assert!(!redirect_matches(&https, "https://app.example.com:444/cb"));
    }

    #[test]
    fn cli_client_rules() {
        let cli = client(
            CLI_CLIENT_ID,
            &[
                "http://127.0.0.1/callback",
                "http://localhost/callback",
                "http://[::1]/callback",
            ],
        );
        assert!(redirect_allowed(&cli, "http://127.0.0.1:49152/callback", P));
        assert!(redirect_allowed(&cli, "http://[::1]:49152/callback", P));
        assert!(redirect_allowed(
            &cli,
            "http://127.0.0.1:8080/oauth/code",
            P
        ));
        assert!(!redirect_allowed(&cli, "http://127.0.0.1:8080/callback", P)); // our own origin
    }

    #[test]
    fn copy_code_page_only_for_cli() {
        let other = client("ss_cl_x", &["http://127.0.0.1:1/oauth/code"]);
        assert!(!redirect_allowed(
            &other,
            "http://127.0.0.1:8080/oauth/code",
            P
        ));
        assert!(redirect_allowed(&other, "http://127.0.0.1:2/oauth/code", P));
    }
}
