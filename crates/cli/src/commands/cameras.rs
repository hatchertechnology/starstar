//! `camera …` commands.

use std::io::Write;

use serde_json::json;
use starstar_shared::{Camera, CameraDetail, CameraSettings};

use super::Ctx;
use super::printers::check_name;
use crate::args::CameraCommand;
use crate::endpoints::{self, Endpoint, validate_camera_id};
use crate::error::{CliError, CliResult};
use crate::output::or_dash;

fn camera_line(c: &Camera) -> String {
    format!(
        "{} {} connected={} detected={} linked={}",
        c.id,
        or_dash(c.name.as_ref()),
        c.connected,
        c.detected,
        c.linked
    )
}

pub fn run(ctx: &Ctx, command: CameraCommand) -> CliResult<()> {
    let out = ctx.out;
    match command {
        CameraCommand::Ls { printer } => {
            check_name(&printer)?;
            let cams: Vec<Camera> = ctx.client()?.get_json(&endpoints::cameras(&printer))?;
            out.emit_value(&cams, || {
                if cams.is_empty() {
                    "no cameras".to_owned()
                } else {
                    cams.iter().map(camera_line).collect::<Vec<_>>().join("\n")
                }
            });
        }
        CameraCommand::Show { printer, id } => {
            check_name(&printer)?;
            let ep = endpoints::camera(&printer, &id)?;
            let d: CameraDetail = ctx.client()?.get_json(&ep)?;
            out.emit_value(&d, || {
                let res = d
                    .resolution
                    .map_or_else(|| "-".to_owned(), |r| format!("{}x{}", r.width, r.height));
                format!(
                    "name:       {}\ntrigger:    {}\nresolution: {res}\nfocus:      {}\ncapabilities: {}",
                    or_dash(d.name.as_ref()),
                    d.trigger_scheme
                        .and_then(|t| serde_json::to_value(t).ok())
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .unwrap_or_else(|| "-".into()),
                    or_dash(d.focus),
                    d.capabilities.join(", ")
                )
            });
        }
        CameraCommand::Snap {
            printer,
            id,
            output,
            take,
        } => {
            check_name(&printer)?;
            if output == "-" && out.json {
                return Err(CliError::Usage(
                    "`-o -` writes the picture to stdout and cannot be combined with --json".into(),
                ));
            }
            let ep = match (&id, take) {
                (Some(id), true) => endpoints::take_snapshot(&printer, id)?,
                (id, _) => endpoints::snapshot(&printer, id.as_deref())?,
            };
            let (content_type, bytes) = ctx
                .client()?
                .get_bytes(&ep)?
                .ok_or_else(|| CliError::Other("camera has no snapshot yet".into()))?;
            if output == "-" {
                let mut stdout = std::io::stdout().lock();
                stdout.write_all(&bytes)?;
                stdout.flush()?;
            } else {
                std::fs::write(&output, &bytes)?;
                out.emit(
                    &json!({"printer": printer, "camera": id, "path": output, "bytes": bytes.len(), "content_type": content_type}),
                    || format!("saved {} bytes to {output}", bytes.len()),
                );
            }
        }
        CameraCommand::Order { printer, ids } => {
            check_name(&printer)?;
            for id in &ids {
                validate_camera_id(id)?;
            }
            ctx.client()?
                .send_empty(&endpoints::camera_order(&printer), Some(&json!(ids)))?;
            out.emit(&json!({"printer": printer, "order": ids}), || {
                format!("camera order on {printer}: {}", ids.join(" "))
            });
        }
        CameraCommand::Register {
            printer,
            id,
            settings,
        } => {
            check_name(&printer)?;
            let ep = endpoints::register_camera(&printer, &id)?;
            let body = settings_json(&settings.into_settings())?;
            write(ctx, &ep, Some(&body), &printer, &id, "register")?;
        }
        CameraCommand::Config {
            printer,
            id,
            settings,
        } => {
            check_name(&printer)?;
            let ep = endpoints::camera_config(&printer, &id)?;
            let body = settings_json(&settings.into_settings())?;
            if body.as_object().is_some_and(|o| o.is_empty()) {
                return Err(CliError::Usage(
                    "give at least one setting to change".into(),
                ));
            }
            write(ctx, &ep, Some(&body), &printer, &id, "config")?;
        }
        CameraCommand::Remove { printer, id } => {
            check_name(&printer)?;
            let ep = endpoints::remove_camera(&printer, &id)?;
            write(ctx, &ep, None, &printer, &id, "remove")?;
        }
        CameraCommand::Reset { printer, id } => {
            check_name(&printer)?;
            let ep = endpoints::reset_camera(&printer, &id)?;
            write(ctx, &ep, None, &printer, &id, "reset")?;
        }
        CameraCommand::Connect { printer, id } => {
            check_name(&printer)?;
            let ep = endpoints::connect_camera(&printer, &id)?;
            write(ctx, &ep, None, &printer, &id, "connect")?;
        }
        CameraCommand::Disconnect { printer, id } => {
            check_name(&printer)?;
            let ep = endpoints::disconnect_camera(&printer, &id)?;
            write(ctx, &ep, None, &printer, &id, "disconnect")?;
        }
    }
    Ok(())
}

fn settings_json(s: &CameraSettings) -> CliResult<serde_json::Value> {
    serde_json::to_value(s).map_err(|e| CliError::Other(e.to_string()))
}

fn write(
    ctx: &Ctx,
    ep: &Endpoint,
    body: Option<&serde_json::Value>,
    printer: &str,
    id: &str,
    action: &str,
) -> CliResult<()> {
    ctx.client()?.send_empty(ep, body)?;
    ctx.out.emit(
        &json!({"printer": printer, "camera": id, "action": action}),
        || format!("{action} camera {id} on {printer}: done"),
    );
    Ok(())
}
