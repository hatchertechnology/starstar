//! Errors, exit codes and error reporting.

use std::io::Write;

/// Everything that can go wrong in a command. Messages never contain secrets.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// Bad arguments or client-side validation (exit 2).
    #[error("{0}")]
    Usage(String),
    /// Not signed in, or the sign-in is no longer valid (exit 3).
    #[error("{0}")]
    Auth(String),
    /// A non-success API response: 401 is exit 3, 403 is exit 4, else exit 1.
    #[error("{message}")]
    Api { status: u16, message: String },
    /// A destructive command without `--yes` and without a terminal (exit 5).
    #[error("{0}")]
    NeedsConfirmation(String),
    /// The user answered no at a prompt (exit 1).
    #[error("cancelled")]
    Cancelled,
    /// The API could not be reached (exit 1).
    #[error("could not reach the starstar API at {url}: {detail}")]
    Unreachable { url: String, detail: String },
    /// Any other failure (exit 1).
    #[error("{0}")]
    Other(String),
    /// A local I/O failure (exit 1).
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type CliResult<T> = Result<T, CliError>;

impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Usage(_) => 2,
            Self::Auth(_) => 3,
            Self::Api { status: 401, .. } => 3,
            Self::Api { status: 403, .. } => 4,
            Self::NeedsConfirmation(_) => 5,
            _ => 1,
        }
    }

    pub fn http_status(&self) -> Option<u16> {
        match self {
            Self::Api { status, .. } => Some(*status),
            _ => None,
        }
    }
}

/// Writes the error as `error: {message}`, or as one JSON object with `json`.
pub fn report(err: &CliError, json: bool, stderr: &mut dyn Write) {
    let _ = if json {
        let value = serde_json::json!({"error": err.to_string(), "http_status": err.http_status()});
        writeln!(stderr, "{value}")
    } else {
        writeln!(stderr, "error: {err}")
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes() {
        let api = |status| CliError::Api {
            status,
            message: String::new(),
        };
        assert_eq!(api(401).exit_code(), 3);
        assert_eq!(api(403).exit_code(), 4);
        assert_eq!(api(409).exit_code(), 1);
        assert_eq!(CliError::Usage(String::new()).exit_code(), 2);
        assert_eq!(CliError::NeedsConfirmation(String::new()).exit_code(), 5);
        assert_eq!(CliError::Cancelled.exit_code(), 1);
    }

    #[test]
    fn reports_in_both_shapes() {
        let err = CliError::Api {
            status: 503,
            message: "db down".into(),
        };
        let mut human = Vec::new();
        report(&err, false, &mut human);
        assert_eq!(human, b"error: db down\n");
        let mut json = Vec::new();
        report(&err, true, &mut json);
        let v: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(v["error"], "db down");
        assert_eq!(v["http_status"], 503);
    }
}
