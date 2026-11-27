//! Runtime configuration, read from `STARSTAR_*` environment variables.
//!
//! **Interface contract:** every part reads settings from `AppState::config`.
//! New settings are added here (one field, one env var, one default) rather
//! than read from the environment elsewhere.

use std::env;
use std::net::IpAddr;
use std::time::Duration;

use ipnet::IpNet;

/// Settings for one API process. Not `Debug`: it holds database credentials and the setup token.
#[derive(Clone)]
pub struct Config {
    /// `STARSTAR_DATABASE_URL`, required PostgreSQL connection URL.
    pub database_url: Option<String>,
    /// `STARSTAR_DB_MAX_CONNECTIONS`, default 10 (bounded `1..=100`).
    pub db_max_connections: u32,
    /// `STARSTAR_DB_CONNECT_TIMEOUT_SECS`, default 30. Startup retry budget
    /// when PostgreSQL is not yet accepting connections.
    pub db_connect_timeout: Duration,
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
    /// `STARSTAR_PRINTER_TIMEOUT_SECS`, default 10. Per-request timeout for PrusaLink control calls.
    pub printer_timeout: Duration,
    /// `STARSTAR_UPLOAD_TIMEOUT_SECS`, default 3600. Per-request timeout for uploads to a printer.
    pub upload_timeout: Duration,
    /// `STARSTAR_TRUSTED_PROXIES`: comma-separated CIDRs whose
    /// `X-Forwarded-For` header is believed. Default: `127.0.0.1/32` and
    /// `::1/128` when `bind` is a loopback address, otherwise empty. Any
    /// explicit value replaces the default.
    ///
    /// Every hop listed here must **overwrite or append** the real peer
    /// address to `X-Forwarded-For`; only the rightmost untrusted entry is
    /// believed. A hop that passes the client's header through unchanged
    /// (nginx with a bare `proxy_pass`) or that forwards raw TCP (`socat`,
    /// `ssh -L`, HAProxy `mode tcp`, `kubectl port-forward`) lets the client
    /// pick its own rate-limit key, or makes every client share the hop's
    /// address. The per-username sign-in limit holds either way. See the
    /// README, "Deploying behind a proxy".
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

/// Formats a PostgreSQL connection URL for logs without credentials or query
/// parameters (scheme, host, optional port, and database path only).
pub fn redact_database_url(raw: &str) -> String {
    let Ok(url) = url::Url::parse(raw.trim()) else {
        return "<invalid database URL>".to_owned();
    };
    if !matches!(url.scheme(), "postgres" | "postgresql") {
        return "<invalid database URL>".to_owned();
    }
    let Some(host) = url.host_str() else {
        return "<invalid database URL>".to_owned();
    };
    let port = url.port().map_or_else(String::new, |p| format!(":{p}"));
    format!("{}://{host}{port}{}", url.scheme(), url.path())
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
        Self::from_lookup(|k| env::var(k).ok())
    }

    /// Like [`Config::from_env`], reading settings through `lookup`. Empty
    /// values count as unset.
    ///
    /// When `STARSTAR_TRUSTED_PROXIES` is unset and `STARSTAR_BIND` is a
    /// loopback address, `127.0.0.1/32` and `::1/128` are trusted: only local
    /// processes (a reverse proxy on the same host) can reach a loopback
    /// listener, so believing their `X-Forwarded-For` adds no remote attack
    /// surface, and without it every client would share one rate-limit key.
    /// Set the variable to override (any value replaces the default).
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let var = |k: &str| lookup(k).filter(|v| !v.is_empty());
        let mb = |k: &str, default: u64| {
            var(k)
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(default)
                * MIB
        };
        let secs = |k: &str, default: Duration| {
            var(k)
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|v| *v > 0)
                .map_or(default, Duration::from_secs)
        };
        let defaults = Self::default();
        let bind = var("STARSTAR_BIND").unwrap_or(defaults.bind);
        let (trusted_proxies, invalid_trusted_proxies) = match var("STARSTAR_TRUSTED_PROXIES") {
            Some(v) => parse_trusted_proxies(&v),
            None if bind_is_loopback(&bind) => (loopback_networks(), Vec::new()),
            None => (Vec::new(), Vec::new()),
        };
        let db_max_connections = var("STARSTAR_DB_MAX_CONNECTIONS")
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|v| (1..=100).contains(v))
            .unwrap_or(defaults.db_max_connections);
        Self {
            database_url: var("STARSTAR_DATABASE_URL"),
            db_max_connections,
            db_connect_timeout: secs(
                "STARSTAR_DB_CONNECT_TIMEOUT_SECS",
                defaults.db_connect_timeout,
            ),
            bind,
            public_url: var("STARSTAR_PUBLIC_URL")
                .map(|u| normalize_public_url(&u))
                .unwrap_or(defaults.public_url),
            allow_insecure_http: var("STARSTAR_ALLOW_INSECURE_HTTP").is_some_and(|v| v == "1"),
            setup_token: var("STARSTAR_SETUP_TOKEN"),
            max_upload_bytes: mb("STARSTAR_MAX_UPLOAD_MB", 512),
            mcp_max_upload_bytes: mb("STARSTAR_MCP_MAX_UPLOAD_MB", 25),
            web_dir: var("STARSTAR_WEB_DIR").unwrap_or(defaults.web_dir),
            printer_timeout: secs("STARSTAR_PRINTER_TIMEOUT_SECS", defaults.printer_timeout),
            upload_timeout: secs("STARSTAR_UPLOAD_TIMEOUT_SECS", defaults.upload_timeout),
            trusted_proxies,
            invalid_trusted_proxies,
        }
    }
}

