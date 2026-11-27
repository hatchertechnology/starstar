//! Types and helpers shared by the starstar CLI and API.
//!
//! These types are the API's own vocabulary. The API translates each printer's
//! native responses into them, so clients never need to know about PrusaLink.

pub mod library;
pub use library::LibraryKind;
pub mod printers;

pub use printers::*;

use std::fmt;
use std::str::FromStr;

pub mod auth;

use serde::{Deserialize, Serialize};

/// Application name, used in banners and API responses.
pub const APP_NAME: &str = "starstar";

/// Address of the starstar API when none is configured.
pub const DEFAULT_API_URL: &str = "http://127.0.0.1:8080";

/// Printer models the API can manage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrinterKind {
    Mk4,
    Mini,
}

impl PrinterKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mk4 => "mk4",
            Self::Mini => "mini",
        }
    }
}

impl fmt::Display for PrinterKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned when a string is not a known [`PrinterKind`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownPrinterKind(String);

impl fmt::Display for UnknownPrinterKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown printer kind `{}`, expected `mk4` or `mini`",
            self.0
        )
    }
}

impl std::error::Error for UnknownPrinterKind {}

impl FromStr for PrinterKind {
    type Err = UnknownPrinterKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "mk4" => Ok(Self::Mk4),
            "mini" => Ok(Self::Mini),
            _ => Err(UnknownPrinterKind(s.to_owned())),
        }
    }
}

/// Access levels, ordered from least to most privileged.
///
/// Every API endpoint and MCP tool requires a minimum role. A caller's
/// effective role is the lower of its token grant and its user's current role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Read printers, status, jobs, files, transfers, cameras and snapshots.
    Viewer,
    /// Viewer, plus upload, start/pause/resume/continue/stop jobs, cancel transfers, take snapshots.
    Operator,
    /// Operator, plus printer registry, file deletion, camera settings, firmware updates, user management.
    Admin,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Operator => "operator",
            Self::Admin => "admin",
        }
    }

    /// The OAuth scope that grants this role, e.g. `starstar:operator`.
    pub fn scope(self) -> &'static str {
        match self {
            Self::Viewer => "starstar:viewer",
            Self::Operator => "starstar:operator",
            Self::Admin => "starstar:admin",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned when a string is not a known [`Role`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownRole(String);

impl fmt::Display for UnknownRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown role `{}`, expected `viewer`, `operator` or `admin`",
            self.0
        )
    }
}

impl std::error::Error for UnknownRole {}

impl FromStr for Role {
    type Err = UnknownRole;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "viewer" => Ok(Self::Viewer),
            "operator" => Ok(Self::Operator),
            "admin" => Ok(Self::Admin),
            _ => Err(UnknownRole(s.to_owned())),
        }
    }
}

/// Printer names are the identifiers used in API paths: 1 to 63 lowercase
/// ASCII letters, digits or dashes, not starting with a dash.
pub fn is_valid_printer_name(name: &str) -> bool {
    (1..=63).contains(&name.len())
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Request body for registering a printer. Contains the password, so it is
/// intentionally not `Debug`.
#[derive(Clone, Serialize, Deserialize)]
pub struct RegisterPrinter {
    pub name: String,
    pub kind: PrinterKind,
    /// Hostname or IP address, optionally with a port.
    pub host: String,
    pub username: String,
    pub password: String,
}

/// A registered printer. Never includes credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrinterSummary {
    pub name: String,
    pub kind: PrinterKind,
    pub host: String,
    pub username: String,
}

/// Firmware and hardware details for a printer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrinterInfo {
    pub name: String,
    pub kind: PrinterKind,
    pub firmware: String,
    pub api_version: String,
    pub hostname: Option<String>,
    pub serial: Option<String>,
    pub nozzle_diameter: Option<f64>,
    pub mmu: Option<bool>,
}

/// Current state and temperatures of a printer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrinterStatus {
    pub name: String,
    pub state: String,
    pub nozzle_temp: Option<f64>,
    pub nozzle_target: Option<f64>,
    pub bed_temp: Option<f64>,
    pub bed_target: Option<f64>,
}

/// A storage device attached to a printer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StorageDevice {
    pub name: Option<String>,
    pub kind: String,
    pub path: String,
    pub available: bool,
    pub read_only: Option<bool>,
    pub free_space: Option<u64>,
    pub total_space: Option<u64>,
}

/// Response from the API health endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Health {
    pub status: String,
    pub app: String,
    pub version: String,
}

impl Health {
    pub fn ok() -> Self {
        Self {
            status: "ok".to_owned(),
            app: APP_NAME.to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }
}

/// Error body returned by the API with every non-success response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_name_is_starstar() {
        assert_eq!(APP_NAME, "starstar");
    }

    #[test]
    fn parses_known_printer_kinds() {
        assert_eq!("mk4".parse::<PrinterKind>(), Ok(PrinterKind::Mk4));
        assert_eq!("MINI".parse::<PrinterKind>(), Ok(PrinterKind::Mini));
        assert!("i3".parse::<PrinterKind>().is_err());
    }

    #[test]
    fn roles_are_ordered_and_parse() {
        assert!(Role::Viewer < Role::Operator && Role::Operator < Role::Admin);
        assert_eq!("Operator".parse::<Role>(), Ok(Role::Operator));
        assert_eq!(Role::Admin.scope(), "starstar:admin");
        assert!("root".parse::<Role>().is_err());
    }

    #[test]
    fn validates_printer_names() {
        assert!(is_valid_printer_name("mk4"));
        assert!(is_valid_printer_name("office-mini-2"));
        assert!(!is_valid_printer_name(""));
        assert!(!is_valid_printer_name("-mk4"));
        assert!(!is_valid_printer_name("MK4"));
        assert!(!is_valid_printer_name("mk4/info"));
        assert!(!is_valid_printer_name(&"a".repeat(64)));
    }
}
