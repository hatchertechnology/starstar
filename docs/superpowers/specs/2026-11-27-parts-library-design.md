# Parts library: one ID per part, all its artifacts attached

Date: 2026-11-27. Size: project. Changes phase 1 of
[the print library design](2026-11-27-print-library-design.md) and the shape
phase 2 (farm batches) builds on.

## Intent

Phase 1 made the library a flat list of sliced files (`.bgcode`, `.gcode`)
keyed by name and version. The user wants the library to be an archive of
**parts**. A part has one ID, and every artifact for it hangs off that ID:
source models (STEP/STL), slicer projects (3MF), rendered images, and many
G-code slices for different printers, nozzles and print profiles.

Why:

- **Organization.** The source files and 3MF are not kept as a backup. They
  are kept so everything about a part is in one place and linked, and so
  starstar can act on them later (slicing from the 3MF is the expected next
  step).
- **Printing a part, not a file.** On the farm the user picks a part and
  printers. Starstar picks the right slice for each printer (model, nozzle,
  profile). The user can override the choice for any printer.
- **No stale prints.** When the design or the plate layout changes, starstar
  must know which slices are out of date and not pick them on its own.

## The model

```
Part  "Cable clip"  (one ID, name, notes, default setup)
├─ Model revisions    rev 1, rev 2 …       ← geometry: STEP/STL, renders
├─ Setups             "Single", "Full plate" … (named, each a 3MF lineage)
│   └─ Setup revisions  r1, r2 …  (each records the model revision it was built from)
│       ├─ 3MF (zero or one)
│       └─ Slices  one per target: printer family · nozzle · profile
└─ Other files        drawings, PDFs, notes (attached to the part or a model revision)
```

- **Model revision**: a change to the object itself. Holds source files and
  renders. Created only by an explicit **New revision** action, never
  automatically. The action lists the previous revision's files with
  checkboxes to carry over (blobs are content-addressed, so carrying over
  costs nothing).
- **Setup**: a named way to print the part (one copy, a full bed). A part has
  any number of setups. A setup lives across model revisions.
- **Setup revision**: a change to the layout: orientation, bed position,
  supports. Each records which model revision it was built from (default:
  the latest). Uploading a 3MF to a setup creates a new setup revision with
  that 3MF. A setup revision can also be created without a 3MF (needed for
  migrated files and for slice-only parts).
- **Slice**: a G-code file in one setup revision. Its **target** is printer
  family + nozzle + profile. Nozzle, printer and profile are not setup
  revisions: one setup revision has one slice per target.

## Goals

- Parts with model revisions, named setups with revisions, slices per target,
  renders, source files, 3MF and other files, all under one part ID.
- Slice targets read from the file. The profile label defaults to
  PrusaSlicer's `print_settings_id` and can be edited.
- Out-of-date flags: a setup whose latest revision is behind the latest model
  revision; a slice that is not in its setup's latest revision.
- A matching function phase 2 calls: (part, setup, profile, printer) → slice,
  or the reason there is none.
- Web UI, CLI and MCP get the same capabilities, as in phase 1.
- Existing phase 1 library entries are migrated into parts without loss.

## Non-goals (now)

- Starstar slicing from the 3MF. Later. The data model must allow it to write
  slice rows into a setup revision with no schema change.
- Parsing STEP, STL or 3MF contents (beyond the 3MF thumbnail, see below), or
  rendering STL to an image. Renders are uploaded images.
- Checking that a slice really came from the setup revision's 3MF. The user
  says where it belongs.
- Tags, folders or part categories. Search and sort only, as in phase 1.
- Farm batch execution itself. That is phase 2, redesigned on top of this.

## Anti-goals

- Starstar choosing a slice from an older setup revision or an older model
  revision on its own. Only a manual override can do that, with a visible
  warning.
- An artifact that belongs to no part, or a slice that belongs to no setup
  revision. The database enforces the hierarchy.
- Losing or silently changing any phase 1 library file in the migration.

## Constraints

- Everything in the phase 1 design still holds: files on the `zfs-local`
  volume stored by SHA-256, catalog in Postgres, single API replica, Thor via
  `--context thor`, no ArgoCD, UID 10001 with a read-only root filesystem,
  the `STARSTAR_MAX_UPLOAD_MB` size limit, the nightly backup plan.
