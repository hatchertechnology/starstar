use crate::{PrintMeta, PrinterKind};
use serde::{Deserialize, Serialize};
use std::path::Path;
use unicode_casefold::UnicodeCaseFold;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileRole {
    Source,
    Render,
    #[serde(rename = "project_3mf")]
    Project3mf,
    Slice,
    Other,
}

impl FileRole {
    pub fn accepts(self, file_name: &str) -> bool {
        self == Self::Other || Self::infer(file_name) == Some(self)
    }

    pub fn infer(file_name: &str) -> Option<Self> {
        let extension = Path::new(file_name)
            .extension()?
            .to_str()?
            .to_ascii_lowercase();
        match extension.as_str() {
            "step" | "stp" | "stl" | "f3d" | "scad" | "py" => Some(Self::Source),
            "png" | "jpg" | "jpeg" | "webp" => Some(Self::Render),
            "3mf" => Some(Self::Project3mf),
            "bgcode" | "gcode" => Some(Self::Slice),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrinterFamily {
    Mk4,
    Mini,
}

impl PrinterFamily {
    pub fn from_printer_model(model: &str) -> Option<Self> {
        let model = model.to_ascii_uppercase();
        if model.starts_with("MK4") {
            Some(Self::Mk4)
        } else if model.starts_with("MINI") {
            Some(Self::Mini)
        } else {
            None
        }
    }

    pub fn matches_kind(self, kind: PrinterKind) -> bool {
        matches!(
            (self, kind),
            (Self::Mk4, PrinterKind::Mk4) | (Self::Mini, PrinterKind::Mini)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SliceTarget {
    pub family: Option<PrinterFamily>,
    pub nozzle: Option<f64>,
    pub profile: Option<String>,
}

impl SliceTarget {
    pub fn nozzle_matches(a: f64, b: f64) -> bool {
        (a - b).abs() <= 0.001 + f64::EPSILON
    }

    pub fn profile_eq(a: &str, b: &str) -> bool {
        a.trim().case_fold().eq(b.trim().case_fold())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartFile {
    pub id: i64,
    pub part_id: i64,
    pub model_revision_id: Option<i64>,
    pub setup_revision_id: Option<i64>,
    pub role: FileRole,
    pub name: String,
    pub sha256: String,
    pub size: u64,
    pub meta: Option<PrintMeta>,
    pub parsed: bool,
    pub has_preview: bool,
    pub target: Option<SliceTarget>,
    pub superseded_by: Option<i64>,
    pub note: Option<String>,
    pub uploaded_by: String,
    pub uploaded_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelRevision {
    pub id: i64,
    pub number: i32,
    pub note: Option<String>,
    pub created_by: String,
    pub created_at: i64,
    pub files: Vec<PartFile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetupRevision {
    pub id: i64,
    pub number: i32,
    pub model_revision_id: i64,
    pub model_revision_number: i32,
    pub note: Option<String>,
    pub created_by: String,
    pub created_at: i64,
    pub project: Option<PartFile>,
    pub slices: Vec<PartFile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Setup {
    pub id: i64,
    pub name: String,
    pub default_profile: Option<String>,
    pub revisions: Vec<SetupRevision>,
    pub behind_model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub id: i64,
    pub name: String,
    pub notes: Option<String>,
    pub default_setup_id: Option<i64>,
    pub created_by: String,
    pub created_at: i64,
    pub preview_file_id: Option<i64>,
    pub model_revisions: Vec<ModelRevision>,
    pub setups: Vec<Setup>,
    pub other_files: Vec<PartFile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartSummary {
    pub id: i64,
    pub name: String,
    pub latest_model_revision: i32,
    pub setups: u32,
    pub slices: u32,
    pub preview_file_id: Option<i64>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartQuery {
    pub q: Option<String>,
    pub sort: Option<PartSort>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartSort {
    #[default]
    Newest,
    Name,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrinterMatch {
    pub printer: String,
    pub slice: Option<PartFile>,
    pub blocked: Option<String>,
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_roles_follow_extension_table() {
        assert_eq!(FileRole::infer("a.STEP"), Some(FileRole::Source));
        assert_eq!(FileRole::infer("x.3mf"), Some(FileRole::Project3mf));
        assert_eq!(FileRole::infer("notes.pdf"), None);
        assert!(FileRole::Render.accepts("a.webp"));
        assert!(!FileRole::Slice.accepts("a.stl"));
        for ext in ["step", "stp", "stl", "f3d", "scad", "py"] {
            assert_eq!(FileRole::infer(&format!("a.{ext}")), Some(FileRole::Source));
        }
        for ext in ["png", "jpg", "jpeg", "webp"] {
            assert_eq!(FileRole::infer(&format!("a.{ext}")), Some(FileRole::Render));
        }
        for ext in ["bgcode", "gcode"] {
            assert_eq!(FileRole::infer(&format!("a.{ext}")), Some(FileRole::Slice));
        }
        assert!(FileRole::Other.accepts("anything"));
    }

    #[test]
    fn printer_families_follow_model_prefix() {
        assert_eq!(
            PrinterFamily::from_printer_model("MK4S"),
            Some(PrinterFamily::Mk4)
        );
        assert_eq!(
            PrinterFamily::from_printer_model("MK4IS"),
            Some(PrinterFamily::Mk4)
        );
        assert_eq!(
            PrinterFamily::from_printer_model("MINI"),
            Some(PrinterFamily::Mini)
        );
        assert_eq!(PrinterFamily::from_printer_model("XL"), None);
        assert_eq!(
            PrinterFamily::from_printer_model("mini+"),
            Some(PrinterFamily::Mini)
        );
        assert!(PrinterFamily::Mk4.matches_kind(crate::PrinterKind::Mk4));
        assert!(!PrinterFamily::Mk4.matches_kind(crate::PrinterKind::Mini));
    }

    #[test]
    fn project_3mf_role_round_trips_with_wire_name() {
        assert_eq!(
            serde_json::to_string(&FileRole::Project3mf).unwrap(),
            "\"project_3mf\""
        );
        assert_eq!(
            serde_json::from_str::<FileRole>("\"project_3mf\"").unwrap(),
            FileRole::Project3mf
        );
    }

    #[test]
    fn nozzle_tolerance_includes_boundary() {
        assert!(SliceTarget::nozzle_matches(0.4, 0.401));
        assert!(!SliceTarget::nozzle_matches(0.4, 0.401_000_01));
    }

    #[test]
    fn profile_comparison_uses_unicode_case_folding() {
        assert!(SliceTarget::profile_eq(" Σ ", "ς"));
    }

    #[test]
    fn slice_target_comparisons() {
        assert!(SliceTarget::nozzle_matches(0.4, 0.4005));
        assert!(!SliceTarget::nozzle_matches(0.4, 0.6));
        assert!(SliceTarget::profile_eq(" Speed ", "speed"));
        assert!(SliceTarget::profile_eq("Ébauche", "éBAUCHE"));
    }
}
