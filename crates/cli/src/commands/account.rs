//! Account commands: status, login, logout, whoami, setup.

use starstar_shared::Health;

use super::Ctx;
use crate::endpoints;
use crate::error::CliResult;

pub fn status(ctx: &Ctx) -> CliResult<()> {
    let health: Health = ctx.public_client()?.get_json(&endpoints::health())?;
    ctx.out.emit_value(&health, || {
        format!(
            "{} API {} is {} at {}",
            health.app, health.version, health.status, ctx.api
        )
    });
    Ok(())
}
