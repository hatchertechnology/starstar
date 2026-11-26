//! The minimum role of every tool, taken from the `printers::` constants.
//! Used only to hide tools in `tools/list`; the `printers::` functions are
//! what enforce roles.

use starstar_shared::Role;

use crate::printers::{files, jobs, registry, telemetry, transfer};

pub(crate) fn tool_role(name: &str) -> Option<Role> {
    Some(match name {
        "list_printers" => registry::LIST_PRINTERS_ROLE,
        "get_printer" => registry::SHOW_PRINTER_ROLE,
        "printer_info" => telemetry::PRINTER_INFO_ROLE,
        "printer_status" => telemetry::PRINTER_STATUS_ROLE,
        "list_storage" => telemetry::PRINTER_STORAGE_ROLE,
        "get_job" => jobs::CURRENT_JOB_ROLE,
        "pause_job" => jobs::PAUSE_JOB_ROLE,
        "resume_job" => jobs::RESUME_JOB_ROLE,
        "continue_job" => jobs::CONTINUE_JOB_ROLE,
        "stop_job" => jobs::STOP_JOB_ROLE,
        "get_transfer" => transfer::CURRENT_TRANSFER_ROLE,
        "cancel_transfer" => transfer::CANCEL_TRANSFER_ROLE,
        "list_files" => files::FILE_INFO_ROLE,
        "file_state" => files::FILE_STATE_ROLE,
        "upload_file" => files::UPLOAD_FILE_ROLE,
        "start_print" => files::PRINT_FILE_ROLE,
        "delete_file" => files::DELETE_FILE_ROLE,
        _ => return None,
    })
}
