//! `update …` commands.

use serde_json::json;
use starstar_shared::UpdateInfo;

use super::Ctx;
use super::printers::check_name;
use crate::args::UpdateCommand;
use crate::endpoints;
use crate::error::{CliError, CliResult};

pub fn run(ctx: &Ctx, command: UpdateCommand) -> CliResult<()> {
    match command {
        UpdateCommand::Check { printer, env } => {
            check_name(&printer)?;
            let info: UpdateInfo = ctx
                .client()?
                .get_json(&endpoints::update_check(&printer, &env))?;
            ctx.out
                .emit_value(&info, || match (&info.available, &info.new_version) {
                    (true, Some(v)) => format!("{printer}: update available ({v})"),
                    (true, None) => format!("{printer}: update available"),
                    _ => format!("{printer}: up to date"),
                });
        }
        UpdateCommand::Apply { printer, env, yes } => {
            check_name(&printer)?;
            // The spec's more specific rule wins: no prompt ever stands in for --yes.
            if !yes {
                return Err(CliError::NeedsConfirmation(
                    "applying a firmware update needs --yes".into(),
                ));
            }
            ctx.client()?
                .send_empty(&endpoints::update_apply(&printer, &env), None)?;
            ctx.out.emit(
                &json!({"printer": printer, "env": env, "action": "apply"}),
                || format!("applying the {env} update on {printer}"),
            );
        }
    }
    Ok(())
}
