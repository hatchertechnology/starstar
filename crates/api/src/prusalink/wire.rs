//! Serde structs matching the JSON that PrusaLink sends and receives.

use serde::Deserialize;
use starstar_shared::{Job, JobFile, PrintMeta};

/// `GET /api/version`
#[derive(Debug, Deserialize)]
pub struct Version {
    pub api: String,
    pub firmware: String,
}

/// `GET /api/v1/info`
#[derive(Debug, Deserialize)]
pub struct Info {
    pub hostname: Option<String>,
    pub serial: Option<String>,
    pub nozzle_diameter: Option<f64>,
    pub mmu: Option<bool>,
}

/// `GET /api/v1/status`
#[derive(Debug, Deserialize)]
pub struct Status {
    pub printer: StatusPrinter,
    #[serde(default)]
    pub job: Option<StatusJob>,
    #[serde(default)]
    pub transfer: Option<StatusTransfer>,
}

#[derive(Debug, Deserialize)]
pub struct StatusPrinter {
    pub state: String,
    pub temp_nozzle: Option<f64>,
    pub target_nozzle: Option<f64>,
    pub temp_bed: Option<f64>,
    pub target_bed: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct StatusJob {
    #[serde(default)]
    pub id: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct StatusTransfer {
    #[serde(default)]
    pub id: Option<u64>,
}

/// `GET /api/v1/storage`
#[derive(Debug, Deserialize)]
pub struct StorageList {
    pub storage_list: Vec<Storage>,
}

#[derive(Debug, Deserialize)]
pub struct Storage {
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
    pub path: String,
    pub available: bool,
    pub read_only: Option<bool>,
    pub free_space: Option<u64>,
    pub total_space: Option<u64>,
}

/// `GET /api/v1/job`. Serial-print jobs lack a file and most other fields.
#[derive(Debug, Deserialize)]
pub struct WireJob {
    pub id: u64,
    pub state: String,
    pub progress: f64,
    #[serde(default)]
    pub time_remaining: Option<i64>,
    #[serde(default)]
    pub time_printing: Option<i64>,
    #[serde(default)]
    pub inaccurate_estimates: Option<bool>,
    #[serde(default)]
    pub serial_print: bool,
    #[serde(default)]
    pub file: Option<WireJobFile>,
}

#[derive(Debug, Deserialize)]
pub struct WireJobFile {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub display_path: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub m_timestamp: Option<i64>,
    #[serde(default)]
    pub meta: Option<WireMeta>,
}

/// Slicer metadata as PrusaLink spells it.
#[derive(Debug, Default, Deserialize)]
pub struct WireMeta {
    #[serde(default)]
    pub estimated_print_time: Option<f64>,
    #[serde(default)]
    pub filament_type: Option<String>,
    #[serde(default)]
    pub layer_height: Option<f64>,
    #[serde(default)]
    pub nozzle_diameter: Option<f64>,
    #[serde(default)]
    pub printer_model: Option<String>,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub bed_temperature: Option<f64>,
    #[serde(default, rename = "filament used [g]")]
    pub filament_used_g: Option<f64>,
    #[serde(default, rename = "filament used [mm]")]
    pub filament_used_mm: Option<f64>,
}

fn seconds(value: f64) -> Option<u64> {
    (value.is_finite() && value >= 0.0).then_some(value as u64)
}

impl From<WireMeta> for PrintMeta {
    fn from(m: WireMeta) -> Self {
        Self {
            estimated_print_time: m.estimated_print_time.and_then(seconds),
            filament_type: m.filament_type,
            layer_height: m.layer_height,
            nozzle_diameter: m.nozzle_diameter,
            printer_model: m.printer_model,
            nozzle_temp: m.temperature,
            bed_temp: m.bed_temperature,
            filament_used_g: m.filament_used_g,
            filament_used_mm: m.filament_used_mm,
        }
    }
}

impl From<WireJobFile> for JobFile {
    fn from(f: WireJobFile) -> Self {
        Self {
            name: f.name,
            display_name: f.display_name,
            path: f.path,
            display_path: f.display_path,
            size: f.size,
            modified: f.m_timestamp,
            meta: f.meta.map(PrintMeta::from),
        }
    }
}

impl From<WireJob> for Job {
    fn from(j: WireJob) -> Self {
        Self {
            id: j.id,
            state: j.state,
            progress: j.progress,
            // PrusaLink uses negative numbers for "unknown".
            time_remaining: j.time_remaining.and_then(|t| u64::try_from(t).ok()),
            time_printing: j
                .time_printing
                .and_then(|t| u64::try_from(t).ok())
                .unwrap_or(0),
            inaccurate_estimates: j.inaccurate_estimates,
            serial_print: j.serial_print,
            file: j.file.map(JobFile::from),
        }
    }
}
