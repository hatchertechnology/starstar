//! Camera and update tools.

use axum::http::request::Parts;
use rmcp::handler::server::common::Extension;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{ErrorData, tool, tool_router};

use crate::mcp::StarstarMcp;
use crate::mcp::params::{
    CameraArgs, CameraSettingsArgs, OrderCamerasArgs, PrinterArgs, SnapshotArgs, UpdateArgs,
};
use crate::mcp::results::{caller_of, image_ok, outcome, structured_ok, text_ok};
use crate::printers::{cameras, update};

#[tool_router(router = camera_tools, vis = "pub(super)")]
impl StarstarMcp {
    #[tool(
        description = "Lists the cameras of a printer.",
        annotations(read_only_hint = true)
    )]
    async fn list_cameras(
        &self,
        Parameters(args): Parameters<PrinterArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            cameras::list_cameras(&self.state, &caller, &args.printer).await,
            |v| structured_ok("cameras", v),
        ))
    }

    #[tool(
        description = "Shows one camera: its settings, available resolutions and capabilities.",
        annotations(read_only_hint = true)
    )]
    async fn get_camera(
        &self,
        Parameters(args): Parameters<CameraArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            cameras::camera_detail(&self.state, &caller, &args.printer, &args.id).await,
            |v| structured_ok("camera", v),
        ))
    }

    #[tool(
        description = "Returns the latest picture of a camera as an image, or says there is none yet. Omit id for the default camera.",
        annotations(read_only_hint = true)
    )]
    async fn camera_snapshot(
        &self,
        Parameters(args): Parameters<SnapshotArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            cameras::snapshot(&self.state, &caller, &args.printer, args.id.as_deref()).await,
            |snapshot| match snapshot {
                Some(s) => image_ok(s),
                None => text_ok("No picture yet."),
            },
        ))
    }

    #[tool(
        description = "Takes a picture with a camera now and returns it as an image.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn take_snapshot(
        &self,
        Parameters(args): Parameters<CameraArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            cameras::take_snapshot(&self.state, &caller, &args.printer, &args.id).await,
            image_ok,
        ))
    }

    #[tool(
        description = "Sets the order of a printer's cameras; the first one is the default.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn order_cameras(
        &self,
        Parameters(args): Parameters<OrderCamerasArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            cameras::order_cameras(&self.state, &caller, &args.printer, args.ids).await,
            |()| text_ok(format!("Reordered cameras on {}.", args.printer)),
        ))
    }

    #[tool(
        description = "Registers a camera on a printer with the given settings.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn register_camera(
        &self,
        Parameters(args): Parameters<CameraSettingsArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        let CameraSettingsArgs {
            printer,
            id,
            settings,
        } = args;
        Ok(outcome(
            cameras::register_camera(&self.state, &caller, &printer, &id, settings.into()).await,
            |()| text_ok(format!("Registered camera {id} on {printer}.")),
        ))
    }

    #[tool(
        description = "Removes a camera from a printer.",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn remove_camera(
        &self,
        Parameters(args): Parameters<CameraArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            cameras::remove_camera(&self.state, &caller, &args.printer, &args.id).await,
            |()| text_ok(format!("Removed camera {} on {}.", args.id, args.printer)),
        ))
    }

    #[tool(
        description = "Changes the settings of a camera. Settings left out stay as they are.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn configure_camera(
        &self,
        Parameters(args): Parameters<CameraSettingsArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        let CameraSettingsArgs {
            printer,
            id,
            settings,
        } = args;
        Ok(outcome(
            cameras::update_camera_config(&self.state, &caller, &printer, &id, settings.into())
                .await,
            |()| text_ok(format!("Configured camera {id} on {printer}.")),
        ))
    }

    #[tool(
        description = "Resets a camera's settings to the defaults.",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn reset_camera_config(
        &self,
        Parameters(args): Parameters<CameraArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            cameras::reset_camera_config(&self.state, &caller, &args.printer, &args.id).await,
            |()| {
                text_ok(format!(
                    "Reset the configuration of camera {} on {}.",
                    args.id, args.printer
                ))
            },
        ))
    }

    #[tool(
        description = "Connects a camera to the printer.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn connect_camera(
        &self,
        Parameters(args): Parameters<CameraArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            cameras::connect_camera(&self.state, &caller, &args.printer, &args.id).await,
            |()| text_ok(format!("Connected camera {} on {}.", args.id, args.printer)),
        ))
    }

    #[tool(
        description = "Disconnects a camera from the printer.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn disconnect_camera(
        &self,
        Parameters(args): Parameters<CameraArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            cameras::disconnect_camera(&self.state, &caller, &args.printer, &args.id).await,
            |()| {
                text_ok(format!(
                    "Disconnected camera {} on {}.",
                    args.id, args.printer
                ))
            },
        ))
    }

    #[tool(
        description = "Checks whether a PrusaLink update is available for a printer.",
        annotations(read_only_hint = true)
    )]
    async fn check_update(
        &self,
        Parameters(args): Parameters<UpdateArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            update::check_update(&self.state, &caller, &args.printer, &args.env).await,
            |v| structured_ok("update", v),
        ))
    }

    #[tool(
        description = "Starts the update on a printer. Only do this when the user has asked for it.",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn apply_update(
        &self,
        Parameters(args): Parameters<UpdateArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller = caller_of(&parts)?;
        Ok(outcome(
            update::apply_update(&self.state, &caller, &args.printer, &args.env).await,
            |()| {
                text_ok(format!(
                    "Started the {} update on {}.",
                    args.env, args.printer
                ))
            },
        ))
    }
}
