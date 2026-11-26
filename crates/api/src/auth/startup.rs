//! Startup checks on the configuration.

use std::net::SocketAddr;

use url::Url;

use crate::config::Config;

#[derive(Debug, thiserror::Error)]
pub enum StartupError {
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

fn bad_url(why: &str) -> StartupError {
    StartupError::InvalidPublicUrl(format!("STARSTAR_PUBLIC_URL is invalid: {why}"))
}

/// Validates the config. On success returns an optional warning for stderr.
pub fn check_config(config: &Config) -> Result<Option<String>, StartupError> {
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
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(bind: &str, url: &str, insecure: bool) -> Config {
        Config {
            bind: bind.into(),
            public_url: url.into(),
            allow_insecure_http: insecure,
            ..Config::default()
        }
    }

    #[test]
    fn defaults_pass_without_warning() {
        assert!(check_config(&Config::default()).unwrap().is_none());
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
            check_config(&cfg("0.0.0.0:8080", "https://print.example.com", false))
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
    fn an_invalid_trusted_proxy_entry_fails_the_check() {
        let c = Config {
            invalid_trusted_proxies: vec!["10.0.0.0/33".into()],
            ..Config::default()
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
            ..Config::default()
        };
        assert!(matches!(
            check_config(&c),
            Err(StartupError::WeakSetupToken)
        ));
        c.setup_token = Some("0123456789abcdef".into());
        assert!(check_config(&c).is_ok());
    }
}
