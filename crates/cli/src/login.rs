//! The browser sign-in: a loopback callback server, or a pasted code.

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use starstar_shared::Role;
use starstar_shared::auth::{CLI_REDIRECT_PATH, COPY_CODE_PATH};
use url::Url;

use crate::credentials::StoredToken;
use crate::error::{CliError, CliResult};
use crate::oauth::{self, Pkce};
use crate::session::PlainHttpWarner;

/// The loopback listener that receives the browser's redirect.
pub struct Callback {
    listener: TcpListener,
    pub redirect_uri: String,
}

impl Callback {
    pub fn bind() -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        Ok(Self {
            listener,
            redirect_uri: format!("http://127.0.0.1:{port}{CLI_REDIRECT_PATH}"),
        })
    }

    /// Serves connections until one valid callback arrives or `timeout` passes.
    /// Stray requests (favicon, stale tabs) are answered and ignored.
    pub fn wait(&self, state: &str, timeout: Duration) -> CliResult<String> {
        let deadline = Instant::now() + timeout;
        self.listener.set_nonblocking(true)?;
        loop {
            if Instant::now() >= deadline {
                return Err(CliError::Auth(
                    "timed out waiting for the browser sign-in; on a machine without a browser, use `starstar login --no-browser`"
                        .into(),
                ));
            }
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if let Some(outcome) = serve(stream, state, deadline) {
                        return outcome;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}

/// Handles one connection. `Some` ends the wait.
fn serve(mut stream: TcpStream, state: &str, deadline: Instant) -> Option<CliResult<String>> {
    stream.set_nonblocking(false).ok()?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    let read_for = remaining
        .min(Duration::from_secs(5))
        .max(Duration::from_millis(1));
    stream.set_read_timeout(Some(read_for)).ok()?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    while buf.len() < 8192 && !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let mut line = text.lines().next().unwrap_or("").split_whitespace();
    let (method, target) = (line.next().unwrap_or(""), line.next().unwrap_or(""));
    let url = Url::parse(&format!("http://127.0.0.1{target}")).ok();
    let Some(url) = url.filter(|u| method == "GET" && u.path() == CLI_REDIRECT_PATH) else {
        respond(&mut stream, 404, "Not found.");
        return None;
    };
    let q = |k: &str| {
        url.query_pairs()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.into_owned())
    };
    if q("state").as_deref() != Some(state) {
        respond(
            &mut stream,
            400,
            "This sign-in link is stale. Start again with starstar login.",
        );
        return None;
    }
    if let Some(error) = q("error") {
        respond(
            &mut stream,
            200,
            "Sign-in was cancelled. You can close this tab.",
        );
        return Some(Err(CliError::Auth(if error == "access_denied" {
            "sign-in was denied in the browser".to_owned()
        } else {
            format!("sign-in failed: {error}")
        })));
    }
    match q("code") {
        Some(code) => {
            respond(
                &mut stream,
                200,
                "You are signed in to starstar. You can close this tab.",
            );
            Some(Ok(code))
        }
        None => {
            respond(&mut stream, 400, "This sign-in link has no code.");
            None
        }
    }
}

fn respond(stream: &mut TcpStream, status: u16, message: &str) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Not Found",
    };
    let body =
        format!("<!doctype html><meta charset=utf-8><title>starstar</title><p>{message}</p>");
    let _ = write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.flush();
}

/// Sign in through the browser, receiving the code on a loopback port.
pub fn login_loopback(
    http: &Client,
    api: &Url,
    role: Role,
    timeout: Duration,
    open: &dyn Fn(&str) -> io::Result<()>,
    notice: &mut dyn Write,
) -> CliResult<StoredToken> {
    let meta = oauth::discover(http, api)?;
    let pkce = Pkce::new();
    let state = oauth::random_state();
    let callback = Callback::bind()?;
    let url = oauth::authorize_url(&meta, &callback.redirect_uri, &pkce, &state, role)?;
    if let Ok(endpoint) = Url::parse(&meta.token_endpoint) {
        PlainHttpWarner::stderr().check(&endpoint);
    }
    writeln!(
        notice,
        "Opening your browser to sign in. If it does not open, visit:\n  {url}"
    )?;
    let _ = open(url.as_str());
    let code = callback.wait(&state, timeout)?;
    let tokens = oauth::exchange_code(http, &meta, &code, &callback.redirect_uri, &pkce)?;
    Ok(StoredToken::from_response(&tokens, &meta))
}

/// Sign in on a machine without a browser: the user opens the URL anywhere
/// and pastes the code the page shows.
pub fn login_copy_code(
    http: &Client,
    api: &Url,
    role: Role,
    prompt: &mut dyn FnMut(&str) -> io::Result<String>,
) -> CliResult<StoredToken> {
    let meta = oauth::discover(http, api)?;
    let pkce = Pkce::new();
    let state = oauth::random_state();
    let redirect_uri = format!("{}{COPY_CODE_PATH}", meta.issuer.trim_end_matches('/'));
    let url = oauth::authorize_url(&meta, &redirect_uri, &pkce, &state, role)?;
    if let Ok(endpoint) = Url::parse(&meta.token_endpoint) {
        PlainHttpWarner::stderr().check(&endpoint);
    }
    let code = prompt(url.as_str())?;
    let code = code.trim();
    if code.is_empty() {
        return Err(CliError::Auth("no code entered".into()));
    }
    let tokens = oauth::exchange_code(http, &meta, code, &redirect_uri, &pkce)?;
    Ok(StoredToken::from_response(&tokens, &meta))
}
