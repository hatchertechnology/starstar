//! One module per command group.

use crate::args::{Cli, Command};
use crate::client::{ApiClient, parse_api_url};
use crate::error::CliResult;
use crate::output::Output;

pub mod account;

pub fn dispatch(cli: Cli, out: Output) -> CliResult<()> {
    let api = parse_api_url(&cli.api_url)?;
    match cli.command {
        Command::Status => {
            let client = ApiClient::unauthenticated(api.clone())?;
            account::status(&client, &api, out)
        }
    }
}
