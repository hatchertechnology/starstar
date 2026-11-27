pub mod blobs;
pub mod parse;
pub mod parts;
pub mod store;

use starstar_shared::Role;

pub const READ_PARTS_ROLE: Role = Role::Viewer;
pub const EDIT_PARTS_ROLE: Role = Role::Operator;
pub const DELETE_PARTS_ROLE: Role = Role::Admin;
