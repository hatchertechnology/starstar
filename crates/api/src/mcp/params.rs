//! Tool argument structs. Doc comments become the schema descriptions the
//! model reads, so they say what each argument means.

use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use starstar_shared::{CameraSettings, PrinterKind, RegisterPrinter, Resolution, TriggerScheme};

/// Arguments of every tool that only needs a printer.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct PrinterArgs {
    /// Name of the registered printer, as shown by list_printers.
    pub printer: String,
}

/// Arguments of the job and transfer control tools.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct IdArgs {
    /// Name of the registered printer, as shown by list_printers.
    pub printer: String,
    /// Id of the job or transfer. Omit it to act on the one that is running now.
    pub id: Option<u64>,
}

/// Arguments of list_files.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct FilesArgs {
    /// Name of the registered printer, as shown by list_printers.
    pub printer: String,
    /// Storage device, for example `usb` (see list_storage).
    pub storage: String,
    /// File or folder path inside the storage. Omit it to list the storage root.
    pub path: Option<String>,
}

/// Arguments of the tools that act on one file.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct FileArgs {
    /// Name of the registered printer, as shown by list_printers.
    pub printer: String,
    /// Storage device, for example `usb` (see list_storage).
    pub storage: String,
    /// File path inside the storage, for example `jobs/cube.bgcode`.
    pub path: String,
}

/// Arguments of delete_file.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct DeleteFileArgs {
    /// Name of the registered printer, as shown by list_printers.
    pub printer: String,
    /// Storage device, for example `usb` (see list_storage).
    pub storage: String,
    /// File path inside the storage.
    pub path: String,
    /// Delete the file even if the printer is printing it. Default false.
    #[serde(default)]
    pub force: bool,
}

/// Arguments of upload_file.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct UploadArgs {
    /// Name of the registered printer, as shown by list_printers.
    pub printer: String,
    /// Storage device, for example `usb` (see list_storage).
    pub storage: String,
    /// Destination path inside the storage, for example `jobs/cube.bgcode`.
    pub path: String,
    /// The whole file, base64-encoded (standard alphabet; line breaks are ignored).
    pub content_base64: String,
    /// Replace a file that already exists. Default false.
    #[serde(default)]
    pub overwrite: bool,
    /// Start printing the file as soon as it has arrived. Default false.
    #[serde(default)]
    pub print: bool,
}

/// Arguments of the tools that act on one camera.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CameraArgs {
    /// Name of the registered printer, as shown by list_printers.
    pub printer: String,
    /// Camera id, as shown by list_cameras (for example `cam1`).
    pub id: String,
}

/// Arguments of camera_snapshot.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct SnapshotArgs {
    /// Name of the registered printer, as shown by list_printers.
    pub printer: String,
    /// Camera id. Omit it for the printer's default camera.
    pub id: Option<String>,
}

/// Arguments of order_cameras.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct OrderCamerasArgs {
    /// Name of the registered printer, as shown by list_printers.
    pub printer: String,
    /// Camera ids in the order they should appear; the first is the default camera.
    pub ids: Vec<String>,
}

/// Arguments of register_camera and configure_camera.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CameraSettingsArgs {
    /// Name of the registered printer, as shown by list_printers.
    pub printer: String,
    /// Camera id (for example `cam1`).
    pub id: String,
    /// Settings to set. Settings left out stay as they are.
    #[serde(flatten)]
    pub settings: McpCameraSettings,
}

fn default_env() -> String {
    "prusalink".to_owned()
}

/// Arguments of check_update and apply_update.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct UpdateArgs {
    /// Name of the registered printer, as shown by list_printers.
    pub printer: String,
    /// What to update. Only `prusalink` is supported; this is the default.
    #[serde(default = "default_env")]
    pub env: String,
}

/// Arguments of register_printer. Not `Debug`: it holds the printer password,
/// which passes through the conversation.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct RegisterPrinterArgs {
    /// Name for the printer: 1-63 lowercase letters, digits or dashes.
    pub name: String,
    /// Printer model.
    pub kind: McpPrinterKind,
    /// Hostname or IP address of the printer, optionally with a port.
    pub host: String,
    /// PrusaLink user name (usually `maker`).
    pub username: String,
    /// PrusaLink password. It passes through the conversation, so use a
    /// password that is only for starstar.
    pub password: String,
}

impl From<RegisterPrinterArgs> for RegisterPrinter {
    fn from(a: RegisterPrinterArgs) -> Self {
        Self {
            name: a.name,
            kind: a.kind.into(),
            host: a.host,
            username: a.username,
            password: a.password,
        }
    }
}

/// Printer models starstar can manage.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(rename_all = "lowercase")]
pub enum McpPrinterKind {
    Mk4,
    Mini,
}

impl From<McpPrinterKind> for PrinterKind {
    fn from(k: McpPrinterKind) -> Self {
        match k {
            McpPrinterKind::Mk4 => Self::Mk4,
            McpPrinterKind::Mini => Self::Mini,
        }
    }
}

/// When a camera takes pictures.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(rename_all = "snake_case")]
pub enum McpTriggerScheme {
    TenSec,
    ThirtySec,
    SixtySec,
    EachLayer,
    FifthLayer,
    Manual,
}

impl From<McpTriggerScheme> for TriggerScheme {
    fn from(s: McpTriggerScheme) -> Self {
        match s {
            McpTriggerScheme::TenSec => Self::TenSec,
            McpTriggerScheme::ThirtySec => Self::ThirtySec,
            McpTriggerScheme::SixtySec => Self::SixtySec,
            McpTriggerScheme::EachLayer => Self::EachLayer,
            McpTriggerScheme::FifthLayer => Self::FifthLayer,
            McpTriggerScheme::Manual => Self::Manual,
        }
    }
}

/// A camera resolution in pixels.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct McpResolution {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl From<McpResolution> for Resolution {
    fn from(r: McpResolution) -> Self {
        Self {
            width: r.width,
            height: r.height,
        }
    }
}

/// Camera settings. Every field is optional.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct McpCameraSettings {
    /// Display name.
    pub name: Option<String>,
    /// When the camera takes pictures.
    pub trigger_scheme: Option<McpTriggerScheme>,
    /// Picture size; must be one of the camera's available resolutions.
    pub resolution: Option<McpResolution>,
    /// Rotation in degrees.
    pub rotation: Option<i32>,
    /// Focus, if the camera supports it.
    pub focus: Option<f64>,
    /// Exposure, if the camera supports it.
    pub exposure: Option<f64>,
    /// Whether pictures go to Prusa Connect.
    pub send_to_connect: Option<bool>,
}

impl From<McpCameraSettings> for CameraSettings {
    fn from(s: McpCameraSettings) -> Self {
        Self {
            name: s.name,
            trigger_scheme: s.trigger_scheme.map(Into::into),
            resolution: s.resolution.map(Into::into),
            rotation: s.rotation,
            focus: s.focus,
            exposure: s.exposure,
            send_to_connect: s.send_to_connect,
        }
    }
}
