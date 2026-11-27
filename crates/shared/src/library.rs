use serde::{Deserialize, Serialize};

/// Supported print file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryKind {
    Bgcode,
    Gcode,
}

impl LibraryKind {
    pub fn from_name(name: &str) -> Option<Self> {
        let extension = std::path::Path::new(name).extension()?.to_str()?;
        if extension.eq_ignore_ascii_case("bgcode") {
            Some(Self::Bgcode)
        } else if extension.eq_ignore_ascii_case("gcode") {
            Some(Self::Gcode)
        } else {
            None
        }
    }
}
