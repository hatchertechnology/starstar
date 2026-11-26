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

/// Scheme, host and effective port are all equal to the public URL's. When
/// the public URL is itself a loopback address, every loopback name
/// (`127.0.0.1`, `localhost`, `[::1]`) counts as the same host, since they all
/// reach the same listener.
pub fn same_origin(a: &Url, public_url: &str) -> bool {
    let Ok(b) = Url::parse(public_url) else {
        return false;
    };
    let same_host = bare_host(a) == bare_host(&b) || (is_loopback_host(&b) && is_loopback_host(a));
    a.scheme() == b.scheme() && same_host && a.port_or_known_default() == b.port_or_known_default()
}

/// The host without the root-label dot: `print.lan.` is `print.lan`.
fn bare_host(url: &Url) -> Option<&str> {
    url.host_str().map(|h| h.strip_suffix('.').unwrap_or(h))
}

fn is_loopback_host(url: &Url) -> bool {
    match url.host() {
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        Some(Host::Domain(d)) => d.strip_suffix('.').unwrap_or(d) == "localhost",
        None => false,
    }
}

/// Whether `url` would reach this server: the public origin, or any loopback
/// redirect on the port the API listens on (`bind` is `STARSTAR_BIND`). The
/// second rule covers a public URL that names a TLS proxy while the API
/// itself listens on loopback.
pub fn lands_on_self(url: &Url, public_url: &str, bind: &str) -> bool {
    if same_origin(url, public_url) {
        return true;
    }
    let bind_port = bind.parse::<std::net::SocketAddr>().ok().map(|a| a.port());
    is_loopback_host(url) && bind_port.is_some() && url.port_or_known_default() == bind_port
}

/// Whether `uri` may be registered by a dynamic client.
pub fn is_registrable(uri: &str, public_url: &str, bind: &str) -> bool {
    let Ok(url) = Url::parse(uri) else {
        return false;
    };
    url.fragment().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && (is_loopback_http(&url) || url.scheme() == "https")
        && !lands_on_self(&url, public_url, bind)
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
pub fn redirect_allowed(
    client: &ClientRecord,
    requested: &str,
    public_url: &str,
    bind: &str,
) -> bool {
    if client.id == CLI_CLIENT_ID && requested == format!("{public_url}{COPY_CODE_PATH}") {
        return true;
    }
    let Ok(url) = Url::parse(requested) else {
        return false;
    };
    // The copy-code path belongs to starstar-cli alone, on any host.
    url.path() != COPY_CODE_PATH
        && redirect_matches(&client.redirect_uris, requested)
        && !lands_on_self(&url, public_url, bind)
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: &str = "http://127.0.0.1:8080";
    const B: &str = "127.0.0.1:8080";

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
            assert!(is_registrable(ok, P, B), "{ok}");
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
            assert!(!is_registrable(bad, P, B), "{bad}");
        }
        assert!(!is_registrable(
            "https://print.example.com/oauth/code",
            "https://print.example.com",
            B
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
        assert!(redirect_allowed(
            &cli,
            "http://127.0.0.1:49152/callback",
            P,
            B
        ));
        assert!(redirect_allowed(&cli, "http://[::1]:49152/callback", P, B));
        assert!(redirect_allowed(
            &cli,
            "http://127.0.0.1:8080/oauth/code",
            P,
            B
        ));
        assert!(!redirect_allowed(
            &cli,
            "http://127.0.0.1:8080/callback",
            P,
            B
        )); // our own origin
    }

    #[test]
    fn copy_code_page_only_for_cli() {
        let other = client("ss_cl_x", &["http://127.0.0.1:1/oauth/code"]);
        assert!(!redirect_allowed(
            &other,
            "http://127.0.0.1:8080/oauth/code",
            P,
            B
        ));
        assert!(!redirect_allowed(
            &other,
            "http://127.0.0.1:2/oauth/code",
            P,
            B
        ));
        let ok = client("ss_cl_y", &["http://127.0.0.1:1/cb"]);
        assert!(redirect_allowed(&ok, "http://127.0.0.1:2/cb", P, B));
    }

    #[test]
    fn loopback_aliases_and_the_bind_port_are_our_own_origin() {
        let c = client("ss_cl_z", &["http://localhost:1/cb", "http://[::1]:1/cb"]);
        for host in ["localhost", "[::1]", "127.0.0.2"] {
            let uri = format!("http://{host}:8080/cb");
            let url = Url::parse(&uri).unwrap();
            assert!(same_origin(&url, P), "{host}");
            assert!(!is_registrable(&uri, P, B), "{host}");
        }
        assert!(!redirect_allowed(&c, "http://localhost:8080/cb", P, B));
        assert!(redirect_allowed(&c, "http://localhost:9/cb", P, B));
        // A TLS-proxy public URL with the API on loopback port 8080.
        let proxy = "https://print.lan";
        assert!(!is_registrable("http://127.0.0.1:8080/cb", proxy, B));
        assert!(is_registrable("http://127.0.0.1:9090/cb", proxy, B));
        let c = client("ss_cl_w", &["http://127.0.0.1:1/cb"]);
        assert!(!redirect_allowed(&c, "http://127.0.0.1:8080/cb", proxy, B));
        assert!(redirect_allowed(&c, "http://127.0.0.1:9090/cb", proxy, B));
    }

    #[test]
    fn a_trailing_dot_does_not_make_a_different_host() {
        let proxy = "https://print.lan";
        assert!(!is_registrable("https://print.lan./cb", proxy, B));
        assert!(!is_registrable("https://PRINT.LAN./cb", proxy, B));
        let c = client("ss_cl_d", &["https://print.lan./cb"]);
        assert!(!redirect_allowed(&c, "https://print.lan./cb", proxy, B));
        // Other hosts are unaffected.
        assert!(is_registrable("https://app.example.com./cb", proxy, B));
    }
}
