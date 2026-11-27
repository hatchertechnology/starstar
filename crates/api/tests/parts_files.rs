use starstar_api::library::parts::{FileLevel, UploadSpec};
use starstar_shared::FileRole;

use axum::body::Bytes;
use futures_util::stream;
use starstar_api::{
    library::{
        blobs::Blobs,
        parts::{PartsError, PartsStore},
    },
    store::Db,
};
use starstar_shared::PrinterFamily;
use std::sync::Arc;

async fn setup() -> (Db, PartsStore) {
    let db = Db::connect_test().await;
    let store = PartsStore::new(db.pool().clone(), Arc::new(Blobs::temporary().unwrap()));
    (db, store)
}
fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../prints/cube-12mm/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}
#[allow(clippy::too_many_arguments)]
async fn upload(
    s: &PartsStore,
    part: i64,
    level: FileLevel,
    role: FileRole,
    name: &str,
    data: Vec<u8>,
    profile: Option<&str>,
    replace: bool,
) -> Result<(starstar_shared::PartFile, bool), PartsError> {
    let len = data.len() as u64;
    s.add_file(
        UploadSpec {
            part_id: part,
            model_revision_id: None,
            level,
            role,
            name: name.into(),
            profile: profile.map(str::to_owned),
            replace,
        },
        "u",
        stream::iter([Ok::<_, ()>(Bytes::from(data))]),
        len,
        len,
    )
    .await
}
#[tokio::test]
async fn upload_source_and_render_to_model_revision() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let id = p.model_revisions[0].id;
    let data = fixture("cube-12mm.step");
    let (a, created) = upload(
        &s,
        p.id,
        FileLevel::ModelRevision(id),
        FileRole::Source,
        "cube.step",
        data.clone(),
        None,
        false,
    )
    .await
    .unwrap();
    assert!(created);
    let (again, created) = upload(
        &s,
        p.id,
        FileLevel::ModelRevision(id),
        FileRole::Source,
        "cube.step",
        data,
        None,
        false,
    )
    .await
    .unwrap();
    assert!(!created);
    assert_eq!(a.id, again.id);
    let (b, _) = upload(
        &s,
        p.id,
        FileLevel::ModelRevision(id),
        FileRole::Render,
        "render.png",
        vec![1, 2],
        None,
        false,
    )
    .await
    .unwrap();
    assert_eq!(b.model_revision_id, Some(id));
    let b = s
        .update_file(b.id, None, Some(Some("note".into())))
        .await
        .unwrap();
    assert_eq!(b.note.as_deref(), Some("note"));
}
#[tokio::test]
async fn slice_target_read_from_file() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let setup = s.create_setup(p.id, "Full plate", "u").await.unwrap();
    let (f, _) = upload(
        &s,
        p.id,
        FileLevel::Setup(setup.id),
        FileRole::Slice,
        "cube.bgcode",
        fixture("cube-12mm.MK4.bgcode"),
        None,
        false,
    )
    .await
    .unwrap();
    let t = f.target.unwrap();
    assert_eq!(t.family, Some(PrinterFamily::Mk4));
    assert_eq!(t.nozzle, Some(0.4));
    assert_eq!(t.profile.as_deref(), Some("0.20mm SPEED @MK4IS 0.4"));
}
#[tokio::test]
async fn profile_override_on_upload() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let setup = s.create_setup(p.id, "s", "u").await.unwrap();
    let (f, _) = upload(
        &s,
        p.id,
        FileLevel::Setup(setup.id),
        FileRole::Slice,
        "cube.bgcode",
        fixture("cube-12mm.MK4.bgcode"),
        Some("speed"),
        false,
    )
    .await
    .unwrap();
    assert_eq!(f.target.unwrap().profile.as_deref(), Some("speed"));
}
#[tokio::test]
async fn wrong_extension_for_role_rejected() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let e = upload(
        &s,
        p.id,
        FileLevel::Part,
        FileRole::Slice,
        "cube.stl",
        vec![],
        None,
        false,
    )
    .await
    .unwrap_err();
    assert!(matches!(e, PartsError::BadRequest(_)));
    assert!(e.to_string().contains(".bgcode, .gcode"));
}
#[tokio::test]
async fn slice_needs_setup_level() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    assert!(matches!(
        upload(
            &s,
            p.id,
            FileLevel::Part,
            FileRole::Slice,
            "cube.gcode",
            vec![],
            None,
            false
        )
        .await,
        Err(PartsError::BadRequest(_))
    ));
}
#[tokio::test]
async fn level_from_other_part_not_found() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let q = s.create_part("q", None, "u").await.unwrap();
    assert!(matches!(
        upload(
            &s,
            p.id,
            FileLevel::ModelRevision(q.model_revisions[0].id),
            FileRole::Source,
            "cube.stl",
            vec![],
            None,
            false
        )
        .await,
        Err(PartsError::NotFound(_))
    ));
}
#[tokio::test]
async fn duplicate_target_conflicts_without_replace() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let x = s.create_setup(p.id, "Full plate", "u").await.unwrap();
    let (a, _) = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Slice,
        "a.bgcode",
        fixture("cube-12mm.MK4.bgcode"),
        Some("speed"),
        false,
    )
    .await
    .unwrap();
    let e = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Slice,
        "b.bgcode",
        fixture("cube-12mm.MK4S.bgcode"),
        Some("speed"),
        false,
    )
    .await
    .unwrap_err();
    assert_eq!(
        e.to_string(),
        format!(
            "Full plate r1 already has a slice for MK4 · 0.4 · speed: a.bgcode (id {})",
            a.id
        )
    );
}
#[tokio::test]
async fn replace_supersedes_old_slice() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let x = s.create_setup(p.id, "s", "u").await.unwrap();
    let (a, _) = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Slice,
        "a.bgcode",
        fixture("cube-12mm.MK4.bgcode"),
        Some("speed"),
        false,
    )
    .await
    .unwrap();
    let (b, _) = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Slice,
        "b.bgcode",
        fixture("cube-12mm.MK4S.bgcode"),
        Some("speed"),
        true,
    )
    .await
    .unwrap();
    assert_eq!(
        s.get_file(a.id).await.unwrap().unwrap().superseded_by,
        Some(b.id)
    );
}
#[tokio::test]
async fn threemf_fills_empty_latest_revision_then_creates_next() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let x = s.create_setup(p.id, "s", "u").await.unwrap();
    let r = x.revisions[0].id;
    let (a, _) = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Project3mf,
        "a.3mf",
        vec![1],
        None,
        false,
    )
    .await
    .unwrap();
    assert_eq!(a.setup_revision_id, Some(r));
    let m = s.new_model_revision(p.id, None, &[], "u").await.unwrap();
    let (b, _) = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Project3mf,
        "b.3mf",
        vec![2],
        None,
        false,
    )
    .await
    .unwrap();
    assert_ne!(b.setup_revision_id, Some(r));
    let p = s.get_part(p.id).await.unwrap();
    assert_eq!(p.setups[0].revisions[0].model_revision_id, m.id);
}
#[tokio::test]
async fn shared_blob_survives_delete_in_other_part() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let q = s.create_part("q", None, "u").await.unwrap();
    let data = fixture("cube-12mm.stl");
    let (a, _) = upload(
        &s,
        p.id,
        FileLevel::ModelRevision(p.model_revisions[0].id),
        FileRole::Source,
        "a.stl",
        data.clone(),
        None,
        false,
    )
    .await
    .unwrap();
    let (b, _) = upload(
        &s,
        q.id,
        FileLevel::ModelRevision(q.model_revisions[0].id),
        FileRole::Source,
        "b.stl",
        data,
        None,
        false,
    )
    .await
    .unwrap();
    s.delete_file(a.id).await.unwrap();
    assert!(s.blobs.path(&b.sha256).exists());
    s.delete_file(b.id).await.unwrap();
    assert!(!s.blobs.path(&b.sha256).exists());
}
#[tokio::test]
async fn delete_file_unsets_superseded_by() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let x = s.create_setup(p.id, "s", "u").await.unwrap();
    let (a, _) = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Slice,
        "a.bgcode",
        fixture("cube-12mm.MK4.bgcode"),
        Some("speed"),
        false,
    )
    .await
    .unwrap();
    let (b, _) = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Slice,
        "b.bgcode",
        fixture("cube-12mm.MK4S.bgcode"),
        Some("speed"),
        true,
    )
    .await
    .unwrap();
    let (c, _) = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Slice,
        "c.bgcode",
        fixture("cube-12mm.MK4.bgcode"),
        Some("speed"),
        true,
    )
    .await
    .unwrap();
    s.delete_file(c.id).await.unwrap();
    assert_eq!(
        s.get_file(a.id).await.unwrap().unwrap().superseded_by,
        Some(b.id)
    );
    assert_eq!(s.get_file(b.id).await.unwrap().unwrap().superseded_by, None);
}
#[tokio::test]
async fn unparsed_slice_has_unknown_target_and_no_conflict() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let x = s.create_setup(p.id, "s", "u").await.unwrap();
    for (name, body) in [
        ("a.gcode", b"G1 X1".as_slice()),
        ("b.gcode", b"G1 X2".as_slice()),
    ] {
        let (f, _) = upload(
            &s,
            p.id,
            FileLevel::Setup(x.id),
            FileRole::Slice,
            name,
            body.to_vec(),
            None,
            false,
        )
        .await
        .unwrap();
        assert!(!f.parsed);
        assert_eq!(f.target.unwrap().family, None);
    }
}

