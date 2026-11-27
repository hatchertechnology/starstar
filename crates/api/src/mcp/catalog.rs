//! The minimum role of every tool, taken from the `printers::` constants.
//! Used only to hide tools in `tools/list`; the `printers::` functions are
//! what enforce roles.

use starstar_shared::Role;

use crate::printers::{cameras, files, jobs, registry, telemetry, transfer, update};

pub(crate) fn tool_role(name: &str) -> Option<Role> {
    Some(match name {
        "part_list" | "part_info" => crate::library::READ_PARTS_ROLE,
        "part_match" => crate::library::EDIT_PARTS_ROLE,
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

#[cfg(test)]
pub(crate) const TOOL_NAMES: &[&str] = &[
    "part_list",
    "part_info",
    "part_match",
    "list_printers",
    "get_printer",
    "register_printer",
    "remove_printer",
    "printer_info",
    "printer_status",
    "list_storage",
    "get_job",
    "pause_job",
    "resume_job",
    "continue_job",
    "stop_job",
    "get_transfer",
    "cancel_transfer",
    "list_files",
    "file_state",
    "upload_file",
    "start_print",
    "delete_file",
    "list_cameras",
    "get_camera",
    "camera_snapshot",
    "take_snapshot",
    "order_cameras",
    "register_camera",
    "remove_camera",
    "configure_camera",
    "reset_camera_config",
    "connect_camera",
    "disconnect_camera",
    "check_update",
    "apply_update",
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::mcp::tools::all_tools;

    #[test]
    fn every_routed_tool_has_a_role_and_vice_versa() {
        let routed: BTreeSet<String> = all_tools()
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        let named: BTreeSet<String> = TOOL_NAMES.iter().map(|n| (*n).to_owned()).collect();
        assert_eq!(routed, named);
        assert_eq!(TOOL_NAMES.len(), 35);
        for name in TOOL_NAMES {
            assert!(tool_role(name).is_some(), "{name} has no role");
        }
        assert_eq!(tool_role("nope"), None);
    }

    /// (name, read_only, destructive, idempotent) copied from the plan's tool table.
    const ANNOTATIONS: &[(&str, bool, bool, Option<bool>)] = &[
        ("part_list", true, false, None),
        ("part_info", true, false, None),
        ("part_match", true, false, None),
        ("list_printers", true, false, None),
        ("get_printer", true, false, None),
        ("register_printer", false, false, None),
        ("remove_printer", false, true, None),
        ("printer_info", true, false, None),
        ("printer_status", true, false, None),
        ("list_storage", true, false, None),
        ("get_job", true, false, None),
        ("pause_job", false, false, Some(true)),
        ("resume_job", false, false, Some(true)),
        ("continue_job", false, false, None),
        ("stop_job", false, true, None),
        ("get_transfer", true, false, None),
        ("cancel_transfer", false, true, None),
        ("list_files", true, false, None),
        ("file_state", true, false, None),
        ("upload_file", false, true, None),
        ("start_print", false, false, None),
        ("delete_file", false, true, None),
        ("list_cameras", true, false, None),
        ("get_camera", true, false, None),
        ("camera_snapshot", true, false, None),
        ("take_snapshot", false, false, None),
        ("order_cameras", false, false, Some(true)),
        ("register_camera", false, false, None),
        ("remove_camera", false, true, None),
        ("configure_camera", false, false, Some(true)),
        ("reset_camera_config", false, true, None),
        ("connect_camera", false, false, None),
        ("disconnect_camera", false, false, None),
        ("check_update", true, false, None),
        ("apply_update", false, true, None),
    ];

    #[test]
    fn annotations_match_the_tool_table() {
        let tools = all_tools().list_all();
        assert_eq!(ANNOTATIONS.len(), tools.len());
        for (name, read_only, destructive, idempotent) in ANNOTATIONS {
            let tool = tools
                .iter()
                .find(|t| t.name == *name)
                .unwrap_or_else(|| panic!("{name} is not routed"));
            let a = tool
                .annotations
                .as_ref()
                .unwrap_or_else(|| panic!("{name} has no annotations"));
            assert_eq!(a.read_only_hint, Some(*read_only), "{name} read_only");
            if !read_only {
                assert_eq!(a.destructive_hint, Some(*destructive), "{name} destructive");
            }
            assert_eq!(a.idempotent_hint, *idempotent, "{name} idempotent");
        }
    }

    #[test]
    fn roles_match_the_printer_consts() {
        use crate::printers::{cameras, files, jobs};
        assert_eq!(tool_role("pause_job"), Some(jobs::PAUSE_JOB_ROLE));
        assert_eq!(tool_role("delete_file"), Some(files::DELETE_FILE_ROLE));
        assert_eq!(
            tool_role("take_snapshot"),
            Some(cameras::TAKE_SNAPSHOT_ROLE)
        );
    }
}
