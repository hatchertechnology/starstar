use starstar_api::{
    library::{
        blobs::Blobs,
        parts::{PartPatch, PartsError, PartsStore},
    },
    store::Db,
};
use std::sync::Arc;

async fn setup() -> (Db, PartsStore) {
    let db = Db::connect_test().await;
    let store = PartsStore::new(db.pool().clone(), Arc::new(Blobs::temporary().unwrap()));
    (db, store)
}
async fn file(db: &Db, part: i64, rev: i64) -> i64 {
    sqlx::query_scalar("INSERT INTO part_files (part_id,model_revision_id,role,name,sha256,size,kind,parsed,has_preview,uploaded_by,uploaded_at) VALUES ($1,$2,'source','x.stl',$3,1,'source',false,false,'u',1) RETURNING id")
        .bind(part).bind(rev).bind("a".repeat(64)).fetch_one(db.pool()).await.unwrap()
}
#[tokio::test]
async fn create_part_has_model_rev_1() {
    let (_db, s) = setup().await;
    let p = s.create_part("Thing", None, "u").await.unwrap();
    assert_eq!(p.model_revisions[0].number, 1);
}
#[tokio::test]
async fn duplicate_part_name_case_insensitive_conflicts() {
    let (_db, s) = setup().await;
    s.create_part("Thing", None, "u").await.unwrap();
    assert!(matches!(
        s.create_part("thing", None, "u").await,
        Err(PartsError::Conflict(_))
    ));
}
#[tokio::test]
async fn new_setup_has_r1_on_latest_model() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let m = s.new_model_revision(p.id, None, &[], "u").await.unwrap();
    let x = s.create_setup(p.id, "s", "u").await.unwrap();
    assert_eq!(
        (x.revisions[0].number, x.revisions[0].model_revision_id),
        (1, m.id)
    );
}
#[tokio::test]
async fn new_model_revision_numbers_increase_and_carry_files() {
    let (db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let id = file(&db, p.id, p.model_revisions[0].id).await;
    let m = s.new_model_revision(p.id, None, &[id], "u").await.unwrap();
    assert_eq!(m.number, 2);
    assert_eq!(m.files[0].sha256, "a".repeat(64));
}
#[tokio::test]
async fn carry_rejects_foreign_file() {
    let (db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let q = s.create_part("q", None, "u").await.unwrap();
    let id = file(&db, q.id, q.model_revisions[0].id).await;
    assert!(matches!(
        s.new_model_revision(p.id, None, &[id], "u").await,
        Err(PartsError::BadRequest(_))
    ));
}
#[tokio::test]
async fn delete_model_revision_in_use_names_setup() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    s.create_setup(p.id, "Single", "u").await.unwrap();
    s.new_model_revision(p.id, None, &[], "u").await.unwrap();
    assert!(
        s.delete_model_revision(p.model_revisions[0].id)
            .await
            .unwrap_err()
            .to_string()
            .contains("Single")
    );
}
#[tokio::test]
async fn delete_setup_clears_default_setup() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let x = s.create_setup(p.id, "s", "u").await.unwrap();
    s.update_part(
        p.id,
        PartPatch {
            default_setup_id: Some(Some(x.id)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    s.delete_setup(x.id).await.unwrap();
    let p = s.update_part(p.id, PartPatch::default()).await.unwrap();
    assert_eq!(p.default_setup_id, None);
}
#[tokio::test]
async fn revision_numbers_not_reused_after_delete() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let m = s.new_model_revision(p.id, None, &[], "u").await.unwrap();
    s.delete_model_revision(m.id).await.unwrap();
    assert_eq!(
        s.new_model_revision(p.id, None, &[], "u")
            .await
            .unwrap()
            .number,
        3
    );
}
#[tokio::test]
async fn concurrent_setup_revisions_get_distinct_numbers() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let x = s.create_setup(p.id, "s", "u").await.unwrap();
    let (a, b) = tokio::join!(
        s.new_setup_revision(x.id, None, None, "u"),
        s.new_setup_revision(x.id, None, None, "u")
    );
    let mut ns = [a.unwrap().number, b.unwrap().number];
    ns.sort();
    assert_eq!(ns, [2, 3]);
}
#[tokio::test]
async fn delete_populated_part() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    s.create_setup(p.id, "s", "u").await.unwrap();
    s.delete_part(p.id).await.unwrap();
}

