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
