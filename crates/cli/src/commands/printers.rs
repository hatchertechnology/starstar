//! `printer …` commands.

use starstar_shared::{
    PrinterInfo, PrinterStatus, PrinterSummary, StorageDevice, is_valid_printer_name,
};

use super::Ctx;
use crate::args::PrinterCommand;
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
