mod client;

use std::io::{self, BufRead};

use clap::{Args, Parser, Subcommand};
use client::{ApiClient, CliResult};
use starstar_shared::{
    APP_NAME, DEFAULT_API_URL, PrinterKind, RegisterPrinter, is_valid_printer_name,
};

/// Manage 3D printers from the command line.
#[derive(Debug, Parser)]
#[command(name = APP_NAME, version, about)]
struct Cli {
    /// Base URL of the starstar API.
    #[arg(long, env = "STARSTAR_API_URL", default_value = DEFAULT_API_URL, global = true)]
    api_url: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check that the API is reachable.
    Status,
    /// Register and manage printers.
    #[command(subcommand)]
    Printer(PrinterCommand),
}

#[derive(Debug, Subcommand)]
enum PrinterCommand {
    /// Register a printer. The API checks it can reach the printer first.
    Add(AddArgs),
    /// List registered printers.
    List,
    /// Show a registered printer's connection details.
    Show { name: String },
    /// Remove a printer from the API. The printer itself is not changed.
    Remove { name: String },
    /// Show firmware and hardware details from the printer.
    Info { name: String },
    /// Show the printer's state and temperatures.
    Status { name: String },
    /// List storage devices on the printer.
    Storage { name: String },
}

#[derive(Debug, Args)]
struct AddArgs {
    /// Unique name for the printer, e.g. `office-mk4`.
    name: String,
    /// Printer model: `mk4` or `mini`.
    #[arg(long, value_parser = |s: &str| s.parse::<PrinterKind>())]
    kind: PrinterKind,
    /// Hostname or IP address, optionally with a port.
    #[arg(long)]
    host: String,
    /// PrusaLink username.
    #[arg(long)]
    username: String,
    /// Read the password from stdin instead of prompting for it.
    #[arg(long)]
    password_stdin: bool,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> CliResult<()> {
    let cli = Cli::parse();
    let api = ApiClient::new(&cli.api_url)?;

    match cli.command {
        Command::Status => {
            let health = api.health()?;
            println!(
                "{} API {} is {} at {}",
                health.app, health.version, health.status, cli.api_url
            );
        }
        Command::Printer(command) => printer(&api, command)?,
    }
    Ok(())
}

fn printer(api: &ApiClient, command: PrinterCommand) -> CliResult<()> {
    match command {
        PrinterCommand::Add(args) => add(api, args)?,

        PrinterCommand::List => {
            let printers = api.list_printers()?;
            if printers.is_empty() {
                println!("no printers registered");
            }
            for p in printers {
                println!(
                    "{:<20} {:<6} {:<22} {}",
                    p.name,
                    p.kind.as_str(),
                    p.host,
                    p.username
                );
            }
        }

        PrinterCommand::Show { name } => {
            check_name(&name)?;
            let p = api.show_printer(&name)?;
            println!("name:     {}", p.name);
            println!("kind:     {}", p.kind.as_str());
            println!("host:     {}", p.host);
            println!("username: {}", p.username);
        }

        PrinterCommand::Remove { name } => {
            check_name(&name)?;
            api.remove_printer(&name)?;
            println!("removed printer `{name}`");
        }

        PrinterCommand::Info { name } => {
            check_name(&name)?;
            let info = api.printer_info(&name)?;
            println!("name:          {}", info.name);
            println!("kind:          {}", info.kind.as_str());
            println!("firmware:      {}", info.firmware);
            println!("api version:   {}", info.api_version);
            println!("hostname:      {}", or_dash(info.hostname));
            println!("serial:        {}", or_dash(info.serial));
            println!("nozzle:        {} mm", or_dash(info.nozzle_diameter));
            println!("mmu:           {}", or_dash(info.mmu));
        }

        PrinterCommand::Status { name } => {
            check_name(&name)?;
            let status = api.printer_status(&name)?;
            println!("{}: {}", status.name, status.state);
            println!(
                "nozzle: {} / {} °C",
                temperature(status.nozzle_temp),
                temperature(status.nozzle_target)
            );
            println!(
                "bed:    {} / {} °C",
                temperature(status.bed_temp),
                temperature(status.bed_target)
            );
        }

        PrinterCommand::Storage { name } => {
            check_name(&name)?;
            let devices = api.printer_storage(&name)?;
            if devices.is_empty() {
                println!("no storage devices");
            }
            for d in devices {
                println!(
                    "{:<12} {:<8} available={:<5} read_only={:<5} free={} total={}",
                    d.path,
                    d.kind,
                    d.available,
                    or_dash(d.read_only),
                    or_dash(d.free_space),
                    or_dash(d.total_space)
                );
            }
        }
    }
    Ok(())
}

fn add(api: &ApiClient, args: AddArgs) -> CliResult<()> {
    check_name(&args.name)?;
    let password = read_password(&args)?;

    let printer = api.register(&RegisterPrinter {
        name: args.name,
        kind: args.kind,
        host: args.host,
        username: args.username,
        password,
    })?;
    println!(
        "registered {} printer `{}` at {}",
        printer.kind.as_str(),
        printer.name,
        printer.host
    );
    Ok(())
}

/// Reads the password from stdin when asked, so it never appears in shell
/// history or the process list. Otherwise prompts without echoing it.
fn read_password(args: &AddArgs) -> CliResult<String> {
    if args.password_stdin {
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
        return Ok(line.trim_end_matches(['\r', '\n']).to_owned());
    }

    let prompt = format!("Password for {}@{}: ", args.username, args.host);
    Ok(rpassword::prompt_password(prompt)?)
}

fn check_name(name: &str) -> CliResult<()> {
    if is_valid_printer_name(name) {
        Ok(())
    } else {
        Err(
            format!("invalid printer name `{name}`: use 1-63 lowercase letters, digits or dashes")
                .into(),
        )
    }
}

fn or_dash<T: ToString>(value: Option<T>) -> String {
    value.map_or_else(|| "-".to_owned(), |v| v.to_string())
}

fn temperature(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_owned(), |t| format!("{t:.1}"))
}
