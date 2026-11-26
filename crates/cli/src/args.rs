//! Command-line arguments.

use clap::{Parser, Subcommand};
use starstar_shared::{APP_NAME, DEFAULT_API_URL};

/// Manage 3D printers through the starstar API.
#[derive(Debug, Parser)]
#[command(name = APP_NAME, version, about)]
pub struct Cli {
    /// Base URL of the starstar API.
    #[arg(long, env = "STARSTAR_API_URL", default_value = DEFAULT_API_URL, global = true)]
    pub api_url: String,

    /// Print results (and errors) as JSON.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check that the API is reachable.
    Status,
    /// List and inspect printers.
    #[command(subcommand)]
    Printer(PrinterCommand),
}

#[derive(Debug, Subcommand)]
pub enum PrinterCommand {
    /// List registered printers.
    List,
    /// Show a registered printer's connection details.
    Show {
        /// Printer name.
        name: String,
    },
    /// Show firmware and hardware details from the printer.
    Info {
        /// Printer name.
        name: String,
    },
    /// Show the printer's state and temperatures.
    Status {
        /// Printer name.
        name: String,
    },
    /// List storage devices on the printer.
    Storage {
        /// Printer name.
        name: String,
    },
}
