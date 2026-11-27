use starstar_api::library::parts::{MatchPrinter, PartsStore, check_override, match_printers};
use starstar_api::{library::blobs::Blobs, store::Db};
use starstar_shared::{
    FileRole, ModelRevision, Part, PartFile, PartSort, PrinterFamily, PrinterKind, Setup,
    SetupRevision, SliceTarget,
};
use std::sync::Arc;

fn file(id: i64, profile: Option<&str>) -> PartFile {
    PartFile {
        id,
        part_id: 1,
        model_revision_id: None,
        setup_revision_id: Some(1),
        role: FileRole::Slice,
        name: "x.gcode".into(),
        sha256: "a".repeat(64),
        size: 1,
        meta: None,
        parsed: true,
        has_preview: false,
        target: Some(SliceTarget {
            family: Some(PrinterFamily::Mk4),
            nozzle: Some(0.4),
            profile: profile.map(str::to_owned),
        }),
        superseded_by: None,
        note: None,
        uploaded_by: "u".into(),
        uploaded_at: 1,
    }
}
fn part() -> Part {
    Part {
        id: 1,
        name: "part".into(),
        notes: None,
        default_setup_id: Some(1),
        created_by: "u".into(),
        created_at: 1,
        preview_file_id: None,
        model_revisions: vec![ModelRevision {
            id: 1,
            number: 1,
            note: None,
            created_by: "u".into(),
            created_at: 1,
            files: vec![],
        }],
        setups: vec![Setup {
            id: 1,
            name: "Full plate".into(),
            default_profile: None,
            revisions: vec![SetupRevision {
                id: 1,
                number: 1,
                model_revision_id: 1,
                model_revision_number: 1,
                note: None,
                created_by: "u".into(),
                created_at: 1,
                project: None,
                slices: vec![file(1, Some("speed"))],
            }],
            behind_model: None,
        }],
        other_files: vec![],
    }
}
fn printer() -> MatchPrinter {
    MatchPrinter {
        name: "p".into(),
        kind: PrinterKind::Mk4,
        nozzle: Some(0.4),
    }
}
fn result(p: &Part, profile: Option<&str>) -> starstar_shared::PrinterMatch {
    match_printers(p, None, profile, &[printer()])
        .unwrap()
        .remove(0)
}
#[test]
fn matches_by_family_nozzle_profile() {
    assert_eq!(result(&part(), Some(" SPEED ")).slice.unwrap().id, 1);
}
#[test]
fn uses_default_setup_and_default_profile() {
    let mut p = part();
    p.setups[0].default_profile = Some("speed".into());
    p.setups[0].revisions[0]
        .slices
        .push(file(2, Some("quality")));
    assert_eq!(result(&p, None).slice.unwrap().id, 1);
}
#[test]
fn single_profile_needs_no_choice() {
    assert!(result(&part(), None).slice.is_some());
    let mut p = part();
    p.setups[0].revisions[0].slices[0]
        .target
        .as_mut()
        .unwrap()
        .profile = None;
    assert!(result(&p, None).slice.is_some());
}
#[test]
fn two_profiles_without_default_asks() {
    let mut p = part();
    p.setups[0].revisions[0]
        .slices
        .push(file(2, Some("quality")));
    assert_eq!(
        result(&p, None).blocked.as_deref(),
        Some("choose a profile")
    );
}
#[test]
fn overlapping_nozzles_block_ambiguous_match() {
    let mut p = part();
    let first = &mut p.setups[0].revisions[0].slices[0];
    first.name = "first.gcode".into();
    first.target.as_mut().unwrap().nozzle = Some(0.401);
    let mut second = file(2, Some("speed"));
    second.name = "second.gcode".into();
    second.target.as_mut().unwrap().nozzle = Some(0.402);
    p.setups[0].revisions[0].slices.push(second);
    let mut pr = printer();
    pr.nozzle = Some(0.401);
    let matched = match_printers(&p, None, None, &[pr]).unwrap().remove(0);
    assert!(matched.slice.is_none());
    assert_eq!(
        matched.blocked.as_deref(),
        Some("several slices fit MK4 · 0.401 · speed: first.gcode, second.gcode")
    );
}
#[test]
fn unknown_hardware_does_not_affect_profile_inference() {
    for missing_family in [true, false] {
        let mut p = part();
        let mut unknown = file(2, Some("quality"));
        let target = unknown.target.as_mut().unwrap();
        if missing_family {
            target.family = None;
        } else {
            target.nozzle = None;
        }
        p.setups[0].revisions[0].slices.push(unknown);
        assert_eq!(result(&p, None).slice.unwrap().id, 1);
    }
}
#[test]
fn behind_model_blocks_every_printer() {
    let mut p = part();
    p.model_revisions[0].number = 2;
    let r = match_printers(&p, None, None, &[printer(), printer()]).unwrap();
    assert!(
        r.iter()
            .all(|m| m.blocked.as_deref() == Some("Full plate is on model rev 1; latest is rev 2"))
    );
}
#[test]
fn older_revision_slices_not_matched() {
    let mut p = part();
    let mut r = p.setups[0].revisions[0].clone();
    r.number = 2;
    r.slices.clear();
    p.setups[0].revisions.insert(0, r);
    assert!(result(&p, Some("speed")).slice.is_none());
}
#[test]
fn superseded_not_matched() {
    let mut p = part();
    p.setups[0].revisions[0].slices[0].superseded_by = Some(2);
    assert!(result(&p, Some("speed")).slice.is_none());
}
#[test]
fn unknown_target_not_matched() {
    let mut p = part();
    p.setups[0].revisions[0].slices[0].target = None;
    assert!(result(&p, Some("speed")).slice.is_none());
}
#[test]
fn no_slice_reason_text() {
    assert_eq!(
        result(&part(), Some("quality")).blocked.as_deref(),
        Some("no slice for MK4 · 0.4 · quality in Full plate r1")
    );
}
#[test]
fn override_hardware_mismatch_blocks() {
    let mut pr = printer();
    pr.nozzle = Some(0.6);
    assert!(check_override(&part(), 1, &pr).is_err());
}
#[test]
fn override_old_revision_warns() {
    let mut p = part();
    let mut r = p.setups[0].revisions[0].clone();
    r.number = 2;
    r.slices.clear();
    p.setups[0].revisions.insert(0, r);
    assert!(
        check_override(&p, 1, &printer())
            .unwrap()
            .contains(&"from Full plate r1; latest is r2".to_owned())
    );
}
#[tokio::test]
async fn tree_flags_behind_model() {
    let db = Db::connect_test().await;
    let s = PartsStore::new(db.pool().clone(), Arc::new(Blobs::temporary().unwrap()));
    let p = s.create_part("p", None, "u").await.unwrap();
    s.create_setup(p.id, "s", "u").await.unwrap();
    s.new_model_revision(p.id, None, &[], "u").await.unwrap();
    let p = s.tree(p.id).await.unwrap().unwrap();
    assert_eq!(
        p.setups[0].behind_model.as_deref(),
        Some("on model rev 1; latest is rev 2")
    );
    assert_eq!(p.model_revisions[0].number, 2);
}
#[tokio::test]
async fn list_search_finds_profile_label() {
    let db = Db::connect_test().await;
    let s = PartsStore::new(db.pool().clone(), Arc::new(Blobs::temporary().unwrap()));
    let p = s.create_part("p", None, "u").await.unwrap();
    let setup = s.create_setup(p.id, "s", "u").await.unwrap();
    sqlx::query("INSERT INTO part_files (part_id,setup_revision_id,role,name,sha256,size,kind,parsed,has_preview,uploaded_by,uploaded_at,profile) VALUES ($1,$2,'slice','x.gcode',$3,1,'gcode',false,false,'u',1,'Speed')").bind(p.id).bind(setup.revisions[0].id).bind("a".repeat(64)).execute(db.pool()).await.unwrap();
    assert_eq!(
        s.list(Some("speED"), PartSort::Name).await.unwrap()[0].id,
        p.id
    );
}
