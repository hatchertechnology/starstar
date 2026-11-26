//! Job and transfer tools.

use axum::http::request::Parts;
use rmcp::handler::server::common::Extension;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{ErrorData, tool, tool_router};
use starstar_shared::Role;

use crate::auth::Caller;
use crate::error::ApiError;
use crate::mcp::StarstarMcp;
use crate::mcp::params::{IdArgs, PrinterArgs};
use crate::mcp::results::{caller_of, outcome, structured_ok, text_ok, tool_try};
use crate::printers::{jobs, transfer};

impl StarstarMcp {
    /// The job to act on: `id` if given, else the running job. `role` is the
    /// role of the final operation (the same constant that function checks),
    /// so a caller below it is refused before any lookup reaches the printer.
    async fn resolve_job_id(
        &self,
        caller: &Caller,
        printer: &str,
        id: Option<u64>,
        role: Role,
    ) -> Result<u64, ApiError> {
        caller.require(role)?;
        match id {
            Some(id) => Ok(id),
            None => jobs::current_job(&self.state, caller, printer)
                .await?
                .map(|job| job.id)
                .ok_or_else(|| ApiError::Conflict(format!("no job is running on {printer}"))),
        }
    }

    /// Like [`Self::resolve_job_id`], for the running transfer.
    async fn resolve_transfer_id(
        &self,
        caller: &Caller,
        printer: &str,
        id: Option<u64>,
        role: Role,
    ) -> Result<u64, ApiError> {
        caller.require(role)?;
        match id {
            Some(id) => Ok(id),
            None => transfer::current_transfer(&self.state, caller, printer)
                .await?
                .and_then(|t| t.id)
                .ok_or_else(|| ApiError::Conflict(format!("no transfer is running on {printer}"))),
        }
    }
}

#[tool_router(router = job_tools, vis = "pub(super)")]
impl StarstarMcp {
    #[tool(
        description = "Shows the job that is running on a printer, or null when it is idle.",
        annotations(read_only_hint = true)
    )]
    async fn get_job(
        &self,
        Parameters(args): Parameters<PrinterArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            jobs::current_job(&self.state, &caller, &args.printer).await,
            |v| structured_ok("job", v),
        ))
    }

    #[tool(
        description = "Pauses a print job. Omit id to act on the current job.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn pause_job(
        &self,
        Parameters(args): Parameters<IdArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        let id = tool_try!(
            self.resolve_job_id(&caller, &args.printer, args.id, jobs::PAUSE_JOB_ROLE)
                .await
        );
        Ok(outcome(
            jobs::pause_job(&self.state, &caller, &args.printer, id).await,
            |()| text_ok(format!("Paused job {id} on {}.", args.printer)),
        ))
    }

    #[tool(
        description = "Resumes a paused print job. Omit id to act on the current job.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn resume_job(
        &self,
        Parameters(args): Parameters<IdArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        let id = tool_try!(
            self.resolve_job_id(&caller, &args.printer, args.id, jobs::RESUME_JOB_ROLE)
                .await
        );
        Ok(outcome(
            jobs::resume_job(&self.state, &caller, &args.printer, id).await,
            |()| text_ok(format!("Resumed job {id} on {}.", args.printer)),
        ))
    }

    #[tool(
        description = "Continues a print job that waits for the user (for example after a filament change). Omit id to act on the current job.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn continue_job(
        &self,
        Parameters(args): Parameters<IdArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        let id = tool_try!(
            self.resolve_job_id(&caller, &args.printer, args.id, jobs::CONTINUE_JOB_ROLE)
                .await
        );
        Ok(outcome(
            jobs::continue_job(&self.state, &caller, &args.printer, id).await,
            |()| text_ok(format!("Continued job {id} on {}.", args.printer)),
        ))
    }

    #[tool(
        description = "Stops (cancels) a print job for good. Omit id to act on the current job.",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn stop_job(
        &self,
        Parameters(args): Parameters<IdArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        let id = tool_try!(
            self.resolve_job_id(&caller, &args.printer, args.id, jobs::STOP_JOB_ROLE)
                .await
        );
        Ok(outcome(
            jobs::stop_job(&self.state, &caller, &args.printer, id).await,
            |()| text_ok(format!("Stopped job {id} on {}.", args.printer)),
        ))
    }

    #[tool(
        description = "Shows the file transfer to the printer that is in progress, or null.",
        annotations(read_only_hint = true)
    )]
    async fn get_transfer(
        &self,
        Parameters(args): Parameters<PrinterArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            transfer::current_transfer(&self.state, &caller, &args.printer).await,
            |v| structured_ok("transfer", v),
        ))
    }

    #[tool(
        description = "Cancels a file transfer. Omit id to act on the current transfer.",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn cancel_transfer(
        &self,
        Parameters(args): Parameters<IdArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        let id = tool_try!(
            self.resolve_transfer_id(
                &caller,
                &args.printer,
                args.id,
                transfer::CANCEL_TRANSFER_ROLE
            )
            .await
        );
        Ok(outcome(
            transfer::cancel_transfer(&self.state, &caller, &args.printer, id).await,
            |()| text_ok(format!("Cancelled transfer {id} on {}.", args.printer)),
        ))
    }
}
