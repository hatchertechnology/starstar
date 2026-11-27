# Print library and farm batches

Date: 2026-11-27. Size: project.

## Intent

The user runs a print farm with several identical Prusa MK4 printers. Today a print file goes straight to one printer, and starstar keeps nothing. The user wants starstar to be the home of print files:

1. Upload a sliced file once into the starstar library. Nothing goes to a printer.
2. Open the part's details page (preview, slicer details), click **Print**, tick several compatible printers, click **Send to printer**. Starstar copies the file to each printer that does not already have it and starts the prints.

The same file is printed on many printers without uploading it again. The library survives printers' USB sticks being swapped or wiped.

## Goals

- A server-side library of sliced print files (`.bgcode`, `.gcode`) with versions, preview images and slicer details read from the file itself, not from printer firmware.
- A two-step flow: upload, then print a batch on chosen printers from the part's details page.
- A compatibility list for the batch: printer model family, nozzle diameter, recorded filament, and ready state.
- A recorded "loaded filament" per printer, set by operators.
- Per-printer progress and errors for a batch. One printer failing does not stop the others.
- Web UI, CLI and MCP all get the same capabilities.
- Off-node backups of the database and the library.

## Non-goals (now)

- Slicing per printer (store 3D models and slice for each printer). Later.
- Folders or tags in the library. A flat list with search and sort first.
- Cleaning printer storage after prints. Copies stay on the printer.
- Exporting the library over NFS or any path that bypasses starstar.
- Reading the loaded filament from the printer. PrusaLink on MK4 firmware 8.0.2 does not report it (checked: `/api/v1/status` and `/api/v1/info` have no filament field).

## Anti-goals

- Starting a print on a printer whose bed may not be clear. Only printers that report the ready state can be ticked.
- Library files whose only copy is on Thor's disk. Backups are part of this project.
- A batch that silently skips a printer. Every chosen printer ends with a visible result.

## Constraints