fn bind_is_loopback(bind: &str) -> bool {
    bind.parse::<std::net::SocketAddr>()
        .is_ok_and(|a| a.ip().is_loopback())
}

fn loopback_networks() -> Vec<IpNet> {
    ["127.0.0.1/32", "::1/128"]
        .iter()
        .filter_map(|n| n.parse().ok())
        .collect()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database_url: None,
            db_max_connections: 10,
            db_connect_timeout: Duration::from_secs(30),
            bind: "127.0.0.1:8080".to_owned(),
            public_url: "http://127.0.0.1:8080".to_owned(),
            allow_insecure_http: false,
            setup_token: None,
            max_upload_bytes: 512 * MIB,
            mcp_max_upload_bytes: 25 * MIB,
            web_dir: "web/.output/public".to_owned(),
            printer_timeout: Duration::from_secs(10),
            upload_timeout: Duration::from_secs(3600),
            trusted_proxies: Vec::new(),
            invalid_trusted_proxies: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printer_timeouts_default() {
        let config = Config::default();
        assert_eq!(config.printer_timeout, Duration::from_secs(10));
        assert_eq!(config.upload_timeout, Duration::from_secs(3600));
    }

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

    fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    fn shown(c: &Config) -> Vec<String> {
        c.trusted_proxies.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn from_lookup_normalizes_the_public_url() {
        let c = Config::from_lookup(lookup(&[("STARSTAR_PUBLIC_URL", "HTTPS://X.Example/")]));
        assert_eq!(c.public_url, "https://x.example");
        assert_eq!(
            Config::from_lookup(|_| None).public_url,
            Config::default().public_url
        );
    }

    #[test]
    fn loopback_bind_trusts_loopback_proxies_by_default() {
        assert_eq!(
            shown(&Config::from_lookup(|_| None)),
            ["127.0.0.1/32", "::1/128"]
        );
        let c = Config::from_lookup(lookup(&[("STARSTAR_BIND", "[::1]:9000")]));
        assert_eq!(shown(&c), ["127.0.0.1/32", "::1/128"]);
    }

    #[test]
    fn non_loopback_bind_trusts_no_proxies_by_default() {
        for bind in ["0.0.0.0:8080", "192.168.1.5:8080", "[::]:8080"] {
            let c = Config::from_lookup(lookup(&[("STARSTAR_BIND", bind)]));
            assert!(c.trusted_proxies.is_empty(), "{bind}");
        }
    }

    #[test]
    fn an_explicit_value_replaces_the_loopback_default() {
        let c = Config::from_lookup(lookup(&[("STARSTAR_TRUSTED_PROXIES", "10.0.0.0/8")]));
        assert_eq!(shown(&c), ["10.0.0.0/8"]);
        // Even an all-invalid value does not fall back to the default.
        let c = Config::from_lookup(lookup(&[("STARSTAR_TRUSTED_PROXIES", "nope")]));
        assert!(c.trusted_proxies.is_empty());
        assert_eq!(c.invalid_trusted_proxies, ["nope"]);
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

    #[test]
    fn database_settings_and_redaction() {
        let c = Config::from_lookup(|_| None);
        assert!(c.database_url.is_none());
        assert_eq!(c.db_max_connections, 10);
        assert_eq!(c.db_connect_timeout, Duration::from_secs(30));

        let pw = ["s3", "cr3t"].concat();
        let sample_url = format!("postgres://starstar:{pw}@db:5432/starstar?sslmode=require");
        let c = Config::from_lookup(lookup(&[
            ("STARSTAR_DATABASE_URL", sample_url.as_str()),
            ("STARSTAR_DB_MAX_CONNECTIONS", "25"),
            ("STARSTAR_DB_CONNECT_TIMEOUT_SECS", "45"),
        ]));
        assert_eq!(c.database_url.as_deref(), Some(sample_url.as_str()));
        assert_eq!(c.db_max_connections, 25);
        assert_eq!(c.db_connect_timeout, Duration::from_secs(45));

        // Out-of-bounds pool sizes fall back to default.
        for bad in ["0", "101", "nope"] {
            let c = Config::from_lookup(lookup(&[("STARSTAR_DB_MAX_CONNECTIONS", bad)]));
            assert_eq!(c.db_max_connections, 10, "{bad}");
        }

        let redacted = redact_database_url(&sample_url);
        assert_eq!(redacted, "postgres://db:5432/starstar");
        assert!(!redacted.contains(&pw));
        assert!(!redacted.contains("starstar:"));
        assert!(!redacted.contains("sslmode"));

        assert_eq!(
            redact_database_url("postgresql://user:pw@[::1]:54329/testdb"),
            "postgresql://[::1]:54329/testdb"
        );
        assert_eq!(
            redact_database_url("http://user:pw@host/db"),
            "<invalid database URL>"
        );
        assert_eq!(redact_database_url("starstar.db"), "<invalid database URL>");
    }
}
