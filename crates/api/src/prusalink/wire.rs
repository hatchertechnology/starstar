//! Serde structs matching the JSON that PrusaLink sends and receives.

use serde::Deserialize;
use starstar_shared::{
    Camera, CameraDetail, CameraSettings, FileEntry, FileKind, Job, JobFile, PrintMeta, Resolution,
    Transfer, TriggerScheme,
};

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

/// Reads a size sent as a number or as a numeric string.
fn number_or_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Value {
        Int(u64),
        Float(f64),
        Text(String),
    }
    Option::<Value>::deserialize(d)?
        .map(|v| match v {
            Value::Int(n) => Ok(n),
            Value::Float(f) => seconds(f).ok_or_else(|| serde::de::Error::custom("bad size")),
            Value::Text(s) => s.trim().parse().map_err(serde::de::Error::custom),
        })
        .transpose()
}

/// `GET /api/v1/transfer`. The upstream body may lack `id`.
#[derive(Debug, Deserialize)]
pub struct WireTransfer {
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default, deserialize_with = "number_or_string")]
    pub size: Option<u64>,
    #[serde(default)]
    pub progress: f64,
    #[serde(default)]
    pub transferred: u64,
    #[serde(default)]
    pub time_remaining: Option<i64>,
    #[serde(default)]
    pub time_transferring: u64,
    #[serde(default)]
    pub to_print: bool,
}

impl From<WireTransfer> for Transfer {
    fn from(t: WireTransfer) -> Self {
        Self {
            id: t.id,
            kind: t.kind,
            display_name: t.display_name,
            path: t.path,
            url: t.url,
            size: t.size,
            progress: t.progress,
            transferred: t.transferred,
            time_remaining: t.time_remaining.and_then(|v| u64::try_from(v).ok()),
            time_transferring: t.time_transferring,
            to_print: t.to_print,
        }
    }
}

/// `GET /api/v1/files/{storage}/{path}`.
#[derive(Debug, Deserialize)]
pub struct WireFile {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub m_timestamp: Option<i64>,
    #[serde(default)]
    pub meta: Option<serde_json::Value>,
    #[serde(default)]
    pub children: Option<Vec<WireFile>>,
}

impl From<WireFile> for FileEntry {
    fn from(f: WireFile) -> Self {
        let kind = match f.kind.as_str() {
            "PRINT_FILE" => FileKind::PrintFile,
            "FIRMWARE" => FileKind::Firmware,
            "FILE" => FileKind::File,
            "FOLDER" => FileKind::Folder,
            _ => FileKind::Other,
        };
        // Only print files carry slicer metadata worth showing.
        let meta = match (kind, f.meta) {
            (FileKind::PrintFile, Some(value)) => serde_json::from_value::<WireMeta>(value)
                .ok()
                .map(PrintMeta::from),
            _ => None,
        };
        let children = (kind == FileKind::Folder).then(|| {
            f.children
                .unwrap_or_default()
                .into_iter()
                .map(FileEntry::from)
                .collect()
        });
        Self {
            name: f.name,
            display_name: f.display_name,
            kind,
            read_only: f.read_only,
            size: f.size,
            modified: f.m_timestamp,
            meta,
            children,
        }
    }
}

/// `GET /api/v1/cameras` answers a bare array, or `{"camera_list": [...]}`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum WireCameraList {
    Bare(Vec<WireCamera>),
    Wrapped { camera_list: Vec<WireCamera> },
}

impl WireCameraList {
    pub fn into_vec(self) -> Vec<WireCamera> {
        match self {
            Self::Bare(list) | Self::Wrapped { camera_list: list } => list,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct WireCamera {
    pub camera_id: String,
    #[serde(default)]
    pub config: Option<WireCameraInfo>,
    #[serde(default)]
    pub connected: bool,
    #[serde(default)]
    pub detected: bool,
    #[serde(default)]
    pub stored: bool,
    #[serde(default)]
    pub linked: bool,
}

#[derive(Debug, Default, Deserialize)]
pub struct WireCameraInfo {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub driver: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    /// Usually `"1280x720"`; some firmware sends `{width,height}`.
    #[serde(default)]
    pub resolution: Option<serde_json::Value>,
}

impl From<WireCamera> for Camera {
    fn from(c: WireCamera) -> Self {
        let config = c.config.unwrap_or_default();
        let resolution = config.resolution.and_then(|v| match v {
            serde_json::Value::String(s) => Some(s),
            serde_json::Value::Object(o) => {
                Some(format!("{}x{}", o.get("width")?, o.get("height")?))
            }
            _ => None,
        });
        Self {
            id: c.camera_id,
            name: config.name,
            driver: config.driver,
            path: config.path,
            resolution,
            connected: c.connected,
            detected: c.detected,
            stored: c.stored,
            linked: c.linked,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct WireCameraConfig {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub trigger_scheme: Option<String>,
    #[serde(default)]
    pub available_resolutions: Vec<Resolution>,
    #[serde(default)]
    pub resolution: Option<Resolution>,
    #[serde(default)]
    pub focus: Option<f64>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl From<WireCameraConfig> for CameraDetail {
    fn from(c: WireCameraConfig) -> Self {
        Self {
            name: c.name,
            trigger_scheme: c.trigger_scheme.as_deref().and_then(trigger_from_wire),
            available_resolutions: c.available_resolutions,
            resolution: c.resolution,
            focus: c.focus,
            capabilities: c
                .capabilities
                .into_iter()
                .map(|s| s.to_lowercase())
                .collect(),
        }
    }
}

/// PrusaLink spells trigger schemes in SCREAMING_SNAKE_CASE.
pub fn trigger_to_wire(scheme: TriggerScheme) -> &'static str {
    match scheme {
        TriggerScheme::TenSec => "TEN_SEC",
        TriggerScheme::ThirtySec => "THIRTY_SEC",
        TriggerScheme::SixtySec => "SIXTY_SEC",
        TriggerScheme::EachLayer => "EACH_LAYER",
        TriggerScheme::FifthLayer => "FIFTH_LAYER",
        TriggerScheme::Manual => "MANUAL",
    }
}

/// An unknown value gives `None`.
pub fn trigger_from_wire(value: &str) -> Option<TriggerScheme> {
    Some(match value {
        "TEN_SEC" => TriggerScheme::TenSec,
        "THIRTY_SEC" => TriggerScheme::ThirtySec,
        "SIXTY_SEC" => TriggerScheme::SixtySec,
        "EACH_LAYER" => TriggerScheme::EachLayer,
        "FIFTH_LAYER" => TriggerScheme::FifthLayer,
        "MANUAL" => TriggerScheme::Manual,
        _ => return None,
    })
}

/// Camera settings as PrusaLink takes them; `None` fields are omitted.
#[derive(Debug, Default, serde::Serialize)]
pub struct WireCameraSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_scheme: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<Resolution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exposure: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_to_connect: Option<bool>,
}

impl From<CameraSettings> for WireCameraSettings {
    fn from(s: CameraSettings) -> Self {
        Self {
            name: s.name,
            trigger_scheme: s.trigger_scheme.map(trigger_to_wire),
            resolution: s.resolution,
            rotation: s.rotation,
            focus: s.focus,
            exposure: s.exposure,
            send_to_connect: s.send_to_connect,
        }
    }
}
