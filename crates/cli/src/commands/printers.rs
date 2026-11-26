//! `printer …` commands.

use serde_json::json;
use starstar_shared::{
    PrinterInfo, PrinterStatus, PrinterSummary, RegisterPrinter, StorageDevice,
    is_valid_printer_name,
};

use super::{Ctx, read_secret};
use crate::args::{AddArgs, PrinterCommand};
use crate::confirm::confirm_here;
use crate::endpoints;
use crate::error::{CliError, CliResult};
use crate::output::{or_dash, temperature};

/// Rejects names the API could never have registered.
pub fn check_name(name: &str) -> CliResult<()> {
    if is_valid_printer_name(name) {
        Ok(())
    } else {
        Err(CliError::Usage(format!(
            "invalid printer name `{name}`: use 1-63 lowercase letters, digits or dashes"
        )))
    }
}

pub fn run(ctx: &Ctx, command: PrinterCommand) -> CliResult<()> {
    let out = ctx.out;
    match command {
        PrinterCommand::Add(args) => add(ctx, args)?,
        PrinterCommand::Remove { name, confirm } => {
            check_name(&name)?;
            // Ask before anything can touch a token or the network.
            confirm_here(
                &format!("remove printer `{name}` from starstar"),
                confirm.yes,
            )?;
            ctx.client()?
                .send_empty(&endpoints::remove_printer(&name), None)?;
            out.emit(&json!({"printer": name, "action": "remove"}), || {
                format!("removed printer `{name}`")
            });
        }
        PrinterCommand::List => {
            let printers: Vec<PrinterSummary> = ctx.client()?.get_json(&endpoints::printers())?;
            out.emit_value(&printers, || {
                if printers.is_empty() {
                    return "no printers registered".to_owned();
                }
                printers
                    .iter()
                    .map(|p| {
                        format!(
                            "{:<20} {:<6} {:<22} {}",
                            p.name,
                            p.kind.as_str(),
                            p.host,
                            p.username
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        PrinterCommand::Show { name } => {
            check_name(&name)?;
            let p: PrinterSummary = ctx.client()?.get_json(&endpoints::printer(&name))?;
            out.emit_value(&p, || {
                format!(
                    "name:     {}\nkind:     {}\nhost:     {}\nusername: {}",
                    p.name,
                    p.kind.as_str(),
                    p.host,
                    p.username
                )
            });
        }
        PrinterCommand::Info { name } => {
            check_name(&name)?;
            let i: PrinterInfo = ctx.client()?.get_json(&endpoints::printer_info(&name))?;
            out.emit_value(&i, || {
                format!(
                    "name:          {}\nkind:          {}\nfirmware:      {}\napi version:   {}\nhostname:      {}\nserial:        {}\nnozzle:        {} mm\nmmu:           {}",
                    i.name,
                    i.kind.as_str(),
                    i.firmware,
                    i.api_version,
                    or_dash(i.hostname.as_ref()),
                    or_dash(i.serial.as_ref()),
                    or_dash(i.nozzle_diameter),
                    or_dash(i.mmu)
                )
            });
        }
        PrinterCommand::Status { name } => {
            check_name(&name)?;
            let s: PrinterStatus = ctx.client()?.get_json(&endpoints::printer_status(&name))?;
            out.emit_value(&s, || {
                format!(
                    "{}: {}\nnozzle: {} / {} °C\nbed:    {} / {} °C",
                    s.name,
                    s.state,
                    temperature(s.nozzle_temp),
                    temperature(s.nozzle_target),
                    temperature(s.bed_temp),
                    temperature(s.bed_target)
                )
            });
        }
        PrinterCommand::Storage { name } => {
            check_name(&name)?;
            let devices: Vec<StorageDevice> =
                ctx.client()?.get_json(&endpoints::printer_storage(&name))?;
            out.emit_value(&devices, || {
                if devices.is_empty() {
                    return "no storage devices".to_owned();
                }
                devices
                    .iter()
                    .map(|d| {
                        format!(
                            "{:<12} {:<8} available={:<5} read_only={:<5} free={} total={}",
                            d.path,
                            d.kind,
                            d.available,
                            or_dash(d.read_only),
                            or_dash(d.free_space),
                            or_dash(d.total_space)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
    }
    Ok(())
}

fn add(ctx: &Ctx, args: AddArgs) -> CliResult<()> {
    check_name(&args.name)?;
    let password = read_secret(
        &format!("Password for {}@{}: ", args.username, args.host),
        args.password_stdin,
    )?;
    let printer: PrinterSummary = ctx.client()?.send_json(
        &endpoints::register_printer(),
        &RegisterPrinter {
            name: args.name,
            kind: args.kind,
            host: args.host,
            username: args.username,
            password,
        },
    )?;
    ctx.out.emit_value(&printer, || {
        format!(
            "registered {} printer `{}` at {}",
            printer.kind.as_str(),
            printer.name,
            printer.host
        )
    });
    Ok(())
}
