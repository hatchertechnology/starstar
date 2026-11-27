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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryFile {
    pub id: i64,
    pub name: String,
    pub version: i32,
    pub sha256: String,
    pub size: u64,
    pub kind: LibraryKind,
    pub meta: Option<crate::PrintMeta>,
    pub parsed: bool,
    pub has_preview: bool,
    pub uploaded_by: String,
    pub uploaded_at: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibrarySort {
    #[default]
    Newest,
    Name,
    SlicedFor,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryQuery {
    pub q: Option<String>,
    pub sort: Option<LibrarySort>,
}
