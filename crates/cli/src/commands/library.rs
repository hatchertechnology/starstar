//! Print library commands.
use super::Ctx;
use crate::{
    args::LibraryCommand,
    confirm::confirm_here,
    endpoints::Endpoint,
    error::{CliError, CliResult},
    output::{duration, or_dash},
};
use reqwest::Method;
use serde_json::json;
use starstar_shared::LibraryFile;

fn endpoint(method: Method, id: Option<i64>, suffix: Option<&str>) -> Endpoint {
    let mut segments = vec!["api".into(), "library".into()];
    if let Some(id) = id {
        segments.push(id.to_string());
    }
    if let Some(suffix) = suffix {
        segments.push(suffix.into());
    }
    Endpoint {
        method,
        segments,
        query: vec![],
    }
}

pub fn run(ctx: &Ctx, command: LibraryCommand) -> CliResult<()> {
    match command {
        LibraryCommand::Ls { search, sort } => {
            let mut ep = endpoint(Method::GET, None, None);
            if let Some(q) = search {
                ep.query.push(("q", q));
            }
            if let Some(sort) = sort {
                ep.query.push(("sort", sort.replace('-', "_")));
            }
            let files: Vec<LibraryFile> = ctx.client()?.get_json(&ep)?;
            ctx.out.emit_value(&files, || {
                let mut lines = vec![format!(
                    "{:<8} {:<30} {:<7} {:<15} {:<12} {}",
                    "ID", "NAME", "VERSION", "SLICED FOR", "FILAMENT", "TIME"
                )];
                for f in &files {
                    let m = f.meta.as_ref();
                    lines.push(format!(
                        "{:<8} {:<30} {:<7} {:<15} {:<12} {}",
                        f.id,
                        f.name,
                        f.version,
                        or_dash(m.and_then(|m| m.printer_model.as_ref())),
                        or_dash(m.and_then(|m| m.filament_type.as_ref())),
                        m.and_then(|m| m.estimated_print_time)
                            .map_or_else(|| "-".into(), duration)
                    ));
                }
                lines.join("\n")
            });
        }
        LibraryCommand::Upload { local, name } => {
            if !local.is_file() {
                return Err(CliError::Usage(format!(
                    "{} is not a file",
                    local.display()
                )));
            }
            let name = match name {
                Some(n) => n,
                None => local
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| CliError::Usage("file has no usable name".into()))?
                    .to_owned(),
            };
            let mut ep = endpoint(Method::POST, None, None);
            ep.query.push(("name", name.clone()));
            let bytes = ctx.client()?.upload(&ep, &local)?;
            ctx.out.emit(&json!({"name": name, "bytes": bytes}), || {
                format!("uploaded {name} ({bytes} bytes)")
            });
        }
        LibraryCommand::Info { id } => {
            let file: LibraryFile =
                ctx.client()?
                    .get_json(&endpoint(Method::GET, Some(id), None))?;
            ctx.out.emit_value(&file, || {
                format!(
                    "{} v{}\nid: {}\nsize: {}\nsliced for: {}\nfilament: {}\ntime: {}",
                    file.name,
                    file.version,
                    file.id,
                    file.size,
                    or_dash(file.meta.as_ref().and_then(|m| m.printer_model.as_ref())),
                    or_dash(file.meta.as_ref().and_then(|m| m.filament_type.as_ref())),
                    file.meta
                        .as_ref()
                        .and_then(|m| m.estimated_print_time)
                        .map_or_else(|| "-".into(), duration)
                )
            });
        }
        LibraryCommand::Download { id, output, force } => {
            let client = ctx.client()?;
            let file: LibraryFile = client.get_json(&endpoint(Method::GET, Some(id), None))?;
            let path = match output {
                Some(p) => p,
                None => {
                    let p = std::path::Path::new(&file.name);
                    if p.components().count() != 1 || p.file_name().is_none() {
                        return Err(CliError::Usage(
                            "library name is not a safe local file name; use --output".into(),
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
                &endpoint(Method::GET, Some(id), Some("download")),
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
            ctx.out.emit(&json!({"id": id, "path": path}), || {
                format!("downloaded to {}", path.display())
            });
        }
        LibraryCommand::Rm { id, confirm } => {
            confirm_here(&format!("delete library file {id}"), confirm.yes)?;
            ctx.client()?
                .send_empty(&endpoint(Method::DELETE, Some(id), None), None)?;
            ctx.out.emit(&json!({"id": id, "action": "delete"}), || {
                format!("deleted library file {id}")
            });
        }
    }
    Ok(())
}
