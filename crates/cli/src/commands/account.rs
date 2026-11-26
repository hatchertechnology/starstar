//! Account commands: status, login, logout, whoami, setup.

use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use serde_json::json;
use starstar_shared::auth::{SessionInfo, SetupRequest, WhoAmI};
use starstar_shared::{Health, Role};
use url::Url;

use super::{Ctx, read_pasted_code, read_secret, stdin_line};
use crate::client::{ApiClient, build_http};
use crate::credentials::{CredentialStore, default_path, server_key};
use crate::endpoints;
use crate::error::{CliError, CliResult};
use crate::login::{login_copy_code, login_loopback};
use crate::oauth;
use crate::output::Output;
use crate::session::{PlainHttpWarner, TokenOrigin, TokenSource};

pub fn status(ctx: &Ctx) -> CliResult<()> {
    let health: Health = ctx.public_client()?.get_json(&endpoints::health())?;
    ctx.out.emit_value(&health, || {
        format!(
            "{} API {} is {} at {}",
            health.app, health.version, health.status, ctx.api
        )
    });
    Ok(())
}

/// How `login` gets the authorization code.
pub enum LoginMethod<'a> {
    Loopback {
        open: &'a dyn Fn(&str) -> io::Result<()>,
        timeout: Duration,
    },
    CopyCode {
        prompt: &'a mut dyn FnMut(&str) -> io::Result<String>,
    },
}

fn env_token_set() -> bool {
    std::env::var("STARSTAR_TOKEN").is_ok_and(|t| !t.is_empty())
}

/// Signs in, checks the new token with `GET /api/me`, stores it and revokes
/// the previous sign-in for this server.
pub fn login_with(
    api: &Url,
    store: CredentialStore,
    role: Role,
    method: LoginMethod<'_>,
    out: &Output,
) -> CliResult<()> {
    let http = build_http()?;
    let tok = match method {
        LoginMethod::Loopback { open, timeout } => {
            login_loopback(&http, api, role, timeout, open, &mut io::stderr())?
        }
        LoginMethod::CopyCode { prompt } => login_copy_code(&http, api, role, prompt)?,
    };
    let probe = ApiClient::new(
        api.clone(),
        Some(TokenSource::env(api.clone(), tok.access_token.clone())),
        http.clone(),
        PlainHttpWarner::stderr(),
    );
    let me: WhoAmI = match probe.get_json(&endpoints::me()) {
        Ok(me) => me,
        Err(e) => {
            // Do not leave a live grant behind that nothing will ever use.
            if let Some(ep) = &tok.revocation_endpoint {
                let _ = oauth::revoke(&http, ep, &tok.refresh_token);
            }
            return Err(e);
        }
    };
    let key = server_key(api);
    let previous = store.update(&key, |cur| Ok((Some(tok.clone()), cur)))?;
    if let Some(prev) = previous
        && let Some(ep) = &prev.revocation_endpoint
    {
        let _ = oauth::revoke(&http, ep, &prev.refresh_token);
    }
    out.emit(
        &json!({"api_url": key, "username": me.username, "role": me.role}),
        || format!("Signed in to {key} as {} ({}).", me.username, me.role),
    );
    if env_token_set() {
        eprintln!("note: STARSTAR_TOKEN is set and overrides this sign-in");
    }
    Ok(())
}

pub fn login(ctx: &Ctx, no_browser: bool, role: Role) -> CliResult<()> {
    let store = CredentialStore::at(default_path()?);
    let method = if no_browser {
        let mut prompt = |url: &str| -> io::Result<String> {
            let mut err = io::stderr();
            write!(
                err,
                "Open this URL on any device, sign in and approve:\n  {url}\nThen paste the code it shows here: "
            )?;
            err.flush()?;
            let tty = io::stdin().is_terminal();
            let code = read_pasted_code(tty, rpassword::read_password, || {
                let mut line = String::new();
                io::stdin().read_line(&mut line)?;
                Ok(line)
            })?;
            if tty {
                // The silent read leaves the cursor on the prompt line.
                writeln!(err)?;
            }
            Ok(code)
        };
        return login_with(
            &ctx.api,
            store,
            role,
            LoginMethod::CopyCode {
                prompt: &mut prompt,
            },
            &ctx.out,
        );
    } else {
        LoginMethod::Loopback {
            open: &|url| webbrowser::open(url),
            timeout: Duration::from_secs(300),
        }
    };
    login_with(&ctx.api, store, role, method, &ctx.out)
}

pub fn logout(ctx: &Ctx) -> CliResult<()> {
    let store = CredentialStore::at(default_path()?);
    let key = server_key(&ctx.api);
    let entry = store.remove(&key)?;
    let mut revoked = false;
    if let Some(e) = &entry
        && let Some(ep) = &e.revocation_endpoint
    {
        match oauth::revoke(&build_http()?, ep, &e.refresh_token) {
            Ok(()) => revoked = true,
            Err(err) => eprintln!("warning: could not revoke the token on the server: {err}"),
        }
    }
    let signed_out = entry.is_some();
    ctx.out.emit(
        &json!({"api_url": key, "signed_out": signed_out, "revoked": revoked}),
        || {
            if signed_out {
                format!("Signed out of {key}.")
            } else {
                format!("Not signed in to {key}.")
            }
        },
    );
    if env_token_set() {
        eprintln!("note: STARSTAR_TOKEN is still set; unset it to stop using that token");
    }
    Ok(())
}

pub fn whoami(ctx: &Ctx) -> CliResult<()> {
    let client = ctx.client()?;
    let me: WhoAmI = client.get_json(&endpoints::me())?;
    let source = match client.tokens().map(TokenSource::origin) {
        Some(TokenOrigin::Env) => "env",
        _ => "credentials",
    };
    let key = server_key(&ctx.api);
    ctx.out.emit(
        &json!({"api_url": key, "username": me.username, "role": me.role, "token_source": source}),
        || {
            let from = if source == "env" {
                "STARSTAR_TOKEN"
            } else {
                "credentials file"
            };
            format!("{} ({}) at {key}, token from {from}", me.username, me.role)
        },
    );
    Ok(())
}

/// Creates the first admin with the setup code the API printed at startup.
pub fn setup(
    ctx: &Ctx,
    username: String,
    setup_code_stdin: bool,
    password_stdin: bool,
) -> CliResult<()> {
    if !starstar_shared::auth::is_valid_username(&username) {
        return Err(CliError::Usage(format!(
            "invalid username `{username}`: use 1-32 lowercase letters, digits, '.', '_' or '-', starting with a letter or digit"
        )));
    }
    let setup_code = if setup_code_stdin {
        stdin_line()?
    } else {
        read_secret("Setup code: ", false)?
    };
    let password = if password_stdin {
        stdin_line()?
    } else {
        let p = read_secret(&format!("Password for {username}: "), false)?;
        if read_secret("Repeat: ", false)? != p {
            return Err(CliError::Usage("passwords did not match".into()));
        }
        p
    };
    super::check_password(&password)?;
    let session: SessionInfo = ctx.public_client()?.send_json(
        &endpoints::setup(),
        &SetupRequest {
            setup_code,
            username,
            password,
        },
    )?;
    let key = server_key(&ctx.api);
    ctx.out.emit(
        &json!({"api_url": key, "username": session.username, "role": session.role}),
        || {
            format!(
                "Created admin `{}` on {key}. Run `starstar login` to sign in.",
                session.username
            )
        },
    );
    Ok(())
}