#[tokio::test]
async fn relabel_rejects_existing_target_even_with_whitespace() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let x = s.create_setup(p.id, "s", "u").await.unwrap();
    upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Slice,
        "a.bgcode",
        fixture("cube-12mm.MK4.bgcode"),
        Some("speed"),
        false,
    )
    .await
    .unwrap();
    let (b, _) = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Slice,
        "b.bgcode",
        fixture("cube-12mm.MK4S.bgcode"),
        Some("quality"),
        false,
    )
    .await
    .unwrap();
    assert!(matches!(
        s.update_file(b.id, Some(Some(" SPEED ".into())), None)
            .await,
        Err(PartsError::Conflict(_))
    ));
}

#[tokio::test]
async fn project_override_and_empty_setup_revision() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let q = s.create_part("q", None, "u").await.unwrap();
    let x = s.create_setup(p.id, "s", "u").await.unwrap();
    s.delete_setup_revision(x.revisions[0].id).await.unwrap();
    let err = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Slice,
        "a.gcode",
        vec![1],
        None,
        false,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, PartsError::BadRequest(_)));
    assert_eq!(err.to_string(), "setup s has no revisions");
    let spec = |model_revision_id| UploadSpec {
        part_id: p.id,
        model_revision_id,
        level: FileLevel::Setup(x.id),
        role: FileRole::Project3mf,
        name: "a.3mf".into(),
        profile: None,
        replace: false,
    };
    let send = |spec| {
        s.add_file(
            spec,
            "u",
            stream::iter([Ok::<_, ()>(Bytes::from_static(b"x"))]),
            1,
            1,
        )
    };
    assert!(matches!(
        send(spec(Some(q.model_revisions[0].id))).await,
        Err(PartsError::NotFound(_))
    ));
    let newer = s.new_model_revision(p.id, None, &[], "u").await.unwrap();
    let (f, _) = send(spec(Some(p.model_revisions[0].id))).await.unwrap();
    let p = s.get_part(p.id).await.unwrap();
    assert_eq!(p.setups[0].revisions[0].number, 2);
    assert_eq!(
        p.setups[0].revisions[0].model_revision_id,
        p.model_revisions[1].id
    );
    assert_ne!(p.model_revisions[1].id, newer.id);
    assert_eq!(f.setup_revision_id, Some(p.setups[0].revisions[0].id));
}
#[tokio::test]
async fn reupload_old_bytes_creates_new_current_slice() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let x = s.create_setup(p.id, "s", "u").await.unwrap();
    let a = fixture("cube-12mm.MK4.bgcode");
    let b = fixture("cube-12mm.MK4S.bgcode");
    let (first, _) = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Slice,
        "plate.bgcode",
        a.clone(),
        Some("speed"),
        false,
    )
    .await
    .unwrap();
    let (second, _) = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Slice,
        "plate.bgcode",
        b,
        Some("speed"),
        true,
    )
    .await
    .unwrap();
    let (third, created) = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Slice,
        "plate.bgcode",
        a,
        Some("speed"),
        true,
    )
    .await
    .unwrap();
    assert!(created);
    assert_ne!(first.id, third.id);
    assert_eq!(
        s.get_file(second.id).await.unwrap().unwrap().superseded_by,
        Some(third.id)
    );
}
#[tokio::test]
async fn deleting_relabeled_replacement_keeps_existing_current_slice() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let x = s.create_setup(p.id, "s", "u").await.unwrap();
    let (a, _) = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Slice,
        "a.bgcode",
        fixture("cube-12mm.MK4.bgcode"),
        Some("speed"),
        false,
    )
    .await
    .unwrap();
    let (b, _) = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Slice,
        "b.bgcode",
        fixture("cube-12mm.MK4S.bgcode"),
        Some("speed"),
        true,
    )
    .await
    .unwrap();
    s.update_file(b.id, Some(Some("quality".into())), None)
        .await
        .unwrap();
    let (c, _) = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Slice,
        "c.bgcode",
        fixture("cube-12mm.MK4.bgcode"),
        Some(" speed "),
        false,
    )
    .await
    .unwrap();
    s.delete_file(b.id).await.unwrap();
    assert_eq!(
        s.get_file(a.id).await.unwrap().unwrap().superseded_by,
        Some(c.id)
    );
    assert_eq!(s.get_file(c.id).await.unwrap().unwrap().superseded_by, None);
}
#[tokio::test]
async fn slice_target_normalization_and_occupied_project_conflict() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let x = s.create_setup(p.id, "s", "u").await.unwrap();
    let rev = x.revisions[0].id;
    let data = b"; printer_model = MK4\n; nozzle_diameter = 0.4008\n".to_vec();
    let (a, _) = upload(
        &s,
        p.id,
        FileLevel::SetupRevision(rev),
        FileRole::Slice,
        "a.gcode",
        data,
        Some("   "),
        false,
    )
    .await
    .unwrap();
    assert_eq!(a.target.unwrap().nozzle, Some(0.401));
    let data = b"; printer_model = MK4\n; nozzle_diameter = 0.4011\n".to_vec();
    assert!(matches!(
        upload(
            &s,
            p.id,
            FileLevel::SetupRevision(rev),
            FileRole::Slice,
            "b.gcode",
            data,
            None,
            false
        )
        .await,
        Err(PartsError::Conflict(_))
    ));
    upload(
        &s,
        p.id,
        FileLevel::SetupRevision(rev),
        FileRole::Project3mf,
        "a.3mf",
        vec![1],
        None,
        false,
    )
    .await
    .unwrap();
    let err = upload(
        &s,
        p.id,
        FileLevel::SetupRevision(rev),
        FileRole::Project3mf,
        "b.3mf",
        vec![2],
        None,
        false,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, PartsError::Conflict(_)));
    assert!(err.to_string().contains("already has a 3MF"));
}

