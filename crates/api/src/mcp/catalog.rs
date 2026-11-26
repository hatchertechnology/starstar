//! The minimum role of every tool, taken from the `printers::` constants.
//! Used only to hide tools in `tools/list`; the `printers::` functions are
//! what enforce roles.

use starstar_shared::Role;

use crate::printers::registry::LIST_PRINTERS_ROLE;
use crate::printers::telemetry::PRINTER_STATUS_ROLE;

pub(crate) fn tool_role(name: &str) -> Option<Role> {
    Some(match name {
        "list_printers" => LIST_PRINTERS_ROLE,
        "printer_status" => PRINTER_STATUS_ROLE,
        _ => return None,
    })
}
