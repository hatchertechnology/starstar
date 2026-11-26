//! Runtime configuration, read from `STARSTAR_*` environment variables.
//!
//! **Interface contract:** every part reads settings from `AppState::config`.
//! New settings are added here (one field, one env var, one default) rather
//! than read from the environment elsewhere.

use std::env;
use std::net::IpAddr;

use ipnet::IpNet;

/// Settings for one API process. Not `Debug`: it holds the setup token.
#[derive(Clone)]
pub struct Config {
    /// `STARSTAR_DATABASE`, default `starstar.db`.
    pub database: String,
    /// `STARSTAR_BIND`, default `127.0.0.1:8080`.
    pub bind: String,
    /// `STARSTAR_PUBLIC_URL`, default `http://127.0.0.1:8080`, without a
    /// trailing slash. Used as the OAuth issuer and in redirects.
    pub public_url: String,
    /// `STARSTAR_ALLOW_INSECURE_HTTP=1` allows a non-loopback bind with an
    /// http public URL (spec resolution 4). Default false.
    pub allow_insecure_http: bool,
    /// `STARSTAR_SETUP_TOKEN`: fixed first-admin setup code instead of a
    /// random one printed at startup (spec resolution 1).
    pub setup_token: Option<String>,
    /// `STARSTAR_MAX_UPLOAD_MB`, default 512, as bytes.
    pub max_upload_bytes: u64,
    /// `STARSTAR_MCP_MAX_UPLOAD_MB`, default 25, as bytes.
    pub mcp_max_upload_bytes: u64,
    /// `STARSTAR_WEB_DIR`, default `web/.output/public`.
    pub web_dir: String,
    /// `STARSTAR_TRUSTED_PROXIES`: comma-separated CIDRs whose
    /// `X-Forwarded-For` header is believed. Default empty.
    pub trusted_proxies: Vec<IpNet>,
    /// Entries of `STARSTAR_TRUSTED_PROXIES` that are neither an IP address
    /// nor a CIDR. `check_config` refuses to start while this is non-empty.
    pub invalid_trusted_proxies: Vec<String>,
}

/// Canonical form of the public URL, used for the issuer, redirects and
/// comparisons: lowercase scheme and host, no default port, no trailing slash.
/// Input that is not a URL is only trimmed; `check_config` rejects it.
pub fn normalize_public_url(raw: &str) -> String {
    match url::Url::parse(raw.trim()) {
        Ok(url) => url.as_str().trim_end_matches('/').to_owned(),
        Err(_) => raw.trim().trim_end_matches('/').to_owned(),
    }
}

/// Splits a `STARSTAR_TRUSTED_PROXIES` value into valid networks and invalid
/// entries. A bare IP address is a /32 or /128.
pub fn parse_trusted_proxies(value: &str) -> (Vec<IpNet>, Vec<String>) {
    let mut nets = Vec::new();
    let mut invalid = Vec::new();
    for entry in value.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        match entry
            .parse::<IpNet>()
            .or_else(|_| entry.parse::<IpAddr>().map(IpNet::from))
        {
            Ok(net) => nets.push(net),
            Err(_) => invalid.push(entry.to_owned()),
        }
    }
    (nets, invalid)
}

const MIB: u64 = 1024 * 1024;

impl Config {
    /// Reads the environment, falling back to defaults. Invalid numbers fall
    /// back to the default too.
    pub fn from_env() -> Self {
        let var = |k: &str| env::var(k).ok().filter(|v| !v.is_empty());
        let mb = |k: &str, default: u64| {
            var(k)
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(default)
                * MIB
        };
        let defaults = Self::default();
        let (trusted_proxies, invalid_trusted_proxies) = var("STARSTAR_TRUSTED_PROXIES")
            .map(|v| parse_trusted_proxies(&v))
            .unwrap_or_default();
        Self {
            database: var("STARSTAR_DATABASE").unwrap_or(defaults.database),
            bind: var("STARSTAR_BIND").unwrap_or(defaults.bind),
            public_url: var("STARSTAR_PUBLIC_URL")
                .map(|u| normalize_public_url(&u))
                .unwrap_or(defaults.public_url),
            allow_insecure_http: var("STARSTAR_ALLOW_INSECURE_HTTP").is_some_and(|v| v == "1"),
            setup_token: var("STARSTAR_SETUP_TOKEN"),
            max_upload_bytes: mb("STARSTAR_MAX_UPLOAD_MB", 512),
            mcp_max_upload_bytes: mb("STARSTAR_MCP_MAX_UPLOAD_MB", 25),
            web_dir: var("STARSTAR_WEB_DIR").unwrap_or(defaults.web_dir),
            trusted_proxies,
            invalid_trusted_proxies,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database: "starstar.db".to_owned(),
            bind: "127.0.0.1:8080".to_owned(),
            public_url: "http://127.0.0.1:8080".to_owned(),
            allow_insecure_http: false,
            setup_token: None,
            max_upload_bytes: 512 * MIB,
            mcp_max_upload_bytes: 25 * MIB,
            web_dir: "web/.output/public".to_owned(),
            trusted_proxies: Vec::new(),
            invalid_trusted_proxies: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_url_is_normalized_once() {
        assert_eq!(
            normalize_public_url("HTTPS://Print.Example.com/"),
            "https://print.example.com"
        );
        assert_eq!(
            normalize_public_url("https://print.example.com:443"),
            "https://print.example.com"
        );
        assert_eq!(
            normalize_public_url("http://127.0.0.1:8080/"),
            "http://127.0.0.1:8080"
        );
        assert_eq!(normalize_public_url("not a url/"), "not a url");
    }

    #[test]
    fn bare_ips_become_host_networks() {
        let (nets, invalid) = parse_trusted_proxies("10.0.0.1, 192.168.0.0/16 ,::1,fd00::/8");
        assert!(invalid.is_empty());
        let shown: Vec<String> = nets.iter().map(ToString::to_string).collect();
        assert_eq!(
            shown,
            ["10.0.0.1/32", "192.168.0.0/16", "::1/128", "fd00::/8"]
        );
    }

    #[test]
    fn invalid_entries_are_reported_not_dropped() {
        let (nets, invalid) = parse_trusted_proxies("10.0.0.1,nope,10.0.0.0/33,,172.16.0.0/12");
        assert_eq!(nets.len(), 2);
        assert_eq!(invalid, ["nope", "10.0.0.0/33"]);
    }
}
