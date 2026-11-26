//! `files …` commands.

use serde::Serialize;
use serde_json::json;
use starstar_shared::{FileEntry, FileKind, FileState};

use super::Ctx;
use super::printers::check_name;
use crate::args::FilesCommand;
use crate::confirm::confirm_here;
use crate::endpoints::{self, RemotePath};
use crate::error::CliResult;
use crate::output::{duration, or_dash};

pub fn kind_label(kind: FileKind) -> &'static str {
    match kind {
        FileKind::PrintFile => "print_file",
        FileKind::Firmware => "firmware",
        FileKind::File => "file",
        FileKind::Folder => "folder",
        FileKind::Other => "other",
    }
}

fn line(e: &FileEntry) -> String {
    format!(
        "{:<10} {:>10} {}",
        kind_label(e.kind),
        or_dash(e.size),
        e.display_name.as_deref().unwrap_or(&e.name)
    )
}

/// What `files info` prints: the entry, plus the HEAD flags for a file.
#[derive(Serialize)]
struct FileInfo {
    #[serde(flatten)]
    entry: FileEntry,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<FileState>,
}

fn flag(headers: &reqwest::header::HeaderMap, name: &str) -> bool {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("true"))
}

pub fn run(ctx: &Ctx, command: FilesCommand) -> CliResult<()> {
    let out = ctx.out;
    match command {
        FilesCommand::Ls { printer, remote } => {
            check_name(&printer)?;
            let (path, _) = RemotePath::parse(&remote, true)?;
            let entry: FileEntry = ctx
                .client()?
                .get_json(&endpoints::file_get(&printer, &path))?;
            out.emit_value(&entry, || match (&entry.kind, &entry.children) {
                (FileKind::Folder, Some(children)) if children.is_empty() => "empty".to_owned(),
                (FileKind::Folder, Some(children)) => {
                    children.iter().map(line).collect::<Vec<_>>().join("\n")
                }
                _ => line(&entry),
            });
        }
        FilesCommand::Info { printer, remote } => {
            check_name(&printer)?;
            let (path, _) = RemotePath::parse(&remote, false)?;
            path.require_file()?;
            let client = ctx.client()?;
            let entry: FileEntry = client.get_json(&endpoints::file_get(&printer, &path))?;
            let state = if entry.kind == FileKind::Folder {
                None
            } else {
                let h = client.head(&endpoints::file_head(&printer, &path))?;
                Some(FileState {
                    read_only: flag(&h, "read-only"),
                    currently_printed: flag(&h, "currently-printed"),
                })
            };
            let info = FileInfo { entry, state };
            out.emit_value(&info, || info_text(&info));
        }
        FilesCommand::Print { printer, remote } => {
            check_name(&printer)?;
            let (path, _) = RemotePath::parse(&remote, false)?;
            path.require_file()?;
            ctx.client()?
                .send_empty(&endpoints::file_print(&printer, &path), None)?;
            out.emit(
                &json!({"printer": printer, "storage": path.storage, "path": path.path_display(), "action": "print"}),
                || format!("started printing {} on {printer}", path.display()),
            );
        }
        FilesCommand::Rm {
            printer,
            remote,
            force,
            confirm,
        } => {
            check_name(&printer)?;
            let (path, _) = RemotePath::parse(&remote, false)?;
            path.require_file()?;
            let everything = if force { " and everything in it" } else { "" };
            confirm_here(
                &format!("delete {} on `{printer}`{everything}", path.display()),
                confirm.yes,
            )?;
            ctx.client()?
                .send_empty(&endpoints::file_delete(&printer, &path, force), None)?;
            out.emit(
                &json!({"printer": printer, "storage": path.storage, "path": path.path_display(), "action": "delete"}),
                || format!("deleted {} on {printer}", path.display()),
            );
        }
        FilesCommand::Upload { .. } => unreachable!("handled in the upload task"),
    }
    Ok(())
}

fn info_text(info: &FileInfo) -> String {
    let e = &info.entry;
    let mut lines = vec![
        format!(
            "name:      {}",
            e.display_name.as_deref().unwrap_or(&e.name)
        ),
        format!("kind:      {}", kind_label(e.kind)),
        format!("size:      {}", or_dash(e.size)),
        format!("modified:  {}", or_dash(e.modified)),
    ];
    if let Some(m) = &e.meta {
        lines.push(format!(
            "print time: {}",
            m.estimated_print_time
                .map_or_else(|| "-".to_owned(), duration)
        ));
        lines.push(format!("filament:  {}", or_dash(m.filament_type.as_ref())));
        lines.push(format!("layer:     {} mm", or_dash(m.layer_height)));
        lines.push(format!("nozzle:    {} °C", or_dash(m.nozzle_temp)));
        lines.push(format!("bed:       {} °C", or_dash(m.bed_temp)));
        lines.push(format!("used:      {} g", or_dash(m.filament_used_g)));
    }
    if let Some(s) = &info.state {
        lines.push(format!(
            "read-only: {}",
            if s.read_only { "yes" } else { "no" }
        ));
        lines.push(format!(
            "printing now: {}",
            if s.currently_printed { "yes" } else { "no" }
        ));
    }
    lines.join("\n")
}
