//! Command-line arguments.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use starstar_shared::{CameraSettings, PrinterKind, Resolution, TriggerScheme};

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
    /// List, show, configure and take pictures with the printer's cameras.
    #[command(subcommand)]
    Camera(CameraCommand),
    /// Check for and apply printer firmware updates.
    #[command(subcommand)]
    Update(UpdateCommand),
    /// Manage users (admin) and your own password.
    #[command(subcommand)]
    User(UserCommand),
    /// List and revoke connected apps (OAuth sign-ins).
    #[command(subcommand)]
    Apps(AppsCommand),
    /// Register, list, inspect and remove printers.
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
        /// Delete a folder even if it is not empty.
        #[arg(long)]
        force: bool,
        #[command(flatten)]
        confirm: ConfirmArgs,
    },
}

/// When a camera takes pictures.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum TriggerArg {
    #[value(alias = "ten_sec")]
    TenSec,
    #[value(alias = "thirty_sec")]
    ThirtySec,
    #[value(alias = "sixty_sec")]
    SixtySec,
    #[value(alias = "each_layer")]
    EachLayer,
    #[value(alias = "fifth_layer")]
    FifthLayer,
    Manual,
}

impl From<TriggerArg> for TriggerScheme {
    fn from(t: TriggerArg) -> Self {
        match t {
            TriggerArg::TenSec => Self::TenSec,
            TriggerArg::ThirtySec => Self::ThirtySec,
            TriggerArg::SixtySec => Self::SixtySec,
            TriggerArg::EachLayer => Self::EachLayer,
            TriggerArg::FifthLayer => Self::FifthLayer,
            TriggerArg::Manual => Self::Manual,
        }
    }
}

fn parse_resolution(s: &str) -> Result<Resolution, String> {
    let (w, h) = s
        .split_once(['x', 'X'])
        .ok_or("use WIDTHxHEIGHT, e.g. 1280x720")?;
    let part = |v: &str| {
        v.parse::<u32>()
            .map_err(|_| "use WIDTHxHEIGHT, e.g. 1280x720")
    };
    Ok(Resolution {
        width: part(w)?,
        height: part(h)?,
    })
}

/// Camera settings; unset ones stay as they are.
#[derive(Debug, Args)]
pub struct SettingsArgs {
    /// Camera name.
    #[arg(long)]
    pub name: Option<String>,
    /// When to take pictures.
    #[arg(long, value_enum)]
    pub trigger: Option<TriggerArg>,
    /// Resolution as WIDTHxHEIGHT.
    #[arg(long, value_parser = parse_resolution)]
    pub resolution: Option<Resolution>,
    /// Rotation in degrees.
    #[arg(long, allow_negative_numbers = true)]
    pub rotation: Option<i32>,
    /// Focus.
    #[arg(long, allow_negative_numbers = true)]
    pub focus: Option<f64>,
    /// Exposure.
    #[arg(long, allow_negative_numbers = true)]
    pub exposure: Option<f64>,
    /// Send pictures to Prusa Connect: true or false.
    #[arg(long)]
    pub send_to_connect: Option<bool>,
}

impl SettingsArgs {
    pub fn into_settings(self) -> CameraSettings {
        CameraSettings {
            name: self.name,
            trigger_scheme: self.trigger.map(Into::into),
            resolution: self.resolution,
            rotation: self.rotation,
            focus: self.focus,
            exposure: self.exposure,
            send_to_connect: self.send_to_connect,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum CameraCommand {
    /// List the printer's cameras.
    Ls {
        /// Printer name.
        printer: String,
    },
    /// Show one camera's configuration.
    Show {
        /// Printer name.
        printer: String,
        /// Camera id.
        id: String,
    },
    /// Save a camera picture to a file (or `-` for stdout).
    Snap {
        /// Printer name.
        printer: String,
        /// Camera id. Without it, the printer's default camera is used.
        id: Option<String>,
        /// Where to save the picture; `-` writes it to stdout.
        #[arg(short, long)]
        output: String,
        /// Take a new picture now instead of fetching the last one (needs a camera id).
        #[arg(long, requires = "id")]
        take: bool,
    },
    /// Set the order of the cameras.
    Order {
        /// Printer name.
        printer: String,
        /// Camera ids in the new order.
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Register a camera with the printer, optionally with settings.
    Register {
        /// Printer name.
        printer: String,
        /// Camera id.
        id: String,
        #[command(flatten)]
        settings: SettingsArgs,
    },
    /// Change a camera's settings (give at least one).
    Config {
        /// Printer name.
        printer: String,
        /// Camera id.
        id: String,
        #[command(flatten)]
        settings: SettingsArgs,
    },
    /// Remove a camera from the printer.
    Remove {
        /// Printer name.
        printer: String,
        /// Camera id.
        id: String,
    },
    /// Reset a camera's settings to the defaults.
    Reset {
        /// Printer name.
        printer: String,
        /// Camera id.
        id: String,
    },
    /// Connect a camera.
    Connect {
        /// Printer name.
        printer: String,
        /// Camera id.
        id: String,
    },
    /// Disconnect a camera.
    Disconnect {
        /// Printer name.
        printer: String,
        /// Camera id.
        id: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum UpdateCommand {
    /// Check whether an update is available.
    Check {
        /// Printer name.
        printer: String,
        /// What to update. Only `prusalink` is supported.
        #[arg(long, default_value = "prusalink", value_parser = ["prusalink"])]
        env: String,
    },
    /// Apply an available update. Always needs --yes, even in a terminal.
    Apply {
        /// Printer name.
        printer: String,
        /// What to update. Only `prusalink` is supported.
        #[arg(long, default_value = "prusalink", value_parser = ["prusalink"])]
        env: String,
        /// Confirm the update. Required.
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

fn parse_role(s: &str) -> Result<Role, starstar_shared::UnknownRole> {
    s.parse()
}

#[derive(Debug, Subcommand)]
pub enum UserCommand {
    /// Create a user.
    Add {
        /// New username: lowercase letters, digits, '.', '_' or '-'.
        username: String,
        /// Role: viewer, operator or admin.
        #[arg(long, value_parser = parse_role)]
        role: Role,
        /// Read the password from stdin (one line) instead of prompting.
        #[arg(long)]
        password_stdin: bool,
    },
    /// List users.
    List,
    /// Change a user's role.
    SetRole {
        /// Username.
        username: String,
        /// New role: viewer, operator or admin.
        #[arg(value_parser = parse_role)]
        role: Role,
    },
    /// Disable a user so they cannot sign in.
    Disable {
        /// Username.
        username: String,
    },
    /// Enable a disabled user.
    Enable {
        /// Username.
        username: String,
    },
    /// Delete a user. Needs --yes without a terminal.
    Remove {
        /// Username.
        username: String,
        #[command(flatten)]
        confirm: ConfirmArgs,
    },
    /// Change your own password. This signs out all your other sessions and apps.
    Passwd {
        /// Read the current and then the new password from stdin (two lines).
        #[arg(long)]
        password_stdin: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum AppsCommand {
    /// List connected apps.
    List {
        /// Show everyone's apps (admin only).
        #[arg(long)]
        all: bool,
    },
    /// Revoke a connected app's access.
    Revoke {
        /// App id, from `apps list`.
        id: String,
    },
}
