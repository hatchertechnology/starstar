//! Where the bearer token comes from: `STARSTAR_TOKEN`, or the stored sign-in
//! (refreshed under the credentials lock).

use std::io::Write;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::blocking::Client;
use url::{Host, Url};

use crate::credentials::{CredentialStore, StoredToken, server_key};
use crate::error::{CliError, CliResult};
use crate::oauth::{self, OAuthFailure};

/// Seconds since the Unix epoch.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// The warning for sending a token over plain HTTP to a non-loopback host.
pub fn plain_http_warning(url: &Url) -> Option<String> {
    if url.scheme() != "http" {
        return None;
    }
    let loopback = match url.host()? {
        Host::Domain(d) => d.eq_ignore_ascii_case("localhost"),
        Host::Ipv4(ip) => IpAddr::V4(ip).is_loopback(),
        Host::Ipv6(ip) => IpAddr::V6(ip).is_loopback(),
    };
    if loopback {
        return None;
    }
    let host = url.host_str()?;
    Some(format!(
        "warning: sending your starstar token over plain HTTP to {host}; anyone on the network can read it. Use an https:// URL."
    ))
}

/// Prints the plain-HTTP warning at most once.
pub struct PlainHttpWarner {
    warned: AtomicBool,
    sink: Mutex<Box<dyn Write + Send>>,
}

impl PlainHttpWarner {
    pub fn new(sink: Box<dyn Write + Send>) -> Self {
        Self {
            warned: AtomicBool::new(false),
            sink: Mutex::new(sink),
        }
    }

