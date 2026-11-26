//! Tool argument structs. Doc comments become the schema descriptions the
//! model reads, so they say what each argument means.

use rmcp::schemars::JsonSchema;
use serde::Deserialize;

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