#[tokio::test]
async fn target_rounding_replaces_only_matching_index_key() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let x = s.create_setup(p.id, "s", "u").await.unwrap();
    let data = |n: &str| format!("; printer_model = MK4\n; nozzle_diameter = {n}\n").into_bytes();
    let (a, _) = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Slice,
        "a.gcode",
        data("0.4000"),
        Some(" speed "),
        false,
    )
    .await
    .unwrap();
    let (b, _) = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Slice,
        "b.gcode",
        data("0.4015"),
        Some("speed"),
        false,
    )
    .await
    .unwrap();
    let (c, _) = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Slice,
        "c.gcode",
        data("0.4008"),
        Some("speed"),
        true,
    )
    .await
    .unwrap();
    assert_eq!(s.get_file(a.id).await.unwrap().unwrap().superseded_by, None);
    assert_eq!(s.get_file(b.id).await.unwrap().unwrap().superseded_by, None);
    assert_eq!(c.target.unwrap().nozzle, Some(0.401));
    let (d, _) = upload(
        &s,
        p.id,
        FileLevel::Setup(x.id),
        FileRole::Slice,
        "d.gcode",
        data("0.4009"),
        Some("SPEED"),
        true,
    )
    .await
    .unwrap();
    assert_eq!(
        s.get_file(c.id).await.unwrap().unwrap().superseded_by,
        Some(d.id)
    );
}
