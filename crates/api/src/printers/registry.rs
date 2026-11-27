//! Registering, listing and removing printers.

use std::net::Ipv6Addr;

use starstar_shared::{PrinterSummary, RegisterPrinter, Role, is_valid_printer_name};

use super::{client, find};
use crate::AppState;
use crate::auth::Caller;
use crate::error::ApiError;
use crate::store::StoredPrinter;

pub const LIST_PRINTERS_ROLE: Role = Role::Viewer;
pub const SHOW_PRINTER_ROLE: Role = Role::Viewer;
pub const REGISTER_PRINTER_ROLE: Role = Role::Admin;
pub const REMOVE_PRINTER_ROLE: Role = Role::Admin;

pub async fn list_printers(
    state: &AppState,
    caller: &Caller,
) -> Result<Vec<PrinterSummary>, ApiError> {
    caller.require(LIST_PRINTERS_ROLE)?;
    Ok(state
        .store
        .list()
        .await?
        .iter()
        .map(StoredPrinter::summary)
        .collect())
}

pub async fn show_printer(
    state: &AppState,
    caller: &Caller,
    name: &str,
) -> Result<PrinterSummary, ApiError> {
    caller.require(SHOW_PRINTER_ROLE)?;
    Ok(find(state, name).await?.summary())
}

pub async fn register_printer(
    state: &AppState,
    caller: &Caller,
    req: RegisterPrinter,
) -> Result<PrinterSummary, ApiError> {
    caller.require(REGISTER_PRINTER_ROLE)?;
    validate_registration(&req)?;
    if state.store.get(&req.name).await?.is_some() {
        return Err(ApiError::Conflict(format!(
            "printer `{}` already exists",
            req.name
        )));
    }

    let printer = StoredPrinter {
        name: req.name,
        kind: req.kind,
        host: req.host,
        username: req.username,
        password: req.password,
    };

    // Confirm the printer is reachable and the credentials work before saving.
    client(state, &printer)
        .version()
        .await
        .map_err(|err| with_prefix(ApiError::from(err), "could not verify printer: "))?;

    state.store.insert(printer.clone()).await?;
    Ok(printer.summary())
}

pub async fn remove_printer(state: &AppState, caller: &Caller, name: &str) -> Result<(), ApiError> {
    caller.require(REMOVE_PRINTER_ROLE)?;
    if state.store.remove(name).await? {
        Ok(())
    } else {
        Err(ApiError::NotFound(format!("no printer named `{name}`")))
    }
}

/// Prepends `prefix` to the message of an upstream-derived error, keeping its
/// status mapping (so a timeout stays a 504).
fn with_prefix(err: ApiError, prefix: &str) -> ApiError {
    match err {
        ApiError::Upstream(m) => ApiError::Upstream(format!("{prefix}{m}")),
        ApiError::GatewayTimeout(m) => ApiError::GatewayTimeout(format!("{prefix}{m}")),
        ApiError::Printer(status, m) => ApiError::Printer(status, format!("{prefix}{m}")),
        other => other,
    }
}

fn valid_port(port: &str) -> bool {
    !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) && port.parse::<u16>().is_ok()
}

/// `host[:port]` only: no scheme, credentials, path, query or fragment. IPv6
/// literals must be bracketed.
fn valid_host(host: &str) -> bool {
    if host.is_empty()
        || host.chars().any(|c| {
            c.is_whitespace() || c.is_control() || matches!(c, '/' | '@' | '?' | '#' | '\\')
        })
    {
        return false;
    }
    if let Some(rest) = host.strip_prefix('[') {
        let Some((addr, after)) = rest.split_once(']') else {
            return false;
        };
        return addr.parse::<Ipv6Addr>().is_ok()
            && (after.is_empty() || after.strip_prefix(':').is_some_and(valid_port));
    }
    let mut parts = host.split(':');
    let name = parts.next().unwrap_or_default();
    match (parts.next(), parts.next()) {
        (None, _) => !name.is_empty(),
        (Some(port), None) => !name.is_empty() && valid_port(port),
        _ => false,
    }
}

fn validate_registration(req: &RegisterPrinter) -> Result<(), ApiError> {
    if !is_valid_printer_name(&req.name) {
        return Err(ApiError::BadRequest(
            "printer names must be 1-63 lowercase letters, digits or dashes, and cannot start with a dash"
                .to_owned(),
        ));
    }
    if !valid_host(&req.host) {
        return Err(ApiError::BadRequest(
            "host must be a hostname or IP address, optionally with a port".to_owned(),
        ));
    }
    if req.username.is_empty() || req.password.is_empty() {
        return Err(ApiError::BadRequest(
            "username and password are required".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::valid_host;

    #[test]
    fn valid_host_accepts_bracketed_ipv6_with_or_without_port() {
        for host in ["[::1]", "[::1]:1", "[fe80::1]", "[fe80::1]:8080"] {
            assert!(valid_host(host), "{host}");
        }
    }

    #[test]
    fn valid_host_rejects_bare_or_malformed_ipv6() {
        for host in ["fe80::1", "::1", "[::1", "[::1]:", "[::1]:99999", "[nope]"] {
            assert!(!valid_host(host), "{host}");
        }
    }
}
