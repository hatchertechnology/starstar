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
    let forwarded = parts
        .headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .filter_map(|s| s.trim().parse::<IpAddr>().ok())
        .collect::<Vec<_>>();
    forwarded
        .into_iter()
        .rev()
        .find(|ip| !trusted(*ip, trusted_proxies))
        .unwrap_or(peer)
        .to_string()
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
    fn rightmost_untrusted_forwarded_address_wins_from_trusted_peer() {
        let net: Vec<IpNet> = vec!["10.0.0.0/8".parse().unwrap()];
        let p = parts(Some("10.0.0.2:80"), Some("6.6.6.6, 198.51.100.7, 10.0.0.1"));
        assert_eq!(client_ip(&p, &net), "198.51.100.7");
        let p = parts(Some("10.0.0.2:80"), None);
        assert_eq!(client_ip(&p, &net), "10.0.0.2");
    }
}