    /// The process-wide warner that writes to stderr.
    pub fn stderr() -> &'static Self {
        static W: OnceLock<PlainHttpWarner> = OnceLock::new();
        W.get_or_init(|| Self::new(Box::new(std::io::stderr())))
    }

    /// Call right before sending a credential to `url`.
    pub fn check(&self, url: &Url) {
        if let Some(msg) = plain_http_warning(url)
            && !self.warned.swap(true, Ordering::SeqCst)
            && let Ok(mut sink) = self.sink.lock()
        {
            let _ = writeln!(sink, "{msg}");
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenOrigin {
    Env,
    Stored,
}

enum Kind {
    Env(String),
    Stored {
        key: String,
        store: CredentialStore,
        http: Client,
    },
}

/// Hands out bearer tokens. Not `Debug`.
pub struct TokenSource {
    api: Url,
    kind: Kind,
    warner: &'static PlainHttpWarner,
}

enum Reason<'a> {
    /// The stored token expires within this many seconds.
    Expiring(i64),
    /// The server rejected this access token.
    Rejected(&'a str),
}

impl TokenSource {
    /// A token taken from `STARSTAR_TOKEN`. It is never refreshed.
    pub fn env(api: Url, token: String) -> Self {
        Self {
            api,
            kind: Kind::Env(token),
            warner: PlainHttpWarner::stderr(),
        }
    }

    /// The stored sign-in for `api`. No entry means not signed in.
    pub fn stored(
        api: Url,
        store: CredentialStore,
        http: Client,
        warner: &'static PlainHttpWarner,
    ) -> CliResult<Self> {
        let key = server_key(&api);
        if store.get(&key)?.is_none() {
            return Err(not_signed_in(&key));
        }
        Ok(Self {
            api,
            kind: Kind::Stored { key, store, http },
            warner,
        })
    }

    pub fn api(&self) -> &Url {
        &self.api
    }

    pub fn origin(&self) -> TokenOrigin {
        match self.kind {
            Kind::Env(_) => TokenOrigin::Env,
            Kind::Stored { .. } => TokenOrigin::Stored,
        }
    }

    /// The token to send now. A stored token that expires within 60 s is refreshed first.
    pub fn bearer(&self) -> CliResult<String> {
        self.bearer_with_margin(60)
    }

    /// Like [`TokenSource::bearer`] with a custom margin in seconds.
    pub fn bearer_with_margin(&self, margin: i64) -> CliResult<String> {
        match &self.kind {
            Kind::Env(t) => Ok(t.clone()),
            Kind::Stored { key, store, .. } => {
                let entry = store.get(key)?.ok_or_else(|| not_signed_in(key))?;
                if entry.expires_at - margin > now() {
                    Ok(entry.access_token)
                } else {
                    self.refresh(Reason::Expiring(margin))
                }
            }
        }
    }

    /// After a 401 for `rejected`: `None` for an env token, else a fresh token.
    pub fn after_rejection(&self, rejected: &str) -> CliResult<Option<String>> {
        match &self.kind {
            Kind::Env(_) => Ok(None),
            Kind::Stored { .. } => self.refresh(Reason::Rejected(rejected)).map(Some),
        }
    }

    fn refresh(&self, reason: Reason<'_>) -> CliResult<String> {
        let Kind::Stored { key, store, http } = &self.kind else {
            return Err(CliError::Other("an env token cannot be refreshed".into()));
        };
        store.update(key, |cur| {
            let Some(entry) = cur else {
                return Ok((None, Err(not_signed_in(key))));
            };
            // Someone else may have refreshed while we waited for the lock.
            let fresh = match reason {
                Reason::Expiring(margin) => entry.expires_at - margin > now(),
                Reason::Rejected(r) => entry.access_token != r,
            };
            if fresh {
                let token = entry.access_token.clone();
                return Ok((Some(entry), Ok(token)));
            }
            if let Ok(endpoint) = Url::parse(&entry.token_endpoint) {
                self.warner.check(&endpoint);
            }
            match oauth::refresh(http, &entry.token_endpoint, &entry.refresh_token) {
                Ok(resp) => {
                    let new = StoredToken {
                        access_token: resp.access_token.clone(),
                        refresh_token: resp.refresh_token.clone(),
                        expires_at: now() + resp.expires_in as i64,
                        scope: resp.scope.clone(),
                        ..entry
                    };
                    Ok((Some(new), Ok(resp.access_token)))
                }
                Err(f) if f.is_grant_gone() => Ok((
                    None,
                    Err(CliError::Auth(format!(
                        "your sign-in to {key} has expired or was revoked; run `starstar login`"
                    ))),
                )),
                Err(OAuthFailure::Rejected { error, .. }) => Ok((
                    Some(entry),
                    Err(CliError::Other(format!("token refresh failed: {error}"))),
                )),
                Err(OAuthFailure::Other(e)) => Ok((Some(entry), Err(e))),
            }
        })?
    }
}

fn not_signed_in(key: &str) -> CliError {
    CliError::Auth(format!("not signed in to {key}; run `starstar login`"))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    struct Buf(Arc<Mutex<Vec<u8>>>);
    impl Write for Buf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn plain_http_warning_rules() {
        let w = |s: &str| plain_http_warning(&Url::parse(s).unwrap());
        let lan = w("http://192.168.1.5:8080").unwrap();
        assert!(lan.contains("plain HTTP") && lan.contains("192.168.1.5"));
        assert!(w("http://printer.lan").unwrap().contains("printer.lan"));
        for ok in [
            "https://192.168.1.5",
            "http://127.0.0.1:8080",
            "http://127.9.9.9",
            "http://localhost:1",
            "http://[::1]:8080",
        ] {
            assert!(w(ok).is_none(), "{ok}");
        }
    }

    #[test]
    fn warner_warns_once() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let warner = PlainHttpWarner::new(Box::new(Buf(buf.clone())));
        let url = Url::parse("http://printer.lan").unwrap();
        warner.check(&url);
        warner.check(&url);
        let text = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        assert_eq!(text.lines().count(), 1, "{text}");
    }

    /// A stored sign-in whose token endpoint refuses connections, so any
    /// network refresh fails loudly.
    fn source_with(access: &str, expires_at: i64) -> (tempfile::TempDir, TokenSource) {
        let dir = tempfile::tempdir().unwrap();
        let store = CredentialStore::at(dir.path().join("cfg/starstar/credentials.json"));
        let api = Url::parse("http://127.0.0.1:1").unwrap();
        store
            .put(
                &server_key(&api),
                StoredToken {
                    access_token: access.into(),
                    refresh_token: "rt".into(),
                    expires_at,
                    scope: "starstar:operator".into(),
                    issuer: "http://127.0.0.1:1".into(),
                    token_endpoint: "http://127.0.0.1:1/oauth/token".into(),
                    revocation_endpoint: None,
                },
            )
            .unwrap();
        let http = crate::client::build_http().unwrap();
        let src = TokenSource::stored(api, store, http, PlainHttpWarner::stderr()).unwrap();
        (dir, src)
    }

    #[test]
    fn refresh_rechecks_after_taking_the_lock() {
        // Another process refreshed while we waited: the entry is fresh now,
        // so no second rotation (the endpoint is dead and would fail).
        let (_dir, src) = source_with("at_new", now() + 3600);
        assert_eq!(src.refresh(Reason::Expiring(60)).unwrap(), "at_new");
        assert_eq!(src.refresh(Reason::Rejected("at_old")).unwrap(), "at_new");
        // Still the same token we were told was rejected: that one must be refreshed.
        assert!(src.refresh(Reason::Rejected("at_new")).is_err());
    }

    #[test]
    fn refresh_of_an_expiring_entry_goes_to_the_network() {
        let (_dir, src) = source_with("at_soon", now() + 10);
        assert!(src.refresh(Reason::Expiring(60)).is_err());
    }
}