- Single API replica (in-memory state; `strategy: Recreate`), Thor k3s, `--context thor` always, no ArgoCD.
- Files can be large (up to `STARSTAR_MAX_UPLOAD_MB`, default 512).
- PrusaLink facts from mk4-2 (firmware 8.0.2): file info has `ro` (not `read_only`), `refs.{icon,thumbnail,download}`, and **no `meta`**. Status states seen: `FINISHED`, `IDLE`. Info has `nozzle_diameter`, `mmu`, `serial`, `hostname`, `min_extrusion_temp`, no printer model string.
- Roles: operators upload and send batches; admins delete library files (matches today's printer-file roles).
- The API container runs as UID/GID 10001 with a read-only root filesystem.

## Approaches (storage)

1. **Files on a disk volume, catalog in Postgres (chosen).** Bytes on a `zfs-local` Retain volume at `/hatch1/services/starstar/library` on Thor, stored by SHA-256. Catalog, versions, metadata, filament records and batches in Postgres. Fast streaming, local prints, dedup and the "already on printer" check come from the hash. Cost: tied to Thor's disk; needs a backup job.
2. Files inside Postgres. Rejected: large blobs bloat the database, slow dumps and restores, and stream badly through the driver.
3. Object storage (B2 direct or MinIO on Thor). Rejected: B2 makes every batch depend on the internet uplink; MinIO is another service to run and still needs backups.

The user chose 1, on the `/hatch1` ZFS pool, using the existing `zfs-local` convention (no NFS).

## Design

### Storage layout

- Volume mount in the API pod: `/var/lib/starstar/library` (env `STARSTAR_LIBRARY_DIR`). Compose: a named volume.
- Blobs: `blobs/<sha[0..2]>/<sha[2..4]>/<sha>`. Write to `tmp/<uuid>` while hashing the stream, `fsync`, then rename into place. Identical content shares one blob.
- Preview: on upload, extract the thumbnails. Store the best one for browsers as `thumbs/<sha>.png`: the largest PNG, or the largest QOI converted to PNG. QOI is not displayable by browsers.
- Deleting the last version that references a blob deletes the blob and its thumbnail.

### Metadata extraction

- `.bgcode`: parse the block format (file, printer, print and slicer metadata blocks; thumbnail blocks; CRC32 per block). Read `printer_model`, `nozzle_diameter`, `filament_type`, `filament used [g]`, `filament used [mm]`, `estimated printing time (normal mode)`, temperatures and `layer_height`. Use the same field mapping as `PrintMeta`.
- `.gcode` (ASCII): read the PrusaSlicer comment block (`; key = value`) at the end of the file, and base64 thumbnails in the `; thumbnail begin … end` comments.
- Parsing failure does not reject the upload. The file is stored with empty metadata and a flag that says so.

### Database (new migration v3)

- `library_files`: `id` (uuid), `name` (display name, unique together with `version`), `version` (int, starts at 1, increments per name), `sha256`, `size`, `kind` (bgcode or gcode), `meta` (jsonb, the `PrintMeta` shape), `has_preview`, `uploaded_by`, `uploaded_at`.
- `printer_filament`: `printer` (FK to printers, on delete cascade), `filament_type` (text, upper case), `set_by`, `set_at`.
- `print_batches`: `id`, `file_id`, `created_by`, `created_at`.
- `print_batch_items`: `batch_id`, `printer`, `state` (`queued`, `copying`, `starting`, `printing`, `skipped_copy`, `failed`), `error` (text), `updated_at`.

### Copying to a printer and "already there"

- Name on the printer: `<stem>-<sha[0..8]>.<ext>` in the storage root, for example `cube-12mm.MK4-1a2b3c4d.bgcode`. If that name exists with the same size, the copy is skipped. The hash in the name makes a stale file with the same display name not match.
- Copy with the existing upload code path, then start with `POST` on the file. Do **not** use `Print-After-Upload`, so starstar re-checks the ready state between copy and start.

### Batch execution

- `POST /api/library/{id}/print` with a list of printers creates a batch and returns it at once. A background task per batch runs items with at most 4 copies in parallel.
- Before each item's copy and again before its start: the printer must be reachable, report the ready state, and match the model family and nozzle. Otherwise the item fails with a clear reason.
- The filament check is a warning only. It shows in the selection list. It never fails an item.
- Item state is saved to the database at every step. On API start, items left in `queued`, `copying` or `starting` become `failed` with "interrupted by a restart".

### Compatibility rules (selection list)

- Model family: the file's `printer_model` prefix `MK4` or `MINI` against the printer `kind`. A mismatch means the printer cannot be ticked.
- Nozzle: the file's `nozzle_diameter` against the printer's `nozzle_diameter` (±0.001). A mismatch means the printer cannot be ticked.
- Ready: only printers whose state is the ready state can be ticked. Others show the state as the reason.
- Filament: the file's `filament_type` against the printer's recorded filament. A mismatch or no record is a warning, shown as `PLA · set 2 d ago` or `not set`.
- An unknown value in the file (no metadata) gives a warning, not a block, for model and nozzle.

### API (new, behind the existing auth)

- `GET /api/library` (Viewer): list, with search `q` and sort.
- `POST /api/library` (Operator): streaming upload, the same size limit as printer uploads. Returns the new version.
- `GET /api/library/{id}` (Viewer), `GET /api/library/{id}/preview` (Viewer; PNG; the same headers as the thumbnail route: `nosniff`, sandbox CSP, `private, max-age=300`), `GET /api/library/{id}/download` (Viewer).
- `DELETE /api/library/{id}` (Admin).
- `GET /api/library/{id}/printers` (Operator): the compatibility list.
- `POST /api/library/{id}/print` (Operator): body `{ "printers": [...] }` → batch.
- `GET /api/batches/{id}` (Viewer): batch with item states; `GET /api/batches` (Viewer): recent batches.
- `PUT /api/printers/{name}/filament` (Operator): body `{ "filament_type": "PLA" }`; `GET` returns it with `set_at`, `set_by`.

### Web UI

- New **Library** page in the top navigation: list (name, version, sliced for, filament, time, uploaded), search and sort, Upload button.
- Library part details page: the same layout as the printer file details page (preview, details, mismatch notes), plus versions, Download, Delete (Admin) and **Print**.
- Print opens the selection list (checkboxes, reason for each printer that cannot be ticked, filament warning), then **Send to printer (N)**. A confirmation lists the chosen printers and says "Make sure each bed is clear." After sending, the page shows the batch with live per-printer state and errors, and Retry on failed items.
- Printer page: loaded filament with **Set** (Operator).

### CLI and MCP

- CLI: `starstar library ls|upload|info|rm|download|print <id> <printer>...`, `starstar batch show <id>`, `starstar printer filament <printer> [TYPE]`.
- MCP: `library_list`, `library_info`, `library_print`, `batch_status`, `set_printer_filament`, each with the same roles. The agent skill (`skills/starstar`) documents the flow and the bed-clear rule.

### Deployment on Thor

- A hand-made PV `starstar-library` (`zfs-local`, Retain, `local.path: /hatch1/services/starstar/library`, node affinity `thor`), a PVC bound to it with `volumeName`, mounted in the API pod. Start at 100Gi.
- Host preparation (one time, needs root on Thor): create the directory and give ownership to 10001:10001. The runbook records the command.
- The NetworkPolicy does not change for the library. The backup job (below) needs egress to B2 on 443.

### Backups (deferred item, now in scope)

- A nightly CronJob (openbooks pattern): `pg_dump -Fc` of `starstar-pg`, plus the library directory, with restic to B2. It uses a SopsSecret `starstar-backup-b2` (`RESTIC_REPOSITORY`, `RESTIC_PASSWORD`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`). The user supplies the B2 bucket and key.
- A documented and once-executed restore test into a scratch namespace.
- The dump contains printer passwords in plaintext (existing issue). The restic password protects the off-site copy.

## Delivery order

1. Library store: storage, migration, upload and parse, list, details with preview, download, delete. Web, CLI and MCP. Volume on Thor.
2. Filament records and farm batches: compatibility list, batch execution and progress, Retry. Web, CLI and MCP.
3. Backups: CronJob, SopsSecret, restore test.

Each phase is deployable on its own.

## Left to the builder

- Exact page layout and copy inside the existing design tokens.
- The batch progress transport (polling every few seconds is enough; no websockets needed).
- Library list pagination size.
- Which PrusaLink state string means "ready". **Verify on a real MK4 before phase 2.** Set mk4-2 ready on its screen, then read `/api/v1/status` (the controller can do this with `sops exec-env`). If firmware 8.0.2 has no distinct ready state, stop and bring it back to the user. Do not fall back to `IDLE` without their decision.