- Roles as in phase 1: Viewers read and download; Operators upload, create
  revisions and setups, and edit labels and defaults; Admins delete.
- Phase 1 is live on Thor with schema v3. This change is migration v4.

## Approaches

1. **A table for each layer (chosen).** `parts`, `model_revisions`, `setups`,
   `setup_revisions`, and one `part_files` table for all artifacts with a role
   and a foreign key to the level it belongs to. The database enforces the
   hierarchy and revision numbering; matching and staleness are plain
   queries; starstar slicing later only adds rows. Cost: a data migration and
   a new API, UI and CLI shape.
2. **Parts plus tagged files.** One `parts` table and free-form JSON tags on
   files. Smaller migration, but the hierarchy lives only in code, nothing
   stops a dangling setup revision, and matching becomes JSON queries.
   Rejected: works against "organized".
3. **A folder per part on disk, indexed by starstar.** Rejected: the phase 1
   design rules out paths that bypass starstar, and keeping disk and database
   in step is a constant bug source.

The user chose 1.

## Design

### Database (migration v4)

- `parts`: `id` (bigint identity), `name` (unique, case-insensitive), `notes`
  (text), `default_setup_id` (nullable FK), `created_by`, `created_at`.
- `model_revisions`: `id`, `part_id` (FK, cascade), `number` (int, 1, 2, …
  per part, unique with `part_id`), `note`, `created_by`, `created_at`.
- `setups`: `id`, `part_id` (FK, cascade), `name` (unique per part,
  case-insensitive), `default_profile` (nullable text), `created_by`,
  `created_at`.
- `setup_revisions`: `id`, `setup_id` (FK, cascade), `number` (unique with
  `setup_id`), `model_revision_id` (FK, restrict: a model revision that setup
  revisions are built from cannot be deleted while they exist), `note`,
  `created_by`, `created_at`.
- `part_files`: `id`, `part_id` (FK, cascade; always set, for listing and
  search), `model_revision_id` (nullable), `setup_revision_id` (nullable),
  `role`, `name` (the uploaded file name), `sha256`, `size`, `kind`
  (extension class), `meta` (jsonb, `PrintMeta` for slices), `parsed`,
  `has_preview`, `uploaded_by`, `uploaded_at`. Slice columns:
  `target_family`, `target_nozzle`, `profile` (nullable for unparsed slices),
  `superseded_by` (nullable FK to `part_files`).
- Check constraints per role:
  - `source`, `render`: `model_revision_id` set, `setup_revision_id` null.
  - `project_3mf`, `slice`: `setup_revision_id` set.
  - `other`: either the part only or a model revision.
- At most one current 3MF per setup revision (partial unique index).
- At most one current slice per (setup revision, family, nozzle, profile)
  where `superseded_by` is null (partial unique index).
