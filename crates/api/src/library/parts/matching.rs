use super::PartsError;
use starstar_shared::{Part, PrinterKind, PrinterMatch, SliceTarget};

pub struct MatchPrinter {
    pub name: String,
    pub kind: PrinterKind,
    pub nozzle: Option<f64>,
}
fn profiles_eq(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => SliceTarget::profile_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}
fn latest_model(part: &Part) -> i32 {
    part.model_revisions
        .iter()
        .map(|r| r.number)
        .max()
        .unwrap_or(0)
}
pub fn match_printers(
    part: &Part,
    setup_id: Option<i64>,
    profile: Option<&str>,
    printers: &[MatchPrinter],
) -> Result<Vec<PrinterMatch>, PartsError> {
    let setup = match setup_id.or(part.default_setup_id) {
        Some(id) => Some(
            part.setups
                .iter()
                .find(|s| s.id == id)
                .ok_or_else(|| PartsError::NotFound("setup not found".into()))?,
        ),
        None => None,
    };
    let mut common = None;
    let mut revision = None;
    let mut chosen = profile.or_else(|| setup.and_then(|s| s.default_profile.as_deref()));
    if let Some(s) = setup {
        revision = s.revisions.first();
        if let Some(r) = revision {
            let latest = latest_model(part);
            if r.model_revision_number < latest {
                common = Some(format!(
                    "{} is on model rev {}; latest is rev {}",
                    s.name, r.model_revision_number, latest
                ));
            }
            if common.is_none() && chosen.is_none() {
                let mut profiles: Vec<Option<&str>> = Vec::new();
                for f in &r.slices {
                    if f.superseded_by.is_none()
                        && let Some(t) = &f.target
                    {
                        let p = t.profile.as_deref();
                        if !profiles.iter().any(|x| profiles_eq(*x, p)) {
                            profiles.push(p);
                        }
                    }
                }
                if profiles.len() > 1 {
                    common = Some("choose a profile".into());
                } else if let Some(p) = profiles.first() {
                    chosen = *p;
                }
            }
        }
    } else {
        common = Some("choose a setup".into());
    }
    Ok(printers
        .iter()
        .map(|printer| {
            let mut blocked = common.clone();
            let mut slice = None;
            if blocked.is_none() {
                if let Some(nozzle) = printer.nozzle {
                    if let (Some(s), Some(r)) = (setup, revision) {
                        slice =
                            r.slices
                                .iter()
                                .find(|f| {
                                    f.superseded_by.is_none()
                                        && f.target.as_ref().is_some_and(|t| {
                                            t.family.is_some_and(|family| {
                                                family.matches_kind(printer.kind)
                                            }) && t.nozzle.is_some_and(|n| {
                                                SliceTarget::nozzle_matches(n, nozzle)
                                            }) && profiles_eq(t.profile.as_deref(), chosen)
                                        })
                                })
                                .cloned();
                        if slice.is_none() {
                            let family = match printer.kind {
                                PrinterKind::Mk4 => "MK4",
                                PrinterKind::Mini => "MINI",
                            };
                            blocked = Some(format!(
                                "no slice for {family} · {nozzle} · {} in {} r{}",
                                chosen.unwrap_or("no profile"),
                                s.name,
                                r.number
                            ));
                        }
                    } else {
                        blocked = Some("setup has no revisions".into());
                    }
                } else {
                    blocked = Some("printer nozzle unknown".into());
                }
            }
            PrinterMatch {
                printer: printer.name.clone(),
                slice,
                blocked,
                warnings: vec![],
            }
        })
        .collect())
}
pub fn check_override(
    part: &Part,
    slice_id: i64,
    printer: &MatchPrinter,
) -> Result<Vec<String>, String> {
    for setup in &part.setups {
        for revision in &setup.revisions {
            if let Some(file) = revision.slices.iter().find(|f| f.id == slice_id) {
                let mut warnings = Vec::new();
                if let Some(t) = &file.target {
                    if let Some(family) = t.family
                        && !family.matches_kind(printer.kind)
                    {
                        return Err("printer family mismatch".into());
                    }
                    if let Some(nozzle) = t.nozzle {
                        let actual = printer
                            .nozzle
                            .ok_or_else(|| "printer nozzle unknown".to_owned())?;
                        if !SliceTarget::nozzle_matches(nozzle, actual) {
                            return Err("printer nozzle mismatch".into());
                        }
                    }
                    if t.family.is_none() || t.nozzle.is_none() {
                        warnings.push("unknown target".into());
                    }
                } else {
                    warnings.push("unknown target".into());
                }
                if let Some(latest) = setup.revisions.first()
                    && revision.number < latest.number
                {
                    warnings.push(format!(
                        "from {} r{}; latest is r{}",
                        setup.name, revision.number, latest.number
                    ));
                }
                let latest = latest_model(part);
                if revision.model_revision_number < latest {
                    warnings.push(format!(
                        "built from model rev {}; latest is rev {}",
                        revision.model_revision_number, latest
                    ));
                }
                if file.superseded_by.is_some() {
                    warnings.push("replaced".into());
                }
                return Ok(warnings);
            }
        }
    }
    Err("slice not found".into())
}
