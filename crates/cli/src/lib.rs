//! The starstar command-line interface.

pub mod args;
pub mod client;
pub mod commands;
pub mod credentials;
pub mod endpoints;
pub mod error;
pub mod output;

use clap::Parser;

use crate::args::Cli;
use crate::output::Output;

/// Parses the command line, runs the command and returns the exit code.
pub fn run() -> i32 {
    let json_requested = std::env::args().any(|a| a == "--json");
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            if err.use_stderr() && json_requested {
                let msg = err.render().to_string();
                let e = error::CliError::Usage(msg.trim_end().to_owned());
                error::report(&e, true, &mut std::io::stderr());
                return 2;
            }
            err.exit()
        }
    };
    let out = Output { json: cli.json };
    match commands::dispatch(cli, out) {
        Ok(()) => 0,
        Err(err) => {
            error::report(&err, out.json, &mut std::io::stderr());
            err.exit_code()
        }
    }
}
