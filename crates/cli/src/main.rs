use clap::{Parser, Subcommand};
use starstar_shared::APP_NAME;

/// Manage 3D printers from the command line.
#[derive(Debug, Parser)]
#[command(name = APP_NAME, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show the status of the API layer.
    Status,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Status => println!("{}", starstar_api::status()),
    }
}