#[tokio::test]
async fn deleting_revision_preserves_shared_blob_until_last_reference() {
    let (db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let old = p.model_revisions[0].id;
    let id = file(&db, p.id, old).await;
    let sha = "a".repeat(64);
    let path = s.blobs.path(&sha);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"blob").unwrap();
    let _new = s.new_model_revision(p.id, None, &[id], "u").await.unwrap();
    s.delete_model_revision(old).await.unwrap();
    assert!(path.exists());
    s.delete_part(p.id).await.unwrap();
    assert!(!path.exists());
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM library_cleanup")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(pending, 0);
}

#[tokio::test]
async fn setup_revision_numbers_not_reused_after_delete() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    let setup = s.create_setup(p.id, "s", "u").await.unwrap();
    let r2 = s
        .new_setup_revision(setup.id, None, None, "u")
        .await
        .unwrap();
    s.delete_setup_revision(r2.id).await.unwrap();
    assert_eq!(
        s.new_setup_revision(setup.id, None, None, "u")
            .await
            .unwrap()
            .number,
        3
    );
}

#[tokio::test]
async fn setup_reports_when_model_is_newer() {
    let (_db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    s.create_setup(p.id, "s", "u").await.unwrap();
    s.new_model_revision(p.id, None, &[], "u").await.unwrap();
    let p = s.update_part(p.id, PartPatch::default()).await.unwrap();
    assert_eq!(
        p.setups[0].behind_model.as_deref(),
        Some("on model rev 1; latest is rev 2")
    );
}

#[tokio::test]
async fn names_are_trimmed_and_empty_names_rejected() {
    let (_db, s) = setup().await;
    assert!(matches!(
        s.create_part("  ", None, "u").await,
        Err(PartsError::BadRequest(_))
    ));
    let p = s.create_part("  part  ", None, "u").await.unwrap();
    assert_eq!(p.name, "part");
    assert!(matches!(
        s.update_part(
            p.id,
            PartPatch {
                name: Some("  ".into()),
                ..Default::default()
            }
        )
        .await,
        Err(PartsError::BadRequest(_))
    ));
    assert_eq!(
        s.update_part(
            p.id,
            PartPatch {
                name: Some(" renamed ".into()),
                ..Default::default()
            }
        )
        .await
        .unwrap()
        .name,
        "renamed"
    );
    assert!(matches!(
        s.create_setup(p.id, "  ", "u").await,
        Err(PartsError::BadRequest(_))
    ));
    let setup = s.create_setup(p.id, " setup ", "u").await.unwrap();
    assert_eq!(setup.name, "setup");
    assert!(matches!(
        s.update_setup(setup.id, Some(" ".into()), None).await,
        Err(PartsError::BadRequest(_))
    ));
    assert_eq!(
        s.update_setup(setup.id, Some(" new ".into()), None)
            .await
            .unwrap()
            .name,
        "new"
    );
}

#[tokio::test]
async fn last_model_revision_cannot_be_deleted_and_carry_deduplicates() {
    let (db, s) = setup().await;
    let p = s.create_part("p", None, "u").await.unwrap();
    assert!(
        matches!(s.delete_model_revision(p.model_revisions[0].id).await, Err(PartsError::Conflict(message)) if message == "a part needs at least one model revision")
    );
    let id = file(&db, p.id, p.model_revisions[0].id).await;
    let rev = s
        .new_model_revision(p.id, None, &[id, id], "u")
        .await
        .unwrap();
    assert_eq!(rev.files.len(), 1);
}
