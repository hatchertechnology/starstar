//! The tools, grouped by area. Each file has its own router; [`all_tools`]
//! adds them together.

use rmcp::handler::server::router::tool::ToolRouter;

use super::StarstarMcp;

mod cameras;
mod files;
mod jobs;
mod parts;
mod printers;

pub(crate) fn all_tools() -> ToolRouter<StarstarMcp> {
    StarstarMcp::part_tools()
        + StarstarMcp::printer_tools()
        + StarstarMcp::job_tools()
        + StarstarMcp::file_tools()
        + StarstarMcp::camera_tools()
}

/// The description of `upload_file`, with the limit this server is set to.
pub(crate) fn upload_description(max: u64) -> String {
    format!(
        "Uploads a file to printer storage. content_base64 is the whole file, base64-encoded; files over the server's MCP upload limit ({}) must go through the starstar CLI instead.",
        super::results::format_limit(max)
    )
}
