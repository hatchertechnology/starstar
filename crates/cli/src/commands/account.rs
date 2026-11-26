//! Account commands: status, login, logout, whoami, setup.

use starstar_shared::Health;
use url::Url;

use crate::client::ApiClient;
use crate::endpoints;
use crate::error::CliResult;
use crate::output::Output;

pub fn status(client: &ApiClient, api: &Url, out: Output) -> CliResult<()> {
    let health: Health = client.get_json(&endpoints::health())?;
    out.emit_value(&health, || {
        format!(
            "{} API {} is {} at {api}",
            health.app, health.version, health.status
        )
    });
    Ok(())
}
