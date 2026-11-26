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
