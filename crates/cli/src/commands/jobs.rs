//! `job …` and `transfer …` commands.

use serde_json::json;
use starstar_shared::{Job, Transfer};

use super::Ctx;
use super::printers::check_name;
use crate::args::{JobCommand, TransferCommand};
use crate::client::ApiClient;
use crate::confirm::{confirm, confirm_here, is_interactive};
use crate::endpoints::{self, JobAction};
use crate::error::{CliError, CliResult};
use crate::output::duration;

fn job_name(job: &Job) -> String {
    job.file
        .as_ref()
        .map(|f| f.display_name.clone().unwrap_or_else(|| f.name.clone()))
        .unwrap_or_default()
}

/// The given id, or the id of the printer's current job.
fn resolve_job(
    client: &ApiClient,
    printer: &str,
    id: Option<u64>,
) -> CliResult<(u64, Option<Job>)> {
    if let Some(id) = id {
        return Ok((id, None));
    }
    match client.get_optional::<Job>(&endpoints::job(printer))? {
        Some(job) => Ok((job.id, Some(job))),
        None => Err(CliError::Other(format!("no job is running on `{printer}`"))),
    }
}

pub fn run_job(ctx: &Ctx, command: JobCommand) -> CliResult<()> {
    let out = ctx.out;
    match command {
        JobCommand::Show { printer } => {
            check_name(&printer)?;
            let job = ctx
                .client()?
                .get_optional::<Job>(&endpoints::job(&printer))?;
            out.emit_value(&job, || match &job {
                None => format!("{printer}: no job"),
                Some(j) => {
                    let left = j
                        .time_remaining
                        .map(|t| format!(", {} left", duration(t)))
                        .unwrap_or_default();
                    format!(
                        "{printer}: job {} {} {:.1}% {}{left}",
                        j.id,
                        j.state,
                        j.progress,
                        job_name(j)
                    )
                }
            });
        }
        JobCommand::Pause { printer, id } => act(ctx, &printer, id, JobAction::Pause)?,
        JobCommand::Resume { printer, id } => act(ctx, &printer, id, JobAction::Resume)?,
        JobCommand::Continue { printer, id } => act(ctx, &printer, id, JobAction::Continue)?,
        JobCommand::Stop {
            printer,
            id,
            confirm: c,
        } => {
            check_name(&printer)?;
            // Refuse before the lookup: without a terminal and --yes nothing is sent.
            if !c.yes && !is_interactive() {
                confirm(
                    &format!("stop the job on `{printer}`"),
                    false,
                    false,
                    &mut std::io::empty(),
                    &mut std::io::sink(),
                )?;
            }
            let client = ctx.client()?;
            let (id, job) = resolve_job(&client, &printer, id)?;
            let what = match &job {
                Some(j) => format!("stop job {id} ({}) on `{printer}`", job_name(j)),
                None => format!("stop job {id} on `{printer}`"),
            };
            confirm_here(&what, c.yes)?;
            client.send_empty(&endpoints::stop_job(&printer, id), None)?;
            out.emit(
                &json!({"printer": printer, "job_id": id, "action": "stop"}),
                || format!("stopped job {id} on {printer}"),
            );
        }
    }
    Ok(())
}

fn act(ctx: &Ctx, printer: &str, id: Option<u64>, action: JobAction) -> CliResult<()> {
    check_name(printer)?;
    let client = ctx.client()?;
    let (id, _) = resolve_job(&client, printer, id)?;
    client.send_empty(&endpoints::job_action(printer, id, action), None)?;
    ctx.out.emit(
        &json!({"printer": printer, "job_id": id, "action": action.as_str()}),
        || format!("{} job {id} on {printer}", action.past_tense()),
    );
    Ok(())
}

pub fn run_transfer(ctx: &Ctx, command: TransferCommand) -> CliResult<()> {
    let out = ctx.out;
    match command {
        TransferCommand::Show { printer } => {
            check_name(&printer)?;
            let t = ctx
                .client()?
                .get_optional::<Transfer>(&endpoints::transfer(&printer))?;
            out.emit_value(&t, || match &t {
                None => format!("{printer}: no transfer"),
                Some(t) => format!(
                    "{printer}: transfer {} {} {} {:.0}%",
                    t.id.map_or_else(|| "-".to_owned(), |i| i.to_string()),
                    t.kind,
                    t.display_name,
                    t.progress * 100.0
                ),
            });
        }
        TransferCommand::Cancel { printer, id } => {
            check_name(&printer)?;
            let client = ctx.client()?;
            let id = match id {
                Some(id) => id,
                None => {
                    let t = client
                        .get_optional::<Transfer>(&endpoints::transfer(&printer))?
                        .ok_or_else(|| {
                            CliError::Other(format!("no transfer in progress on `{printer}`"))
                        })?;
                    t.id.ok_or_else(|| {
                        CliError::Other(
                            "the printer did not report a transfer id; pass --id".into(),
                        )
                    })?
                }
            };
            client.send_empty(&endpoints::cancel_transfer(&printer, id), None)?;
            out.emit(
                &json!({"printer": printer, "transfer_id": id, "action": "cancel"}),
                || format!("cancelled transfer {id} on {printer}"),
            );
        }
    }
    Ok(())
}
