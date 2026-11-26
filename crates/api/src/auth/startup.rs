//! Startup checks on the configuration.

use std::net::SocketAddr;
use std::str::FromStr;

use sqlx::postgres::PgConnectOptions;
use url::Url;

use crate::config::Config;

#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    #[error(
        "STARSTAR_DATABASE_URL is required (set a postgres:// or postgresql:// connection URL)"
    )]
    MissingDatabaseUrl,
    /// Deliberately carries no URL text: it may contain credentials.
    #[error("{0}")]
    InvalidDatabaseUrl(String),
    #[error("STARSTAR_BIND `{0}` is not a socket address like 127.0.0.1:8080")]
    InvalidBind(String),
    /// Deliberately carries no URL text: it may contain credentials.
    #[error("{0}")]
    InvalidPublicUrl(String),
    #[error(
        "refusing to listen on {0} over plain HTTP: set STARSTAR_PUBLIC_URL to an https:// URL served by a TLS proxy, or set STARSTAR_ALLOW_INSECURE_HTTP=1"
    )]
    InsecureHttp(String),
    #[error(
        "STARSTAR_TRUSTED_PROXIES entry `{0}` is neither an IP address nor a CIDR like 10.0.0.0/8"
    )]
    InvalidTrustedProxy(String),
    #[error("STARSTAR_SETUP_TOKEN must be at least 16 characters")]
    WeakSetupToken,
}

fn bad_db_url(why: &str) -> StartupError {
    StartupError::InvalidDatabaseUrl(format!("STARSTAR_DATABASE_URL is invalid: {why}"))
}

fn bad_url(why: &str) -> StartupError {
    StartupError::InvalidPublicUrl(format!("STARSTAR_PUBLIC_URL is invalid: {why}"))
}

/// The effective trusted proxy list, for the startup log.
pub fn trusted_proxies_line(config: &Config) -> String {
    if config.trusted_proxies.is_empty() {
        return "trusted proxies: none (X-Forwarded-For is ignored)".to_owned();
    }
    let nets: Vec<String> = config
        .trusted_proxies
        .iter()
        .map(|n| n.to_string())
        .collect();
    format!("trusted proxies: {}", nets.join(", "))
}

