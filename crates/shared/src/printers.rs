//! Printer operation types: jobs, transfers, files, cameras and updates.
//!
//! These are starstar's own vocabulary; the API translates PrusaLink's wire
//! types into them.

use serde::{Deserialize, Serialize};

/// The print job that is currently running. Times are in seconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub id: u64,
    pub state: String,
    pub progress: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_remaining: Option<u64>,
    pub time_printing: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inaccurate_estimates: Option<bool>,
    pub serial_print: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<JobFile>,
}

/// The file a job prints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobFile {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Modification time, seconds since the epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<PrintMeta>,
}

/// Slicer metadata of a print file.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PrintMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_print_time: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filament_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nozzle_diameter: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub printer_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nozzle_temp: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bed_temp: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filament_used_g: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filament_used_mm: Option<f64>,
}

/// A file transfer to the printer in progress.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transfer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    /// Upstream `type`, e.g. `FROM_CLIENT`.
    pub kind: String,
    pub display_name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    pub progress: f64,
    pub transferred: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_remaining: Option<u64>,
    pub time_transferring: u64,
    pub to_print: bool,
}

/// What a file-system entry on the printer is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    PrintFile,
    Firmware,
    File,
    Folder,
    Other,
}

/// A file or folder on a printer's storage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub kind: FileKind,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Modification time, seconds since the epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<PrintMeta>,
    /// Present only for folders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<FileEntry>>,
}

/// The answer to a `HEAD` on a file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileState {
    pub read_only: bool,
    pub currently_printed: bool,
}
