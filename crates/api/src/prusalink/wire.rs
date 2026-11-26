//! Serde structs matching the JSON that PrusaLink sends and receives.

use serde::Deserialize;

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