/// Validates the config. On success returns an optional warning, which the
/// caller prints to stderr. Only the first applicable warning is returned
/// (plain HTTP, then the proxy list); callers that add more must return a list.
pub fn check_config(config: &Config) -> Result<Option<String>, StartupError> {
    let raw_db = config
        .database_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or(StartupError::MissingDatabaseUrl)?;
    let db_url =
        Url::parse(raw_db).map_err(|_| bad_db_url("must be a postgres:// or postgresql:// URL"))?;
    if !matches!(db_url.scheme(), "postgres" | "postgresql") {
        return Err(bad_db_url("the scheme must be postgres or postgresql"));
    }
    if db_url.host_str().is_none() {
        return Err(bad_db_url("it needs a host"));
    }
    if PgConnectOptions::from_str(raw_db).is_err() {
        return Err(bad_db_url("could not parse PostgreSQL connection options"));
    }

    let bind: SocketAddr = config
        .bind
        .parse()
        .map_err(|_| StartupError::InvalidBind(config.bind.clone()))?;

    if let Some(entry) = config.invalid_trusted_proxies.first() {
        return Err(StartupError::InvalidTrustedProxy(entry.clone()));
    }

    let url = Url::parse(&config.public_url).map_err(|_| bad_url("not a URL"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(bad_url("the scheme must be http or https"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(bad_url("it must not contain credentials"));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(bad_url("it must not have a query or fragment"));
    }
    if !(url.path().is_empty() || url.path() == "/") {
        return Err(bad_url("it must not have a path"));
    }
    if url.host_str().is_none() {
        return Err(bad_url("it needs a host"));
    }

    if config.setup_token.as_ref().is_some_and(|t| t.len() < 16) {
        return Err(StartupError::WeakSetupToken);
    }

    if url.scheme() == "http" && !bind.ip().is_loopback() {
        if !config.allow_insecure_http {
            return Err(StartupError::InsecureHttp(config.bind.clone()));
        }
        return Ok(Some(format!(
            "warning: serving plain HTTP on {}; tokens and passwords cross the network unencrypted",
            config.bind
        )));
    }
    if url.scheme() == "https" && config.trusted_proxies.is_empty() {
        return Ok(Some(
            "warning: STARSTAR_PUBLIC_URL is https but STARSTAR_TRUSTED_PROXIES is empty, so behind a TLS proxy every request appears to come from the proxy and the per-IP rate limits apply to all clients together; set STARSTAR_TRUSTED_PROXIES to the proxy's address"
                .to_owned(),
        ));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_DB_URL: &str = "postgres://starstar:dev@127.0.0.1:54329/starstar";

    fn valid_config() -> Config {
        Config {
            database_url: Some(TEST_DB_URL.into()),
            ..Config::default()
        }
    }

    fn cfg(bind: &str, url: &str, insecure: bool) -> Config {
        Config {
            database_url: Some(TEST_DB_URL.into()),
            bind: bind.into(),
            public_url: url.into(),
            allow_insecure_http: insecure,
            ..Config::default()
        }
    }

    #[test]
    fn defaults_pass_without_warning() {
        assert!(check_config(&valid_config()).unwrap().is_none());
    }

    #[test]
    fn requires_valid_postgres_database_url_without_leaking_credentials() {
        assert!(matches!(
            check_config(&Config::default()),
            Err(StartupError::MissingDatabaseUrl)
        ));
        for bad in [
            "starstar.db",
            "sqlite:///tmp/starstar.db",
            "http://user:hunter2@127.0.0.1/db",
            "postgres://user:hunter2@/nohost",
            "postgres://user:hunter2@db:notaport/starstar",
        ] {
            let mut c = valid_config();
            c.database_url = Some(bad.into());
            let err = check_config(&c).unwrap_err();
            assert!(
                matches!(err, StartupError::InvalidDatabaseUrl(_)),
                "{bad}: {err}"
            );
            assert!(!err.to_string().contains("hunter2"), "{err}");
        }
    }

    #[test]
    fn rejects_bad_public_urls() {
        for bad in [
            "ftp://x",
            "https://x/sub",
            "https://x/?a=1",
            "https://u:p@x",
            "not a url",
            "https://x#f",
        ] {
            assert!(
                matches!(
                    check_config(&cfg("127.0.0.1:8080", bad, false)),
                    Err(StartupError::InvalidPublicUrl(_))
                ),
                "{bad}"
            );
        }
    }

    #[test]
    fn invalid_url_error_never_echoes_the_url() {
        let err = check_config(&cfg("127.0.0.1:8080", "https://admin:hunter2@x", false))
            .unwrap_err()
            .to_string();
        assert!(!err.contains("hunter2"), "{err}");
    }

    #[test]
    fn refuses_plain_http_on_lan_unless_overridden() {
        assert!(matches!(
            check_config(&cfg("0.0.0.0:8080", "http://192.168.1.5:8080", false)),
            Err(StartupError::InsecureHttp(_))
        ));
        assert!(
            check_config(&cfg("0.0.0.0:8080", "http://192.168.1.5:8080", true))
                .unwrap()
                .is_some()
        );
        assert!(
            check_config(&Config {
                trusted_proxies: vec!["10.0.0.1/32".parse().unwrap()],
                ..cfg("0.0.0.0:8080", "https://print.example.com", false)
            })
            .unwrap()
            .is_none()
        );
        assert!(
            check_config(&cfg("[::1]:8080", "http://[::1]:8080", false))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn the_effective_trusted_proxy_list_is_printable() {
        let c = Config::from_lookup(|_| None);
        assert_eq!(
            trusted_proxies_line(&c),
            "trusted proxies: 127.0.0.1/32, ::1/128"
        );
        let c = Config::from_lookup(|k| (k == "STARSTAR_BIND").then(|| "0.0.0.0:8080".into()));
        assert!(trusted_proxies_line(&c).contains("none"));
    }

    #[test]
    fn https_without_trusted_proxies_warns_that_limits_are_shared() {
        let c = Config {
            setup_token: Some("a-very-secret-setup-token".into()),
            trusted_proxies: Vec::new(),
            ..cfg("0.0.0.0:8080", "https://print.example.com", false)
        };
        let warning = check_config(&c).unwrap().expect("a warning");
        assert!(warning.contains("STARSTAR_TRUSTED_PROXIES"), "{warning}");
        assert!(warning.contains("all clients"), "{warning}");
        assert!(!warning.contains("very-secret"), "{warning}");
        assert!(!warning.contains("print.example.com"), "{warning}");
        // The loopback-bind defaults trust local proxies, so no warning.
        let c = Config::from_lookup(|k| match k {
            "STARSTAR_DATABASE_URL" => Some(TEST_DB_URL.into()),
            "STARSTAR_PUBLIC_URL" => Some("https://print.example.com".into()),
            _ => None,
        });
        assert!(check_config(&c).unwrap().is_none());
    }

    #[test]
    fn an_invalid_trusted_proxy_entry_fails_the_check() {
        let c = Config {
            invalid_trusted_proxies: vec!["10.0.0.0/33".into()],
            ..valid_config()
        };
        let err = check_config(&c).unwrap_err();
        assert!(matches!(err, StartupError::InvalidTrustedProxy(_)));
        assert!(err.to_string().contains("10.0.0.0/33"));
    }

    #[test]
    fn bad_bind_is_an_error() {
        assert!(matches!(
            check_config(&cfg("nope", "http://127.0.0.1:8080", false)),
            Err(StartupError::InvalidBind(_))
        ));
    }

    #[test]
    fn short_setup_token_is_refused() {
        let mut c = Config {
            setup_token: Some("short".into()),
            ..valid_config()
        };
        assert!(matches!(
            check_config(&c),
            Err(StartupError::WeakSetupToken)
        ));
        c.setup_token = Some("0123456789abcdef".into());
        assert!(check_config(&c).is_ok());
    }
}