- Revision numbers are assigned under a per-part advisory lock (the same
  pattern as phase 1's per-name lock).
- `library_cleanup` and blob refcounting move to count `part_files.sha256`.
  `library_files` is dropped at the end of the migration.

### File roles and accepted types

| Role | Extensions | Parsed |
|---|---|---|
| `source` | `.step`, `.stp`, `.stl`, `.f3d`, `.scad`, `.py` (CadQuery) | no |
| `render` | `.png`, `.jpg`, `.jpeg`, `.webp` | dimensions checked with the phase 1 bounded decoder; served as an image |
| `project_3mf` | `.3mf` | the embedded `Metadata/thumbnail.png` becomes its preview, if present and within the decoder bounds |
| `slice` | `.bgcode`, `.gcode` | yes, the phase 1 parser, plus `print_settings_id` |
| `other` | any | no; always served as a download (`Content-Disposition: attachment`, `nosniff`) |

The upload request names the role and the level. The server rejects an
extension that does not fit the role.

### Slice targets and profiles

- **Family** from `printer_model` by the phase 1 rule (`MK4…` → MK4 family,
  `MINI…` → MINI). Keep the mapping in one function; new families are added
  there.
- **Nozzle** from `nozzle_diameter`, compared with ±0.001.
- **Profile**: on upload, default to `print_settings_id` (for example
  `0.20mm SPEED @MK4IS 0.4`). The uploader can give a shorter label instead
  (`speed`). Operators can rename a slice's profile later. Matching compares
  profile labels exactly (case-insensitive, trimmed).
- An unparsed slice has an unknown target. It is never matched
  automatically; it can be chosen by override, with a warning (same rule as
  phase 1: unknown is a warning, not a block).
- Uploading a slice whose target already has a current slice in that setup
  revision fails with a conflict that names the existing slice, unless the
  request says `replace`. Replace keeps the old file and sets its
  `superseded_by`. Superseded slices are hidden by default and excluded from
  matching; Admins can delete them.

### Matching (used by phase 2)

Input: part, setup (default: the part's default setup), profile (default: the
setup's default profile), printer.

1. Take the setup's latest revision.
2. If that revision was built from an older model revision than the part's
   latest, stop: "Full plate is on model rev 1; latest is rev 2". No printer
   can be ticked automatically for this setup (decided by the user: block,
   not warn).
3. Take its current slices with the printer's family and nozzle and the
   profile.
4. Exactly one → that slice. None → "no slice for MK4 · 0.6 · speed in
   Full plate r3". The printer cannot be ticked.

The **New revision** dialog says that every setup will stop matching
automatically until it gets a new setup revision built from the new model
revision.

The partial unique index guarantees there is never more than one. A setup
with no default profile and more than one profile in its latest revision
asks the user to choose a profile.

**Override**: the user can pick any slice of the part for a printer. Family
and nozzle must still match (a hardware block, as in phase 1). A slice from
an older setup revision, or a setup revision behind the latest model
revision, shows a warning: "from Full plate r2; latest is r3" or "built from
model rev 1; latest is rev 2".

Phase 2's batch items store the chosen slice's `part_files.id`. The phase 2
design is updated before phase 2 is built. Its open item about the printer's
ready state is unchanged.

### Out-of-date flags

- Setup: its latest revision's model revision number is lower than the
  part's latest model revision number → "on model rev 1; latest is rev 2".
- Slice: not in its setup's latest revision → "from r2". Superseded →
  "replaced".
- Flags are computed on read. Nothing is stored.

### Part preview

The part list and the part page header use, in order: the newest render of
the latest model revision, else the latest default-setup revision's 3MF
thumbnail, else the preview of any slice in that revision. If there is none,
show a placeholder.

### Migration of phase 1 entries

Checked on 2026-11-27: the live Thor library has **0 rows** in `library_files`
and `library_cleanup`. The migration therefore stays a plain mapping, with no
grouping rules or dry-run report. It still has to be correct for local and
Compose databases that do have rows. For each distinct `library_files.name`:

- A part with that name; model revision 1 with no files and the note
  "migrated from library".
- One setup named `Default`, set as the part's default setup.
- For each phase 1 version k: setup revision k (built from model rev 1),
  holding that file as a slice, with the same `id`-independent blob, `meta`,
  `parsed`, `has_preview`, `uploaded_by`, `uploaded_at`. The profile is null
  (`print_settings_id` was not stored); the slice is matched on family and
  nozzle with "no profile" as its profile. Operators can label it later.
- The migration runs in one transaction and checks that the count of slice
  rows equals the count of `library_files` rows before dropping the table.
- Blobs and thumbnails on disk do not move.
- Part name: the phase 1 name without its `.bgcode`/`.gcode` extension. If
  two phase 1 names collide case-insensitively, the later ones get ` (2)`,
  ` (3)` … appended, so the transaction cannot fail on the unique index.
  (Builder's call; the user can rename afterwards.)
- Different targets stored as versions of one name are not regrouped. With
  no live rows, this edge case is not worth the rules.
- Before deploying v4 to Thor: a manual `pg_dump -Fc` of `starstar` and a
  copy of the library directory, kept off the pod. The nightly backup job
  (phase 3) is not a prerequisite. (Builder's call: the live library is
  empty, so the risk is small.)

### Adding to setups

- A new setup gets **r1** at once, built from the latest model revision, so
  a slice can be uploaded straight away.
- `setup=<id>` on a slice upload means that setup's latest revision.
  `setup_revision=<id>` targets a specific revision, including an older one
  (for backfilling a target that was never uploaded). Slices added to an
  older revision are still flagged "from r2".
- A `project_3mf` upload with `setup=<id>` creates the next revision. If the
  latest revision has no 3MF yet (for example r1 of a new setup), the 3MF
  goes into that revision instead of creating a new one.
- CLI: `--setup <name>` means the latest revision; `--setup-rev <n>` picks
  one; `--model-rev <n>` picks a model revision for source, render and other
  files (default: latest).

### Deleting

Admin only. Each delete confirms in the UI with the list of what goes with
it.
- **Part**: everything under it.
- **Setup**: its revisions and their files. The part's `default_setup_id`
  is set to null.
- **Setup revision**: its 3MF and slices. Revision numbers are never reused.
- **Model revision**: its source, render and other files. Refused while any
  setup revision is built from it (FK restrict), with a message naming those
  setups.
- **File**: if other slices have `superseded_by` pointing at it, that is
  set to null.
- Blob cleanup: when no `part_files` row references a SHA-256 any more, use
  phase 1's `library_cleanup` path.
- Phase 2 batch items keep a copy of the slice's name, SHA-256 and target,
  so a deleted slice leaves readable history.

### API (behind the existing auth; replaces `/api/library`)

- `GET /api/parts` (Viewer): list with search `q` (part name, file names,
  profile labels) and sort (newest, name).
- `POST /api/parts` (Operator): `{ name, notes? }` → part with model rev 1
  and no setups.
- `GET /api/parts/{id}` (Viewer): the whole tree with flags.
- `PATCH /api/parts/{id}` (Operator): name, notes, default setup.
- `DELETE /api/parts/{id}` (Admin).
- `POST /api/parts/{id}/revisions` (Operator): `{ note?, carry: [file ids] }`.
- `POST /api/parts/{id}/setups` (Operator): `{ name }`. `PATCH` and `DELETE`
  (Admin) on `/api/setups/{id}`.
- `POST /api/setups/{id}/revisions` (Operator): `{ model_revision?, note? }`.
- `POST /api/parts/{id}/files?role=…&model_revision=…|setup_revision=…&name=…[&profile=…][&replace=true]`
  (Operator): streaming upload with `Content-Length`, the same limits as
  phase 1. A `project_3mf` upload to a setup (`setup=…` instead of
  `setup_revision`) creates the next setup revision.
- `GET /api/files/{id}` (Viewer), `/preview`, `/download`. The same headers
  as the phase 1 preview and download routes.
- `PATCH /api/files/{id}` (Operator): profile label, note. `DELETE` (Admin).
- `GET /api/parts/{id}/match?setup=…&profile=…` (Operator): per printer, the
  matched slice or the reason. Phase 2 uses this for its selection list.

### Web UI

- **Library** page lists parts: preview, name, latest model rev, setups, the
  number of slices, last change. Search, sort, **New part**.
- **Part page**:
  - Header: preview, name, notes, model revision picker (latest by default).
  - Model revision section: source files, renders (gallery), other files,
    **New revision**.
  - Setups section: one card per setup with its revision history, the 3MF,
    and a slice table (family, nozzle, profile, filament, time, uploaded),
    with out-of-date flags. Buttons: **Upload 3MF** (new setup revision),
    **Upload slice**, **New setup**, set default setup or profile.
  - Deletes for Admins, with a confirmation that lists what goes with it.
- **Print** stays on the part page, but is built in phase 2.

### CLI and MCP

- CLI replaces `starstar library …` with `starstar part ls|new|info|rm`,
  `starstar part rev <part>`, `starstar part setup <part> <name>`,
  `starstar part upload <part> <file> [--role …] [--setup …] [--setup-rev …]
  [--model-rev …] [--profile …] [--replace]`, `starstar part download <file-id>`. The role
  is inferred from the extension when not given; `.3mf` and slices need
  `--setup`.
- MCP replaces `library_list` and `library_info` with `part_list`,
  `part_info` (the tree with flags) and `part_match`. Uploads stay CLI and
  web only, as in phase 1.
- `skills/starstar/SKILL.md` documents parts, revisions, setups and the
  matching rule.

## Delivery order

1. Migration v4, store, API, and the phase 1 parser gaining
   `print_settings_id`. Tests for the migration with phase 1 data.
2. Web UI, CLI, MCP and skill.
3. Deploy to Thor; verify the migrated entries.
4. Then update the phase 2 design to print parts via matching.

## Left to the builder

- Page layout and copy inside the existing design tokens.
- How the part tree is split across API responses if one response gets too
  large.
- Pagination size.
- Exact CLI flag spellings beyond the ones above.
