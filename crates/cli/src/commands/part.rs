//! Parts library commands.
use super::Ctx;
use crate::{
    args::PartCommand,
    confirm::confirm_here,
    endpoints::Endpoint,
    error::{CliError, CliResult},
    output::duration,
};
use reqwest::Method;
use serde_json::json;
use starstar_shared::{FileRole, ModelRevision, Part, PartFile, PartSummary, Setup};

fn ep(method: Method, segments: &[&str]) -> Endpoint {
    Endpoint {
        method,
        segments: std::iter::once("api".to_owned())
            .chain(segments.iter().map(|s| (*s).to_owned()))
            .collect(),
        query: vec![],
    }
}
fn resolve(ctx: &Ctx, part: &str) -> CliResult<i64> {
    if let Ok(id) = part.parse::<i64>() {
        return Ok(id);
    }
    let mut e = ep(Method::GET, &["parts"]);
    e.query.push(("q", part.to_owned()));
    let matches: Vec<PartSummary> = ctx.client()?.get_json(&e)?;
    let matches: Vec<_> = matches
        .iter()
        .filter(|p| p.name.to_lowercase() == part.to_lowercase())
        .collect();
    match matches.as_slice() {
        [p] => Ok(p.id),
        [] => Err(CliError::Usage(format!("part {part:?} not found"))),
        _ => Err(CliError::Usage(format!("part name {part:?} is ambiguous"))),
    }
}
fn tree(ctx: &Ctx, id: i64) -> CliResult<Part> {
    ctx.client()?
        .get_json(&ep(Method::GET, &["parts", &id.to_string()]))
}
fn role_name(role: FileRole) -> &'static str {
    match role {
        FileRole::Source => "source",
        FileRole::Render => "render",
        FileRole::Project3mf => "project_3mf",
        FileRole::Slice => "slice",
        FileRole::Other => "other",
    }
}
fn render_tree(p: &Part) -> String {
    let mut lines = vec![format!("{} (ID {})", p.name, p.id)];
    if let Some(notes) = &p.notes {
        lines.push(format!("notes: {notes}"));
    }
    for rev in &p.model_revisions {
        lines.push(format!("model revision {} (ID {})", rev.number, rev.id));
        for f in &rev.files {
            lines.push(format!("  {} {} ({})", f.id, f.name, role_name(f.role)));
        }
    }
    for setup in &p.setups {
        lines.push(format!(
            "setup {} (ID {}){}{}",
            setup.name,
            setup.id,
            if p.default_setup_id == Some(setup.id) {
                " [default]"
            } else {
                ""
            },
            setup
                .behind_model
                .as_ref()
                .map(|x| format!(" [behind: {x}]"))
                .unwrap_or_default()
        ));
        for rev in &setup.revisions {
            lines.push(format!(
                "  revision {} (ID {}, model rev {})",
                rev.number, rev.id, rev.model_revision_number
            ));
            if let Some(f) = &rev.project {
                lines.push(format!("    project {} {}", f.id, f.name));
            }
            lines.push("    ID FAMILY NOZZLE PROFILE FILAMENT TIME".into());
            for f in &rev.slices {
                let t = f.target.as_ref();
                let family = t
                    .and_then(|x| x.family)
                    .map(|x| format!("{x:?}"))
                    .unwrap_or_else(|| "-".into());
                let nozzle = t
                    .and_then(|x| x.nozzle)
                    .map(|x| x.to_string())
                    .unwrap_or_else(|| "-".into());
                let profile = t.and_then(|x| x.profile.as_deref()).unwrap_or("-");
                let filament = f
                    .meta
                    .as_ref()
                    .and_then(|x| x.filament_type.as_deref())
                    .unwrap_or("-");
                let time = f
                    .meta
                    .as_ref()
                    .and_then(|x| x.estimated_print_time)
                    .map_or_else(|| "-".into(), duration);
                lines.push(format!(
                    "    {} {family} {nozzle} {profile} {filament} {time}{}",
                    f.id,
                    f.superseded_by
                        .map(|id| format!(" [superseded by {id}]"))
                        .unwrap_or_default()
                ));
            }
        }
    }
    for f in &p.other_files {
        lines.push(format!("file {} {} ({})", f.id, f.name, role_name(f.role)));
    }
    lines.join("\n")
}
pub fn run(ctx: &Ctx, command: PartCommand) -> CliResult<()> {
    match command {
        PartCommand::Ls { search, sort } => {
            let mut e = ep(Method::GET, &["parts"]);
            if let Some(q) = search {
                e.query.push(("q", q));
            }
            if let Some(s) = sort {
                e.query.push(("sort", s));
            }
            let parts: Vec<PartSummary> = ctx.client()?.get_json(&e)?;
            ctx.out.emit_value(&parts, || {
                let mut lines = vec![format!(
                    "{:<8} {:<30} {:<10} {:<8} {}",
                    "ID", "NAME", "MODEL REV", "SETUPS", "SLICES"
                )];
                for p in &parts {
                    lines.push(format!(
                        "{:<8} {:<30} {:<10} {:<8} {}",
                        p.id, p.name, p.latest_model_revision, p.setups, p.slices
                    ));
                }
                lines.join("\n")
            });
        }
        PartCommand::New { name, notes } => {
            let p: Part = ctx.client()?.send_json(
                &ep(Method::POST, &["parts"]),
                &json!({"name":name,"notes":notes}),
            )?;
            ctx.out.emit_value(&p, || render_tree(&p));
        }
        PartCommand::Info { part } => {
            let p = tree(ctx, resolve(ctx, &part)?)?;
            ctx.out.emit_value(&p, || render_tree(&p));
        }
        PartCommand::Rm { part, confirm } => {
            confirm_here(&format!("delete part {part}"), confirm.yes)?;
            let id = resolve(ctx, &part)?;
            ctx.client()?
                .send_empty(&ep(Method::DELETE, &["parts", &id.to_string()]), None)?;
            ctx.out.emit_value(&json!({"id":id,"action":"delete"}), || {
                format!("deleted part {id}")
            });
        }
        PartCommand::Rev { part, note, carry } => {
            let id = resolve(ctx, &part)?;
            let r: ModelRevision = ctx.client()?.send_json(
                &ep(Method::POST, &["parts", &id.to_string(), "revisions"]),
                &json!({"note":note,"carry":carry}),
            )?;
            ctx.out.emit_value(&r, || {
                format!("created model revision {} (ID {})", r.number, r.id)
            });
        }
        PartCommand::Setup { part, name } => {
            let id = resolve(ctx, &part)?;
            let s: Setup = ctx.client()?.send_json(
                &ep(Method::POST, &["parts", &id.to_string(), "setups"]),
                &json!({"name":name}),
            )?;
            ctx.out
                .emit_value(&s, || format!("created setup {} (ID {})", s.name, s.id));
        }
        PartCommand::Upload {
            part,
            local,
            role,
            setup,
            setup_rev,
            model_rev,
            profile,
            replace,
            name,
        } => {
            if !local.is_file() {
                return Err(CliError::Usage(format!(
                    "{} is not a file",
                    local.display()
                )));
            }
            let name = name
                .or_else(|| {
                    local
                        .file_name()
                        .and_then(|s| s.to_str())
                        .map(str::to_owned)
                })
                .ok_or_else(|| CliError::Usage("file has no usable name".into()))?;
            let role = match role.as_deref() {
                Some("source") => Some(FileRole::Source),
                Some("render") => Some(FileRole::Render),
                Some("project_3mf") => Some(FileRole::Project3mf),
                Some("slice") => Some(FileRole::Slice),
                Some("other") => Some(FileRole::Other),
                Some(_) => None,
                None => FileRole::infer(&name),
            }
            .ok_or_else(|| CliError::Usage("file role required; use --role".into()))?;
            if matches!(role, FileRole::Project3mf | FileRole::Slice) && setup.is_none() {
                return Err(CliError::Usage(".3mf and slices need --setup".into()));
            }
            if setup_rev.is_some() && setup.is_none() {
                return Err(CliError::Usage("--setup-rev needs --setup".into()));
            }
            let id = resolve(ctx, &part)?;
            let mut e = ep(Method::POST, &["parts", &id.to_string(), "files"]);
            e.query.push(("name", name.clone()));
            e.query.push(("role", role_name(role).into()));
            if setup.is_some() || model_rev.is_some() {
                let p = tree(ctx, id)?;
                if let Some(n) = model_rev {
                    let r = p
                        .model_revisions
                        .iter()
                        .find(|r| r.number == n)
                        .ok_or_else(|| CliError::Usage(format!("model revision {n} not found")))?;
                    e.query.push(("model_revision", r.id.to_string()));
                }
                if let Some(name) = setup {
                    let s = p
                        .setups
                        .iter()
                        .find(|s| s.name == name)
                        .ok_or_else(|| CliError::Usage(format!("setup {name:?} not found")))?;
                    if role == FileRole::Project3mf && setup_rev.is_none() {
                        e.query.push(("setup", s.id.to_string()));
                    } else {
                        let r = match setup_rev {
                            Some(n) => s.revisions.iter().find(|r| r.number == n),
                            None => s.revisions.iter().max_by_key(|r| r.number),
                        }
                        .ok_or_else(|| CliError::Usage("setup revision not found".into()))?;
                        e.query.push(("setup_revision", r.id.to_string()));
                    }
                }
            }
            if let Some(p) = profile {
                e.query.push(("profile", p));
            }
            if replace {
                e.query.push(("replace", "true".into()));
            }
            let bytes = ctx.client()?.upload(&e, &local)?;
            ctx.out.emit_value(&json!({"name":name,"bytes":bytes}), || {
                format!("uploaded {name} ({bytes} bytes)")
            });
        }
        PartCommand::Download { id, output, force } => {
            let client = ctx.client()?;
            let file: PartFile = client.get_json(&ep(Method::GET, &["files", &id.to_string()]))?;
            let path = match output {
                Some(p) => p,
                None => {
                    let p = std::path::Path::new(&file.name);
                    if p.components().count() != 1 || p.file_name().is_none() {
                        return Err(CliError::Usage(
                            "file name is not a safe local file name; use --output".into(),
                        ));
                    }
                    p.to_path_buf()
                }
            };
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| std::path::Path::new("."));
            let mut temp = tempfile::NamedTempFile::new_in(parent)?;
            let bytes = client.download_to(
                &ep(Method::GET, &["files", &id.to_string(), "download"]),
                &mut temp,
            )?;
            if bytes != file.size {
                return Err(CliError::Other(format!(
                    "download size mismatch: expected {} bytes, received {bytes}",
                    file.size
                )));
            }
            if force {
                temp.persist(&path).map_err(|e| e.error)?;
            } else {
                temp.persist_noclobber(&path).map_err(|e| e.error)?;
            }
            ctx.out.emit_value(&json!({"id":id,"path":path}), || {
                format!("downloaded to {}", path.display())
            });
        }
    }
    Ok(())
}
