//! Command-line arguments.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use starstar_shared::PrinterKind;

use crate::confirm::ConfirmArgs;
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
    /// Show and control the running print job.
    #[command(subcommand)]
    Job(JobCommand),
    /// Show and cancel a file transfer to the printer.
    #[command(subcommand)]
    Transfer(TransferCommand),
    /// List, inspect, upload, print and delete files on a printer.
    #[command(subcommand)]
    Files(FilesCommand),
    /// List and inspect printers.
    #[command(subcommand)]
    Printer(PrinterCommand),
}

#[derive(Debug, Subcommand)]
pub enum PrinterCommand {
    /// Register a printer. The API checks it can reach the printer first.
    Add(AddArgs),
    /// Remove a printer from starstar. The printer itself is not changed. Needs --yes without a terminal.
    Remove {
        /// Printer name.
        name: String,
        #[command(flatten)]
        confirm: ConfirmArgs,
    },
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

#[derive(Debug, Args)]
pub struct AddArgs {
    /// Unique name for the printer, e.g. `office-mk4`.
    pub name: String,
    /// Printer model: `mk4` or `mini`.
    #[arg(long, value_parser = |s: &str| s.parse::<PrinterKind>())]
    pub kind: PrinterKind,
    /// Hostname or IP address, optionally with a port.
    #[arg(long)]
    pub host: String,
    /// PrusaLink username.
    #[arg(long)]
    pub username: String,
    /// Read the password from stdin instead of prompting for it.
    #[arg(long)]
    pub password_stdin: bool,
}

#[derive(Debug, Subcommand)]
pub enum JobCommand {
    /// Show the current job, if any.
    Show {
        /// Printer name.
        printer: String,
    },
    /// Pause the current job.
    Pause {
        /// Printer name.
        printer: String,
        /// Job id. Without it, the current job is looked up first.
        #[arg(long)]
        id: Option<u64>,
    },
    /// Resume a paused job.
    Resume {
        /// Printer name.
        printer: String,
        /// Job id. Without it, the current job is looked up first.
        #[arg(long)]
        id: Option<u64>,
    },
    /// Continue a job that is waiting for the user (for example after a filament change).
    Continue {
        /// Printer name.
        printer: String,
        /// Job id. Without it, the current job is looked up first.
        #[arg(long)]
        id: Option<u64>,
    },
    /// Stop (cancel) the current job. Needs --yes without a terminal.
    Stop {
        /// Printer name.
        printer: String,
        /// Job id. Without it, the current job is looked up first.
        #[arg(long)]
        id: Option<u64>,
        #[command(flatten)]
        confirm: ConfirmArgs,
    },
}

#[derive(Debug, Subcommand)]
pub enum TransferCommand {
    /// Show the file transfer in progress, if any.
    Show {
        /// Printer name.
        printer: String,
    },
    /// Cancel the file transfer in progress.
    Cancel {
        /// Printer name.
        printer: String,
        /// Transfer id. Without it, the current transfer is looked up first.
        #[arg(long)]
        id: Option<u64>,
    },
}

#[derive(Debug, Subcommand)]
pub enum FilesCommand {
    /// List a folder: `<storage>` or `<storage>/<path>`.
    Ls {
        /// Printer name.
        printer: String,
        /// Storage and folder, e.g. `usb` or `usb/parts`.
        remote: String,
    },
    /// Show a file's details and whether it is read-only or printing.
    Info {
        /// Printer name.
        printer: String,
        /// File, e.g. `usb/cube.bgcode`.
        remote: String,
    },
    /// Upload a local file to the printer.
    Upload {
        /// Printer name.
        printer: String,
        /// Destination: a folder (`usb` or `usb/parts/`) or a file path (`usb/cube.bgcode`).
        remote: String,
        /// Local file to upload.
        local: PathBuf,
        /// Replace a file of the same name.
        #[arg(long)]
        overwrite: bool,
        /// Start printing as soon as the upload finishes.
        #[arg(long)]
        print: bool,
    },
    /// Start printing a file on the printer.
    Print {
        /// Printer name.
        printer: String,
        /// File, e.g. `usb/cube.bgcode`.
        remote: String,
    },
    /// Delete a file or folder. Needs --yes without a terminal.
    Rm {
        /// Printer name.
        printer: String,
        /// File or folder, e.g. `usb/cube.bgcode`.
        remote: String,
        /// Delete even if read-only or in use, and folders with everything in them.
        #[arg(long)]
        force: bool,
        #[command(flatten)]
        confirm: ConfirmArgs,
    },
}
