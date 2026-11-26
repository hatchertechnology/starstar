//! One module per command group.

use std::io::BufRead;

use url::Url;

use crate::args::{Cli, Command};
use crate::client::{ApiClient, parse_api_url};
use crate::error::{CliError, CliResult};
use crate::output::Output;

pub mod account;
pub mod cameras;
pub mod files;
pub mod jobs;
pub mod printers;
pub mod update;
pub mod users;

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

/// Reads one line from stdin without its line ending.
pub fn stdin_line() -> CliResult<String> {
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim_end_matches(['\r', '\n']).to_owned())
}

/// Reads a secret from stdin (one line) or, without `from_stdin`, from a
/// prompt that does not echo it.
pub fn read_secret(prompt: &str, from_stdin: bool) -> CliResult<String> {
    if from_stdin {
        stdin_line()
    } else {
        Ok(rpassword::prompt_password(prompt)?)
    }
}

/// The API's password rule, checked early so a bad one sends nothing.
pub fn check_password(password: &str) -> CliResult<()> {
    if (8..=256).contains(&password.chars().count()) {
        Ok(())
    } else {
        Err(CliError::Usage(
            "passwords must be 8 to 256 characters".into(),
        ))
    }
}

pub fn dispatch(cli: Cli, out: Output) -> CliResult<()> {
    let ctx = Ctx {
        api: parse_api_url(&cli.api_url)?,
        out,
    };
    match cli.command {
        Command::Status => account::status(&ctx),
        Command::Login { no_browser, role } => account::login(&ctx, no_browser, role),
        Command::Logout => account::logout(&ctx),
        Command::Whoami => account::whoami(&ctx),
        Command::Setup {
            username,
            setup_code_stdin,
            password_stdin,
        } => account::setup(&ctx, username, setup_code_stdin, password_stdin),
        Command::Printer(c) => printers::run(&ctx, c),
        Command::User(c) => users::run_user(&ctx, c),
        Command::Apps(c) => users::run_apps(&ctx, c),
        Command::Update(c) => update::run(&ctx, c),
        Command::Camera(c) => cameras::run(&ctx, c),
        Command::Files(c) => files::run(&ctx, c),
        Command::Job(c) => jobs::run_job(&ctx, c),
        Command::Transfer(c) => jobs::run_transfer(&ctx, c),
    }
}
