//! The client address used as a rate-limit key.

use std::net::{IpAddr, SocketAddr};

use axum::extract::ConnectInfo;
use axum::http::request::Parts;
use ipnet::IpNet;

fn trusted(ip: IpAddr, proxies: &[IpNet]) -> bool {
    proxies.iter().any(|net| net.contains(&ip))
}

/// The client's IP. It is the socket peer, unless the peer is one of
/// `trusted_proxies`; then it is the rightmost `X-Forwarded-For` address that
/// is not itself a trusted proxy. `"unknown"` when there is no socket info
/// (in-process tests).
pub fn client_ip(parts: &Parts, trusted_proxies: &[IpNet]) -> String {
    let Some(peer) = parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip())
    else {
        return "unknown".to_owned();
    };
    if !trusted(peer, trusted_proxies) {
        return peer.to_string();
    }
    // Walk from the right, the end our own proxies appended to. An entry that
    // does not parse (`ip:port`, junk, non-ASCII) ends the walk: everything
    // left of it is attacker-controlled, so the peer is used.
    let mut entries = Vec::new();
    for value in parts.headers.get_all("x-forwarded-for") {
        let Ok(value) = value.to_str() else {
            return peer.to_string();
        };
        entries.extend(value.split(',').map(str::trim));
    }
    for entry in entries.into_iter().rev() {
        match entry.parse::<IpAddr>() {
            Ok(ip) if trusted(ip, trusted_proxies) => continue,
            Ok(ip) => return ip.to_string(),
            Err(_) => return peer.to_string(),
        }
    }
    peer.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    fn parts(peer: Option<&str>, xff: Option<&str>) -> Parts {
        let mut req = Request::builder();
        if let Some(x) = xff {
            req = req.header("x-forwarded-for", x);
        }
        let mut req = req.body(()).unwrap();
        if let Some(p) = peer {
            req.extensions_mut()
                .insert(ConnectInfo(p.parse::<SocketAddr>().unwrap()));
        }
        req.into_parts().0
    }

    #[test]
    fn no_socket_info_is_unknown() {
        assert_eq!(client_ip(&parts(None, None), &[]), "unknown");
    }

    #[test]
    fn forwarded_header_is_ignored_from_untrusted_peers() {
        let p = parts(Some("203.0.113.9:1234"), Some("1.2.3.4"));
        assert_eq!(client_ip(&p, &[]), "203.0.113.9");
        let net: Vec<IpNet> = vec!["10.0.0.0/8".parse().unwrap()];
        assert_eq!(client_ip(&p, &net), "203.0.113.9");
    }

    #[test]
    fn an_unparseable_entry_stops_the_walk_and_the_peer_is_used() {
        let net: Vec<IpNet> = vec!["10.0.0.0/8".parse().unwrap()];
        for xff in [
            "6.6.6.6, 198.51.100.7:4711",
            "6.6.6.6, [2001:db8::1]:80",
            "6.6.6.6, unknown",
            "6.6.6.6, ",
            "6.6.6.6, 198.51.100.7:4711, 10.0.0.1",
        ] {
            let p = parts(Some("10.0.0.2:80"), Some(xff));
            assert_eq!(client_ip(&p, &net), "10.0.0.2", "{xff}");
        }
        // Valid entries to the right of the bad one are still honoured.
        let p = parts(Some("10.0.0.2:80"), Some("6.6.6.6, junk, 198.51.100.7"));
        assert_eq!(client_ip(&p, &net), "198.51.100.7");
    }

    #[test]
    fn rightmost_untrusted_forwarded_address_wins_from_trusted_peer() {
        let net: Vec<IpNet> = vec!["10.0.0.0/8".parse().unwrap()];
        let p = parts(Some("10.0.0.2:80"), Some("6.6.6.6, 198.51.100.7, 10.0.0.1"));
        assert_eq!(client_ip(&p, &net), "198.51.100.7");
        let p = parts(Some("10.0.0.2:80"), None);
        assert_eq!(client_ip(&p, &net), "10.0.0.2");
    }

    #[test]
    fn loopback_bind_defaults_believe_a_local_proxy() {
        let config = crate::config::Config::from_lookup(|_| None);
        let p = parts(Some("127.0.0.1:5000"), Some("203.0.113.9"));
        assert_eq!(client_ip(&p, &config.trusted_proxies), "203.0.113.9");
        let p = parts(Some("[::1]:5000"), Some("203.0.113.9"));
        assert_eq!(client_ip(&p, &config.trusted_proxies), "203.0.113.9");
    }

    #[test]
    fn non_loopback_bind_with_unset_proxies_ignores_the_header() {
        let config = crate::config::Config::from_lookup(|k| {
            (k == "STARSTAR_BIND").then(|| "0.0.0.0:8080".to_owned())
        });
        let p = parts(Some("192.168.1.7:5000"), Some("203.0.113.9"));
        assert_eq!(client_ip(&p, &config.trusted_proxies), "192.168.1.7");
    }
}
