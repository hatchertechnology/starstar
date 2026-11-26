//! Printer registry and telemetry tools.

use axum::http::request::Parts;
use rmcp::handler::server::common::Extension;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{ErrorData, tool, tool_router};

use crate::mcp::StarstarMcp;
use crate::mcp::params::{PrinterArgs, RegisterPrinterArgs};
use crate::mcp::results::{caller_of, outcome, structured_ok, text_ok};
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

    #[tool(
        description = "Shows one registered printer: its kind, host and the user name starstar signs in with.",
        annotations(read_only_hint = true)
    )]
    async fn get_printer(
        &self,
        Parameters(args): Parameters<PrinterArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            registry::show_printer(&self.state, &caller, &args.printer).await,
            |v| structured_ok("printer", v),
        ))
    }

    #[tool(
        description = "Shows a printer's identity: firmware, hostname, serial number, nozzle diameter and MMU.",
        annotations(read_only_hint = true)
    )]
    async fn printer_info(
        &self,
        Parameters(args): Parameters<PrinterArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            telemetry::printer_info(&self.state, &caller, &args.printer).await,
            |v| structured_ok("info", v),
        ))
    }

    #[tool(
        description = "Lists a printer's storage devices (for example usb) with free space.",
        annotations(read_only_hint = true)
    )]
    async fn list_storage(
        &self,
        Parameters(args): Parameters<PrinterArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            telemetry::printer_storage(&self.state, &caller, &args.printer).await,
            |v| structured_ok("storage", v),
        ))
    }

    #[tool(
        description = "Registers a printer after checking that starstar can reach it with the given credentials. The password is stored by starstar and never shown again.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn register_printer(
        &self,
        Parameters(args): Parameters<RegisterPrinterArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            registry::register_printer(&self.state, &caller, args.into()).await,
            |v| structured_ok("printer", v),
        ))
    }

    #[tool(
        description = "Removes a printer from starstar. The printer itself is not changed.",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn remove_printer(
        &self,
        Parameters(args): Parameters<PrinterArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            registry::remove_printer(&self.state, &caller, &args.printer).await,
            |()| text_ok(format!("Removed printer {}.", args.printer)),
        ))
    }
}
