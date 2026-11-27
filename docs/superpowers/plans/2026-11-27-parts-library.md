# Parts Library Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the flat library of sliced files with parts: one ID per part, holding model revisions, named setups with revisions, per-target slices, renders, source files, 3MF and other files, with automatic slice matching and staleness flags.

**Architecture:** New Postgres tables per layer (migration v4) over the existing SHA-256 blob store. The blob code in `crates/api/src/library/store.rs` is split into a blob layer (`blobs.rs`) and a parts catalog (`parts/`). New `/api/parts…`, `/api/setups…`, `/api/files…` routes replace `/api/library…`; CLI `part` replaces `library`; MCP `part_*` replaces `library_*`; the web Library pages show parts.

**Tech Stack:** Rust (axum, sqlx/Postgres, tokio, rmcp, clap), Nuxt 3 + Vitest, `image` crate, `zip` crate (new).

**Spec:** `docs/superpowers/specs/2026-11-27-parts-library-design.md` (read it with this plan; the spec's wording wins on behavior).

## Global Constraints

- Work on `main`, commit after each task, do not push (AGENTS.md). Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Roles: Viewer reads/downloads; Operator uploads, creates revisions/setups, edits labels/defaults; Admin deletes.
- Upload limit: `state.config.max_upload_bytes` (`STARSTAR_MAX_UPLOAD_MB`, default 512); uploads need `Content-Length` (411 otherwise), as in phase 1.
- Preview/download headers identical to phase 1 routes (`nosniff`, sandbox CSP, `private, max-age=300`). `other` files always `Content-Disposition: attachment`.
- Nozzle compare tolerance ±0.001. Profile compare: trimmed, case-insensitive.
- Part and setup names unique case-insensitively (setup: per part).
- Revision numbers are never reused, assigned under a per-part advisory lock.
- Every `kubectl` command uses `--context thor`. Run `deploy/thor/preflight.sh` before `apply -k`. Never `--prune`.
- Verification suite (run before each commit touching that side): `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`; `npm --prefix web run typecheck && npm --prefix web test`.

## Review Focus

1. **Matching after New revision** — a setup whose latest revision is built from an older model revision must return a blocked reason for every printer, never a slice. Test in Task 6.
2. **Slice re-upload** — same target in the same setup revision without `replace` → 409 naming the existing slice; with `replace` → old one gets `superseded_by` and is excluded from matching. Test in Task 5.
3. **Shared blobs across parts** — the same bytes uploaded to two parts, then one deleted: the blob and thumbnail must survive. Test in Task 5.
4. **3MF upload into a setup whose latest revision has no 3MF** — goes into that revision, no new revision. A second 3MF creates r(n+1). Test in Task 5.
5. **Migration with case-colliding phase 1 names** (`Clip.bgcode`, `clip.bgcode`) — must not fail; second part becomes `clip (2)`. Test in Task 8.

---

### Task 1: Parser reads `print_settings_id`

**Files:**
- Modify: `crates/shared/src/printers.rs:44-63` (`PrintMeta`)
- Modify: `crates/api/src/library/parse.rs:74-136` (`update_meta`)
- Test: `crates/api/src/library/parse.rs` (unit tests module)

**Interfaces:**
- Produces: `PrintMeta.print_settings_id: Option<String>` (serde default, skip if none).

- [ ] **Step 1: Write failing tests** in parse.rs tests:
  - `reads_print_settings_id_from_bgcode`: parse `prints/cube-12mm/cube-12mm.MK4.bgcode`; `assert_eq!(p.meta.print_settings_id.as_deref(), Some("0.20mm SPEED @MK4IS 0.4"))`.
  - `reads_print_settings_id_from_ascii`: ASCII sample with `; print_settings_id = 0.15mm QUALITY @MINI` → `Some("0.15mm QUALITY @MINI")`.
  - Empty value (`; print_settings_id = `) → `None`.
- [ ] **Step 2:** `cargo test -p starstar-api print_settings_id` → FAIL (no field).
- [ ] **Step 3:** Add the field; add `"print_settings_id"` to the key match and `set!(print_settings_id, non-empty string)`. If the bgcode test fails because the key is absent from the metadata blocks the parser reads, report it — do not widen the parser's budgets.
- [ ] **Step 4:** Re-run → PASS. Run the full verification suite (other crates construct `PrintMeta`; fix struct literals with `..Default::default()` or the new field).
- [ ] **Step 5: Commit** `Read the print profile name from slices`.

### Task 2: Shared parts types and target rules

**Files:**
- Create: `crates/shared/src/parts.rs`
- Modify: `crates/shared/src/lib.rs` (`pub mod parts; pub use parts::*;`)

**Interfaces (Produces, all `Serialize, Deserialize, Debug, Clone, PartialEq`; enums `snake_case`):**
- `enum FileRole { Source, Render, Project3mf, Slice, Other }` with `fn accepts(self, file_name: &str) -> bool` and `fn infer(file_name: &str) -> Option<FileRole>` (`.3mf`→Project3mf, `.bgcode/.gcode`→Slice, image exts→Render, source exts→Source; else None). Extension table exactly as the spec's "File roles" table, case-insensitive. `Other::accepts` is always true.
- `enum PrinterFamily { Mk4, Mini }` with `fn from_printer_model(model: &str) -> Option<PrinterFamily>` (prefix `MK4` → Mk4, `MINI` → Mini, case-insensitive) and `fn matches_kind(self, kind: PrinterKind) -> bool`.
- `struct SliceTarget { family: Option<PrinterFamily>, nozzle: Option<f64>, profile: Option<String> }` with `fn nozzle_matches(a: f64, b: f64) -> bool` (±0.001) and `fn profile_eq(a: &str, b: &str) -> bool`.
- `struct PartFile { id: i64, part_id: i64, model_revision_id: Option<i64>, setup_revision_id: Option<i64>, role: FileRole, name: String, sha256: String, size: u64, meta: Option<PrintMeta>, parsed: bool, has_preview: bool, target: Option<SliceTarget>, superseded_by: Option<i64>, note: Option<String>, uploaded_by: String, uploaded_at: i64 }`
- `struct ModelRevision { id, number: i32, note: Option<String>, created_by, created_at, files: Vec<PartFile> }`
- `struct SetupRevision { id, number: i32, model_revision_id: i64, model_revision_number: i32, note, created_by, created_at, project: Option<PartFile>, slices: Vec<PartFile> }`
- `struct Setup { id, name, default_profile: Option<String>, revisions: Vec<SetupRevision>, behind_model: Option<String> }` (`behind_model` = the flag text, e.g. `"on model rev 1; latest is rev 2"`).
- `struct Part { id, name, notes: Option<String>, default_setup_id: Option<i64>, created_by, created_at, preview_file_id: Option<i64>, model_revisions: Vec<ModelRevision>, setups: Vec<Setup>, other_files: Vec<PartFile> }`
- `struct PartSummary { id, name, latest_model_revision: i32, setups: u32, slices: u32, preview_file_id: Option<i64>, updated_at: i64 }`
- `struct PartQuery { q: Option<String>, sort: Option<PartSort> }`, `enum PartSort { Newest, Name }` (default Newest).
- `struct PrinterMatch { printer: String, slice: Option<PartFile>, blocked: Option<String>, warnings: Vec<String> }`
- Slice staleness is not stored on `PartFile`: a slice is stale when its `SetupRevision` is not the setup's first (newest) revision, or `superseded_by` is set. UI and CLI derive "from rN" / "replaced" from that.

- [ ] **Step 1: Write failing unit tests** in parts.rs: `FileRole::infer("a.STEP") == Some(Source)`, `infer("x.3mf") == Some(Project3mf)`, `infer("notes.pdf") == None`, `Render.accepts("a.webp")`, `!Slice.accepts("a.stl")`; `from_printer_model("MK4S") == Some(Mk4)`, `("MK4IS")→Mk4`, `("MINI")→Mini`, `("XL")→None`; `nozzle_matches(0.4, 0.4005)`, `!nozzle_matches(0.4, 0.6)`; `profile_eq(" Speed ", "speed")`.
- [ ] **Step 2:** `cargo test -p starstar-shared parts` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** → PASS. Commit `Add shared types for parts`.

### Task 3: Migration v4 tables and the blob layer

**Files:**
- Modify: `crates/api/src/store.rs` (append `Migration { version: 4, name: "parts", sql }` after v3)
- Create: `crates/api/src/library/blobs.rs` (moved from `store.rs`: dir setup, `blob_for`, ingest, `reconcile`, `cleanup_digest`, fs helpers)
- Modify: `crates/api/src/library/store.rs` (keep phase 1 catalog methods compiling on top of `Blobs` until Task 7 removes them)
- Modify: `crates/api/Cargo.toml` (`image` features add `jpeg`, `webp`; add `zip = { version = "2", default-features = false, features = ["deflate"] }`)
- Test: `crates/api/src/library/blobs.rs` tests (move the existing store.rs tests that cover blobs, keep them passing), `crates/api/tests/postgres_store.rs`

**SQL for v4 (create only; data copy + drop come in Task 8):** tables exactly as the spec's "Database (migration v4)" section, plus:
- `parts.name` unique via `CREATE UNIQUE INDEX parts_name_ci ON parts (lower(name))`; setups likewise `(part_id, lower(name))`.
- `parts.default_setup_id REFERENCES setups(id) ON DELETE SET NULL` (add with `ALTER TABLE` after `setups` exists).
- `part_files.superseded_by REFERENCES part_files(id) ON DELETE SET NULL`; `model_revision_id … ON DELETE CASCADE`; `setup_revision_id … ON DELETE CASCADE`; `setup_revisions.model_revision_id … ON DELETE RESTRICT`.
- Role check constraint as the spec lists; `role IN ('source','render','project_3mf','slice','other')`.
- `CREATE UNIQUE INDEX part_files_one_3mf ON part_files (setup_revision_id) WHERE role='project_3mf'`.
- `CREATE UNIQUE INDEX part_files_one_slice ON part_files (setup_revision_id, target_family, round(target_nozzle::numeric,3), lower(coalesce(profile,''))) WHERE role='slice' AND superseded_by IS NULL AND target_family IS NOT NULL AND target_nozzle IS NOT NULL`.
- `CREATE INDEX part_files_sha256 ON part_files (sha256)`.

**Interfaces (Produces, in `blobs.rs`):**
- `pub struct Blobs` with `open(dir: &Path) -> Result<Blobs, StoreError>`, `temporary() -> Result<Blobs, StoreError>`, `path(&self, sha: &str) -> PathBuf`, `preview(&self, sha: &str) -> PathBuf`.
- `pub enum PreviewSource { Slice(LibraryKind), Image, ThreeMf, None }`.
- `pub struct Ingested { pub sha256: String, pub size: u64, pub parsed: Option<parse::Parsed>, pub has_preview: bool, temp: tempfile::NamedTempFile }`.
- `async fn receive<E>(&self, body, expected_len: u64, max: u64, source: PreviewSource) -> Result<Ingested, LibraryError>` — stream to `tmp/`, hash, size checks (phase 1 errors), then parse/extract preview: Slice → existing `parse_file`; Image → bounded decode (reuse phase 1 bounds from `parse.rs`), re-encode as PNG; ThreeMf → read `Metadata/thumbnail.png` from the zip if ≤ the phase 1 preview byte bound, decode-check it.
- `fn publish(&self, ingested: &Ingested) -> io::Result<()>` — the existing fsync/rename sequence; caller holds the digest advisory lock `(2, hashtext(sha))` in its transaction.
- `async fn cleanup_digest(&self, pool: &PgPool, sha: &str)` — refcount query becomes `SELECT (SELECT COUNT(*) FROM library_files WHERE sha256=$1) + (SELECT COUNT(*) FROM part_files WHERE sha256=$1)` (the `library_files` half is removed in Task 8).

- [ ] **Step 1: Failing tests:** postgres_store test `migration_v4_creates_parts_tables` (schema version is 4; inserting two parts `Clip`/`clip` fails with unique violation; a `part_files` row with role `slice` and null `setup_revision_id` fails the check). blobs.rs tests: `image_preview_is_png` (feed a small JPEG built with `image` in the test → `has_preview`, PNG magic at `preview(sha)` after publish); `threemf_thumbnail_extracted` (build a zip in the test with `Metadata/thumbnail.png`); `threemf_without_thumbnail_has_no_preview`.
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Implement; move code, don't duplicate it. Phase 1 `LibraryStore::add` uses `Blobs::receive` + `publish`.
- [ ] **Step 4:** Full Rust suite → PASS (existing library tests unchanged).
- [ ] **Step 5: Commit** `Add parts tables and split out the blob store`.

### Task 4: Parts catalog store (parts, model revisions, setups, setup revisions)

**Files:**
- Create: `crates/api/src/library/parts/mod.rs`, `crates/api/src/library/parts/catalog.rs`
- Modify: `crates/api/src/lib.rs` (`AppState.parts: Arc<PartsStore>`), `crates/api/src/main.rs` (construct with the same `Blobs`), test harness in `crates/api/tests/common` / `fake_prusalink` where `AppState` is built
- Test: `crates/api/tests/parts_catalog.rs` (store-level, real Postgres like other tests)

**Interfaces (Produces, `PartsStore { pool, blobs: Arc<Blobs> }`):**
- `create_part(name, notes: Option<&str>, by) -> Result<Part, PartsError>` — also inserts model revision 1.
- `update_part(id, PartPatch { name: Option<String>, notes: Option<Option<String>>, default_setup_id: Option<Option<i64>> }) -> Result<Part, PartsError>` — default setup must belong to the part.
- `new_model_revision(part_id, note, carry: &[i64], by) -> Result<ModelRevision, PartsError>` — carried ids must be files of the part's latest model revision; copies rows (same sha) into the new revision.
- `create_setup(part_id, name, by) -> Result<Setup, PartsError>` — also creates r1 on the latest model revision.
- `update_setup(id, name: Option<String>, default_profile: Option<Option<String>>)`.
- `new_setup_revision(setup_id, model_revision_id: Option<i64>, note, by) -> Result<SetupRevision, PartsError>` — model revision must belong to the same part; default latest.
- Deletes: `delete_part`, `delete_setup`, `delete_setup_revision`, `delete_model_revision` (RESTRICT → `PartsError::InUse(Vec<String>)` naming setups), each collecting affected SHA-256s, inserting them into `library_cleanup` in the same transaction, then `blobs.cleanup_digest` after commit (phase 1 pattern).
- `PartsError { NotFound(String), Conflict(String), InUse(String), BadRequest(String), Library(LibraryError), Store(StoreError) }` → `ApiError` (`NotFound`, `Conflict` 409, `InUse` → 409 with the setup names, `BadRequest`).
- Per-part lock: `pg_advisory_xact_lock(3, part_id::int)` (namespace 3, distinct from phase 1's 1 and 2) around every number assignment.

- [ ] **Step 1: Failing tests:** `create_part_has_model_rev_1`; `duplicate_part_name_case_insensitive_conflicts`; `new_setup_has_r1_on_latest_model`; `new_model_revision_numbers_increase_and_carry_files` (carry a source file; new revision lists it; same sha); `carry_rejects_foreign_file`; `delete_model_revision_in_use_names_setup` (error text contains the setup name); `delete_setup_clears_default_setup`; `revision_numbers_not_reused_after_delete` (delete r2, create → r3); `concurrent_setup_revisions_get_distinct_numbers` (two tasks via `tokio::join!`).
- [ ] **Step 2:** Run → FAIL. **Step 3:** Implement. **Step 4:** → PASS.
- [ ] **Step 5: Commit** `Add the parts catalog`.

### Task 5: Part files (upload, label, replace, delete)

**Files:**
- Create: `crates/api/src/library/parts/files.rs`
- Test: `crates/api/tests/parts_files.rs`

**Interfaces:**
- `pub enum FileLevel { Part, ModelRevision(i64), Setup(i64), SetupRevision(i64) }`
- `pub struct UploadSpec { pub part_id: i64, pub level: FileLevel, pub role: FileRole, pub name: String, pub profile: Option<String>, pub replace: bool }`
- `PartsStore::add_file<E>(&self, spec: UploadSpec, by: &str, body, len, max) -> Result<(PartFile, bool), PartsError>` (bool = created).
- `PartsStore::get_file(id) -> Option<PartFile>`, `update_file(id, profile: Option<Option<String>>, note: Option<Option<String>>)`, `delete_file(id)`.

Rules (each one is a test):
- Role/extension mismatch → BadRequest naming the accepted extensions.
- Level must fit role (spec's check-constraint list; `Other` allows `Part` or `ModelRevision`); level ids must belong to `part_id`, else NotFound.
- `Setup(id)` for slices → that setup's latest revision. `Setup(id)` for 3MF → latest revision if it has no 3MF, else a new revision (built from the latest revision's model revision — the 3MF is a placement change, not a geometry change) holding it.
- Slice target: family from `meta.printer_model`, nozzle from `meta.nozzle_diameter`, profile = `spec.profile` else `meta.print_settings_id`.
- Existing current slice with the same target: no `replace` → Conflict `"Full plate r3 already has a slice for MK4 · 0.4 · <profile>: <name> (id N)"`; `replace` → new row, old row `superseded_by = new.id`, same transaction.
- Identical bytes re-uploaded to the same level and role with the same name → return existing (`created=false`), phase 1 behavior.

- [ ] **Step 1: Failing tests** (use `prints/cube-12mm/` fixtures: `.step`, `.stl`, three `.bgcode`): `upload_source_and_render_to_model_revision`; `slice_target_read_from_file` (MK4 fixture → family Mk4, nozzle 0.4, profile `0.20mm SPEED @MK4IS 0.4`); `profile_override_on_upload`; `wrong_extension_for_role_rejected`; `slice_needs_setup_level`; `level_from_other_part_not_found`; `duplicate_target_conflicts_without_replace`; `replace_supersedes_old_slice`; `threemf_fills_empty_latest_revision_then_creates_next` (Review Focus 4); `shared_blob_survives_delete_in_other_part` (Review Focus 3); `delete_file_unsets_superseded_by`; `unparsed_slice_has_unknown_target_and_no_conflict` (two plain `.gcode` without metadata both accepted).
- [ ] **Step 2–4:** FAIL → implement → PASS.
- [ ] **Step 5: Commit** `Add uploads and edits for part files`.

### Task 6: Part tree, flags and matching

**Files:**
- Create: `crates/api/src/library/parts/read.rs`, `crates/api/src/library/parts/matching.rs`
- Test: `crates/api/tests/parts_match.rs`

**Interfaces:**
- `PartsStore::list(q: Option<&str>, sort: PartSort) -> Vec<PartSummary>` — `q` matches part name, file names, profile labels (ILIKE); preview per spec "Part preview" order.
- `PartsStore::tree(id) -> Option<Part>` — revisions newest first; superseded slices included (UI hides them); `Setup.behind_model` set when latest revision's model number < part's latest.
- `pub fn match_printers(part: &Part, setup_id: Option<i64>, profile: Option<&str>, printers: &[PrinterInfo]) -> Result<Vec<PrinterMatch>, PartsError>` where `PrinterInfo { name: String, kind: PrinterKind, nozzle: Option<f64> }`. Pure function over the tree (no DB), so phase 2 can reuse it.
- `pub fn check_override(part: &Part, slice_id: i64, printer: &PrinterInfo) -> Result<Vec<String>, String>` — Err = hardware block text; Ok = warnings (`"from Full plate r2; latest is r3"`, `"built from model rev 1; latest is rev 2"`, `"replaced"`, `"unknown target"`).

Matching algorithm = spec "Matching" steps 1–4, in order. Exact reason strings:
- No setup chosen and no default: `"choose a setup"`.
- Behind model: `"<setup> is on model rev <a>; latest is rev <b>"` (same text for every printer).
- No profile chosen, no default, and >1 distinct profile in latest revision: `"choose a profile"`.
- Printer nozzle unknown: `"printer nozzle unknown"`.
- No match: `"no slice for <MK4|MINI> · <nozzle> · <profile or 'no profile'> in <setup> r<n>"`.

- [ ] **Step 1: Failing tests** (pure, build `Part` values in the test): `matches_by_family_nozzle_profile`; `uses_default_setup_and_default_profile`; `single_profile_needs_no_choice`; `two_profiles_without_default_asks`; `behind_model_blocks_every_printer` (Review Focus 1); `older_revision_slices_not_matched`; `superseded_not_matched`; `unknown_target_not_matched`; `no_slice_reason_text`; `override_hardware_mismatch_blocks`; `override_old_revision_warns`. Plus one DB test `tree_flags_behind_model` and `list_search_finds_profile_label`.
- [ ] **Step 2–4:** FAIL → implement → PASS.
- [ ] **Step 5: Commit** `Add the part tree and slice matching`.

### Task 7: HTTP routes; remove the phase 1 catalog

**Files:**
- Create: `crates/api/src/routes/parts.rs`; Modify: `crates/api/src/routes/mod.rs`
- Delete: `crates/api/src/routes/library.rs`; phase 1 catalog methods in `crates/api/src/library/store.rs` and `library/mod.rs` (keep role constants renamed `READ_PARTS_ROLE`, `EDIT_PARTS_ROLE`, `DELETE_PARTS_ROLE`)
- Replace: `crates/api/tests/library.rs` → `crates/api/tests/parts_api.rs`; update `tests/roles.rs`, `tests/auth_roles.rs`
- Delete: `crates/api/src/mcp/tools/library.rs` and its registration in `mcp/tools/mod.rs` / `mcp/catalog.rs` (update `tests/mcp.rs`, `tests/mcp_tools.rs` to expect no `library_*` tools; `part_*` arrive in Task 10)
- Keep for now: `LibraryFile`, `LibraryQuery`, `LibrarySort` in `crates/shared/src/library.rs` (the CLI still uses them; Task 9 removes them)

Routes exactly as the spec's "API" section. Query params on upload: `role` (optional; inferred via `FileRole::infer`, 400 if neither), one of `model_revision|setup|setup_revision` (none = `Part` for `other`, latest model revision for source/render), `name` (required), `profile`, `replace`. `GET /api/parts/{id}/match?setup=&profile=` loads printers from `state.store` with each printer's nozzle from the existing info call; a printer whose info call fails gets `blocked: "printer unreachable"`.
Upload status: 201 created / 200 existing (phase 1).

- [ ] **Step 1: Failing tests** in parts_api.rs: full round trip (create part → upload STEP → create setup → upload MK4 bgcode with `setup=` → `GET /api/parts/{id}` tree shows slice in r1 with profile → preview and download headers equal phase 1's → `match` returns the slice for a fake MK4 printer). Role tests in roles.rs: Viewer cannot POST/PATCH, Operator cannot DELETE, Admin can. `/api/library` returns 404.
- [ ] **Step 2–4:** FAIL → implement → PASS (full Rust suite; the CLI's `library` tests talk to a mock server, so they keep passing until Task 9 replaces them).
- [ ] **Step 5: Commit** `Serve parts over HTTP and retire the flat library API`.

### Task 8: Migrate phase 1 rows and drop `library_files`

**Files:**
- Modify: `crates/api/src/store.rs` (extend v4 SQL — v4 has not been deployed, so editing it is allowed); `crates/api/src/library/blobs.rs` (refcount over `part_files` only)
- Test: `crates/api/tests/postgres_store.rs`

v4 SQL appended, in one migration transaction, per the spec's "Migration of phase 1 entries": part per distinct name (stem without `.bgcode`/`.gcode`; case-insensitive collisions get ` (2)`, ` (3)` by `MIN(uploaded_at)` order), model rev 1 note `migrated from library`, setup `Default` set as default, setup revision k per version k, slice row copying `sha256,size,kind,meta,parsed,has_preview,uploaded_by,uploaded_at`, name = phase 1 name, profile null, family/nozzle from `meta` (family via SQL `CASE WHEN upper(meta->>'printer_model') LIKE 'MK4%' THEN 'mk4' WHEN … 'MINI%' THEN 'mini' END`). Then `DO $$ … IF (SELECT count(*) FROM part_files WHERE role='slice') <> (SELECT count(*) FROM library_files) THEN RAISE EXCEPTION … $$`, then `DROP TABLE library_files`.

Test harness: the migration test must run v1–v3, insert phase 1 rows, then run v4 (use the existing `run_migrations(pool, &MIGRATIONS[..3])` then `&MIGRATIONS[..]`).

- [ ] **Step 1: Failing tests:** `v4_migrates_versions_to_setup_revisions` (`cube.bgcode` v1+v2 → part `cube`, setup `Default` r1+r2, default setup set); `v4_case_collision_suffixes` (Review Focus 5); `v4_empty_library_ok`; `v4_drops_library_files`.
- [ ] **Step 2–4:** FAIL → implement → PASS.
- [ ] **Step 5: Commit** `Move phase 1 library files into parts`.

### Task 9: CLI `part` commands

**Files:**
- Modify: `crates/cli/src/args.rs` (replace `LibraryCommand` with `PartCommand`), `crates/cli/src/commands/mod.rs`, `crates/cli/src/client.rs` if new verbs (PATCH) are missing
- Replace: `crates/cli/src/commands/library.rs` → `part.rs`; `crates/cli/tests/cli_library.rs` → `cli_part.rs`

Commands per the spec's CLI section: `part ls [--search] [--sort newest|name]`, `part new <name> [--notes]`, `part info <part>` (tree: model revs with files; each setup with revisions, slices table `ID FAMILY NOZZLE PROFILE FILAMENT TIME`, flags), `part rm <part>` (needs `--yes` without a terminal, existing `ConfirmArgs`), `part rev <part> [--note] [--carry <file-id>…]`, `part setup <part> <name>`, `part upload <part> <file> [--role] [--setup <name>] [--setup-rev <n>] [--model-rev <n>] [--profile] [--replace] [--name]`, `part download <file-id> [-o]`. `<part>` accepts an ID or an exact name (resolve via `GET /api/parts?q=` exact case-insensitive match; ambiguous → error). `--setup`/`--setup-rev`/`--model-rev` take names/numbers; the CLI resolves them to IDs from `part info`. `--json` output via existing `ctx.out.emit_value`.

- [ ] **Step 1: Failing tests** in cli_part.rs (existing test pattern against a mock server): `ls_prints_table`, `upload_slice_resolves_setup_name`, `upload_infers_role`, `upload_3mf_without_setup_errors` (`.3mf and slices need --setup`), `rm_needs_yes_without_terminal`, `part_by_name_ambiguous_errors`.
- [ ] **Step 2–4:** FAIL → implement → PASS.
- Also remove `LibraryFile`, `LibraryQuery`, `LibrarySort` from `crates/shared/src/library.rs` (keep `LibraryKind`).
- [ ] **Step 5: Commit** `Replace library commands with part commands`.

### Task 10: MCP tools and agent skill

**Files:**
- Replace: `crates/api/src/mcp/tools/library.rs` → `parts.rs`; Modify `crates/api/src/mcp/tools/mod.rs`, `crates/api/src/mcp/catalog.rs`
- Modify: `crates/api/tests/mcp.rs`, `crates/api/tests/mcp_tools.rs`, `skills/starstar/SKILL.md`

Tools: `part_list { query?, sort? }` (Viewer), `part_info { id }` (Viewer, the tree), `part_match { id, setup_id?, profile? }` (Operator). All `read_only_hint = true`. SKILL.md: a "Parts" section explaining part → model revisions → setups → setup revisions → slices, the matching rule including the behind-model block, and that uploads are CLI/web only.

- [ ] **Step 1: Failing tests:** catalog lists the three tools and not `library_*`; `part_match` as Viewer → insufficient role; `part_info` returns the tree.
- [ ] **Step 2–4:** FAIL → implement → PASS.
- [ ] **Step 5: Commit** `Expose parts through MCP`.

### Task 11: Web — types, paths, parts list

**Files:**
- Modify: `web/app/types/api.ts` (replace `LibraryFile` with TS mirrors of Task 2 types), `web/app/utils/paths.ts` (replace `libraryPath` with `partsPath(id?, suffix?: 'revisions'|'setups'|'files'|'match')`, `setupsPath(id, suffix?: 'revisions')`, `filesPath(id, suffix?: 'preview'|'download')`), `web/app/pages/library/index.vue`
- Test: `web/test/library.test.ts`, `web/test/paths.test.ts`

List page per spec "Web UI" (columns: preview, name, model rev, setups, slices, last change; search; sort Newest/Name; **New part** for operators opens an inline name field → `POST /api/parts` → navigate to `/library/{id}`). Drop the Upload button from the list.

- [ ] **Step 1: Failing tests:** lists parts and searches (`/api/parts?q=cub`); no New part button for viewers; New part posts `{ name }` and navigates; paths tests for the new helpers.
- [ ] **Step 2–4:** FAIL → implement → PASS (`npm --prefix web run typecheck && npm --prefix web test`).
- [ ] **Step 5: Commit** `Show parts on the library page`.

### Task 12: Web — part page

**Files:**
- Rewrite: `web/app/pages/library/[id].vue`; Create components `web/app/components/PartFiles.vue` (file list with role, size, download, admin delete), `web/app/components/SetupCard.vue` (revision history, 3MF, slices table with flags, upload buttons, defaults)
- Reuse: `PrintFacts.vue`, `ConfirmDialog.vue`, `Notice.vue`
- Test: replace `web/test/library-file.test.ts` with `web/test/part-page.test.ts`

Behavior per spec "Web UI → Part page". Uploads use the upload query params from Task 7. **New revision** dialog lists the latest model revision's files as checkboxes (carry), and shows the copy: "Setups built from rev <n> will stop matching automatically until they get a new revision built from rev <n+1>." Superseded slices hidden behind a "Show replaced" toggle. Delete confirmations list what goes with it (counts of revisions and files from the tree).

- [ ] **Step 1: Failing tests:** renders model revision files and setup slices with profile; flag "on model rev 1; latest is rev 2" shown on a behind setup; viewer sees no upload/delete; operator uploads a slice to a setup (`POST /api/parts/7/files?role=slice&setup=3&name=…`); 409 from upload shows the server message and a **Replace** button that retries with `replace=true`; New revision posts selected `carry` ids and shows the warning copy; admin delete of a setup shows the confirmation.
- [ ] **Step 2–4:** FAIL → implement → PASS.
- [ ] **Step 5: Commit** `Add the part page`.

### Task 13: Deploy to Thor

**Files:** `deploy/thor/kustomization.yaml` (image digest), `deploy/thor/README.md` (record the v4 deploy and the pre-deploy snapshot steps)

- [ ] **Step 1:** Full verification suite green; container smoke (`scripts/container-smoke.sh`).
- [ ] **Step 2:** **Ask the user before `git push`** (image publishing runs from CI on push). Wait for the publish run; take the digest.
- [ ] **Step 3:** Pre-deploy snapshot: `kubectl --context thor -n starstar exec starstar-pg-1 -c postgres -- pg_dump -Fc -U postgres starstar > <off-pod path>` and a copy of the library directory; record paths (not contents) in README.
- [ ] **Step 4:** `deploy/thor/preflight.sh thor <verified-rollback-image>` → `PREFLIGHT OK`; `kubectl --context thor apply -k deploy/thor`.
- [ ] **Step 5:** Verify: `/api/health` OK; `SELECT max(version) FROM schema_migrations` = 4; round trip via CLI against Thor: `part new smoke-test`, `part setup smoke-test Single`, upload `prints/cube-12mm/cube-12mm.MK4.bgcode` with `--setup Single`, `part info`, `part rm smoke-test --yes`.
- [ ] **Step 6: Commit** `Deploy the parts library to Thor` with the publish/CI run IDs in the body.

---

## Notes for the executor

- MK4S slices carry `0.20mm SPEED @MK4S HF0.4` (a high-flow nozzle). The spec matches on family + diameter only, so HF and standard 0.4 nozzles match the same printers. Do not add HF handling; mention it in the final report as a follow-up.
- Phase 2 (farm batches) is out of scope. Its design is to be rewritten on top of `match_printers` / `check_override` afterwards.
