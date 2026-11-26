//! The tools, grouped by area. Each file has its own router; [`all_tools`]
//! adds them together.

use rmcp::handler::server::router::tool::ToolRouter;

use super::StarstarMcp;

mod printers;

pub(crate) fn all_tools() -> ToolRouter<StarstarMcp> {
    StarstarMcp::printer_tools()
}
