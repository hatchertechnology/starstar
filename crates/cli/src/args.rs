//! Command-line arguments.

use clap::{Parser, Subcommand};
use starstar_shared::{APP_NAME, DEFAULT_API_URL, Role};

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
    /// Sign in with your browser. On a machine without a browser, use --no-browser.
    Login {
        /// Print a URL and ask for the code it shows, instead of opening a browser.
        /// Use this on a machine without a browser.
        #[arg(long)]
        no_browser: bool,
        /// Highest role this sign-in may use: viewer, operator or admin.
        #[arg(long, default_value = "operator", value_parser = |s: &str| s.parse::<Role>())]
        role: Role,
    },
    /// Sign out and revoke this sign-in on the server.
    Logout,
    /// Show who you are signed in as.
    Whoami,
    /// Create the first admin account with the setup code the API printed.
    Setup {
        /// Username for the new admin.
        #[arg(long)]
        username: String,
        /// Read the setup code from stdin (one line) instead of prompting.
        #[arg(long)]
        setup_code_stdin: bool,
        /// Read the password from stdin (one line, after the setup code) instead of prompting.
        #[arg(long)]
        password_stdin: bool,
    },
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
