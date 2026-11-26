//! The minimum role of every tool, taken from the `printers::` constants.
//! Used only to hide tools in `tools/list`; the `printers::` functions are
//! what enforce roles.

use starstar_shared::Role;

use crate::printers::{cameras, files, jobs, registry, telemetry, transfer, update};

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
        "register_printer" => registry::REGISTER_PRINTER_ROLE,
        "remove_printer" => registry::REMOVE_PRINTER_ROLE,
        "list_cameras" => cameras::LIST_CAMERAS_ROLE,
        "get_camera" => cameras::CAMERA_DETAIL_ROLE,
        "camera_snapshot" => cameras::SNAPSHOT_ROLE,
        "take_snapshot" => cameras::TAKE_SNAPSHOT_ROLE,
        "order_cameras" => cameras::ORDER_CAMERAS_ROLE,
        "register_camera" => cameras::REGISTER_CAMERA_ROLE,
        "remove_camera" => cameras::REMOVE_CAMERA_ROLE,
        "configure_camera" => cameras::UPDATE_CAMERA_CONFIG_ROLE,
        "reset_camera_config" => cameras::RESET_CAMERA_CONFIG_ROLE,
        "connect_camera" => cameras::CONNECT_CAMERA_ROLE,
        "disconnect_camera" => cameras::DISCONNECT_CAMERA_ROLE,
        "check_update" => update::CHECK_UPDATE_ROLE,
        "apply_update" => update::APPLY_UPDATE_ROLE,
        _ => return None,
    })
}
