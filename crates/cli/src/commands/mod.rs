//! One module per command group.

use url::Url;

use crate::args::{Cli, Command};
use crate::client::{ApiClient, parse_api_url};
use crate::error::CliResult;
use crate::output::Output;

pub mod account;
pub mod printers;

/// What every command needs. Clients are built on demand, so a command can
/// refuse (for example without `--yes`) before anything touches a token.
pub struct Ctx {
    pub api: Url,
    pub out: Output,
}

impl Ctx {
    /// A signed-in client.
    pub fn client(&self) -> CliResult<ApiClient> {
        ApiClient::from_env(self.api.clone())
    }

    /// A client that sends no credential.
    pub fn public_client(&self) -> CliResult<ApiClient> {
        ApiClient::unauthenticated(self.api.clone())
    }
}

pub fn dispatch(cli: Cli, out: Output) -> CliResult<()> {
    let ctx = Ctx {
        api: parse_api_url(&cli.api_url)?,
        out,
    };
    match cli.command {
        Command::Status => account::status(&ctx),
        Command::Printer(c) => printers::run(&ctx, c),
    }
}
