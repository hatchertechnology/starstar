//! Printer registry and telemetry tools.

use axum::http::request::Parts;
use rmcp::handler::server::common::Extension;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{ErrorData, tool, tool_router};

use crate::mcp::StarstarMcp;
use crate::mcp::params::PrinterArgs;
use crate::mcp::results::{caller_of, outcome, structured_ok};
use crate::printers::{registry, telemetry};

#[tool_router(router = printer_tools, vis = "pub(super)")]
impl StarstarMcp {
    #[tool(
        description = "Lists the printers registered with starstar.",
        annotations(read_only_hint = true)
    )]
    async fn list_printers(
        &self,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            registry::list_printers(&self.state, &caller).await,
            |v| structured_ok("printers", v),
        ))
    }

    #[tool(
        description = "Shows a printer's state and its nozzle and bed temperatures.",
        annotations(read_only_hint = true)
    )]
    async fn printer_status(
        &self,
        Parameters(args): Parameters<PrinterArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            telemetry::printer_status(&self.state, &caller, &args.printer).await,
            |v| structured_ok("status", v),
        ))
    }
}
