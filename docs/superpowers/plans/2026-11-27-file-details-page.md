# File Details Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clicking a print file in the web UI opens a page with its preview thumbnail, slicer details, and Print/Delete actions.

**Architecture:** The API passes PrusaLink's per-file thumbnail link through as `has_thumbnail` and serves the image from a new Viewer route that fetches it from the printer. The Nuxt UI gets a new page under `/printers/[name]/files/[storage]/[...path]`, and the file browser links print files to it and can reopen a folder from the query string.

**Tech Stack:** Rust (axum, reqwest, serde), Nuxt 4 / Vue 3 (static, `ssr: false`), vitest with `@nuxt/test-utils`.

**Spec:** `docs/superpowers/specs/2026-11-27-file-details-page-design.md`

## Global Constraints

- Thumbnail route: `GET /api/printers/{name}/thumbnails/{storage}/{*path}`, role Viewer (`FILE_INFO_ROLE`).
- Thumbnail response header: `Cache-Control: private, max-age=300`; content type from the printer (`image/*`, fallback `image/png` via the existing `image()` helper).
- No thumbnail → 404. The upstream link must be a path on the same printer (starts with `/`, no scheme, host, query or `..`); anything else → 404 without contacting the printer.
- `FileEntry.has_thumbnail: bool`, serde default `false`, always serialised (so the UI can rely on it).
- The browser never talks to the printer. No CLI or MCP changes.
- Print requires Operator, Delete requires Admin; dialogs reuse the exact texts in `FileBrowser.vue` (`Start printing {name} on {printer}?` / `Make sure the bed is clear.`; `Delete {name}?` / `This removes it from {storage} on {printer}. This can't be undone.`).
- Mismatch warning (controller's call, adjusted): show it when the file's `printer_model` family differs from the printer `kind` (`MK4*` ↔ `mk4`, `MINI*` ↔ `mini`), or when `meta.nozzle_diameter` differs from `PrinterInfo.nozzle_diameter` by more than 0.001. Copy: `This file was sliced for {printer_model} with a {n} mm nozzle. {printer} is a {kind} with a {m} mm nozzle.` Omit parts that are unknown; show nothing when no comparison is possible.
- Copy without a thumbnail: `No preview in this file`.
- Repo rules: work on `main`, commit per task, do not push.

## Review Focus

1. File names with spaces, `#`, `?`, `%` and non-ASCII (`café #1?%.bgcode`) must round-trip through the details URL, the file-info call and the thumbnail call. Task 3 and Task 4 tests use that name.
2. A reload or bookmark of a details URL must load the page (static SPA fallback for nested dynamic routes). Task 4 step checks the server serves `200.html` for `/printers/mk4/files/usb/a/b.bgcode`.
3. Thumbnail fetch that fails upstream (404 or timeout) must show `No preview in this file`, not break the page. Task 4 test.
4. A `refs.thumbnail` that is absolute (`http://evil/x.png`), protocol-relative, or contains `..` or a query must not be fetched. Task 1 test.
5. A file removed while its page is open: Print/Delete or reload must show the "not found" state with a link back, not a blank page. Task 4 test.

---

### Task 1: Pass thumbnail links through the PrusaLink layer

**Files:**
- Modify: `crates/shared/src/printers.rs` (`FileEntry`)
- Modify: `crates/api/src/prusalink/wire.rs` (`WireFile`, `From<WireFile> for FileEntry`)
- Modify: `crates/api/src/prusalink/mod.rs` (new `thumbnail`, unit tests)
- Modify: every `FileEntry { .. }` literal that no longer compiles (`crates/cli/tests/support/mod.rs` and any others the compiler lists) — add `has_thumbnail: false`.

**Interfaces:**
- Produces: `FileEntry.has_thumbnail: bool`; `WireFile.refs: Option<WireRefs>` with `WireRefs { thumbnail: Option<String>, icon: Option<String> }`; `WireFile::thumbnail_ref(&self) -> Option<&str>` (thumbnail, else icon); `PrusaLink::thumbnail(&self, link: &str) -> Result<(String, Bytes), PrusaError>` (content type, bytes).

- [ ] **Step 1: Write failing unit tests** in `wire.rs` / `mod.rs` tests:
  - `wire_file_maps_has_thumbnail`: JSON with `"refs":{"thumbnail":"/thumb/l/usb/CUBE12~1.BGC"}` → `FileEntry.has_thumbnail == true`; `"refs":{"icon":"/thumb/s/usb/X"}` → true and `thumbnail_ref() == Some("/thumb/s/usb/X")`; no `refs` or `"refs":{"download":"/usb/X"}` → false.
  - `thumbnail_link_rejects_unsafe`: `thumbnail_url` (private helper you add, `fn thumbnail_url(&self, link: &str) -> Result<Url, PrusaError>`) returns `Err` for `http://evil/x.png`, `//evil/x.png`, `/thumb/../api/v1/status`, `/thumb/l/x?y=1`, `thumb/l/x` (no leading slash); returns `http://<host>/thumb/l/usb/CUBE12~1.BGC` for `/thumb/l/usb/CUBE12~1.BGC`, and decodes percent-escapes per segment (`/thumb/l/usb/My%20Parts/a.bgc` → path segment `My Parts`, re-encoded once by `endpoint`).
- [ ] **Step 2: Run** `cargo test -p starstar-api --lib prusalink` → FAIL (missing items).
- [ ] **Step 3: Implement.** Split the link on `/` after the leading slash, percent-decode each segment (`percent_encoding::percent_decode_str`, already a dependency via reqwest's `url`; add `percent-encoding` to `crates/api/Cargo.toml` if it is not), reject empty segments, then call the existing `endpoint()` (it already refuses `.`/`..`/slashes). `thumbnail` does `send(GET)` and returns `Self::image(response)`.
- [ ] **Step 4: Run** `cargo test --workspace` → PASS; `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] **Step 5: Commit** `Pass PrusaLink thumbnail links through file info`.

### Task 2: Thumbnail API route

**Files:**
- Modify: `crates/api/src/printers/files.rs` (new `file_thumbnail`)
- Modify: `crates/api/src/routes/files.rs` (route + handler)
- Modify: `crates/api/tests/fake_prusalink/mod.rs` (canned data)
- Modify: `crates/api/tests/printers_files.rs`, `crates/api/tests/auth_roles.rs`

**Interfaces:**
- Consumes: Task 1 `thumbnail_ref`, `PrusaLink::thumbnail`.
- Produces: `pub async fn file_thumbnail(state: &AppState, caller: &Caller, printer: &str, storage: &str, path: &str) -> Result<(String, Bytes), ApiError>` (role `FILE_INFO_ROLE`; `ApiError` 404 with message `file has no preview` when `thumbnail_ref()` is `None` or the link is rejected); route `GET /api/printers/{name}/thumbnails/{storage}/{*path}`.

- [ ] **Step 1: Fake data.** In `default_for`, add `"refs":{"thumbnail":"/thumb/l/usb/CUBE12~1.BGC","download":"/usb/CUBE12~1.BGC"}` to the `GET /api/v1/files/usb/cube-12mm.bgcode` body, and `("GET", "/thumb/l/usb/CUBE12~1.BGC") => png()`.
- [ ] **Step 2: Write failing tests** in `printers_files.rs`:
  - `file_info_reports_has_thumbnail`: GET file info → `j["has_thumbnail"] == true`.
  - `serves_thumbnail`: Viewer GET `/api/printers/mk4/thumbnails/usb/cube-12mm.bgcode` → 200, `content-type: image/png`, `cache-control: private, max-age=300`, body starts with `\x89PNG`; the fake recorded an authorized `GET /thumb/l/usb/CUBE12~1.BGC`.
  - `thumbnail_missing_is_404`: set the file info canned body without `refs` → 404 and no request to `/thumb/`.
  - `thumbnail_unsafe_link_is_404`: canned `refs.thumbnail = "http://evil/x.png"` → 404, no request beyond file info.
  - `thumbnail_unknown_file_is_404`: `/thumbnails/usb/nope.bgcode` → 404.
  - `thumbnail_encoded_path_round_trips`: set canned file info at `/api/v1/files/usb/My%20Parts/caf%C3%A9%20%231%3F%25.bgcode` with a thumbnail ref; request `/api/printers/mk4/thumbnails/usb/My%20Parts/caf%C3%A9%20%231%3F%25.bgcode` → 200 (mirror the existing `file_paths_round_trip_encoded` test for the exact encoded upstream URI).
  - `auth_roles.rs`: add `("GET", "/api/printers/mk4/thumbnails/usb/cube-12mm.bgcode")` to the Viewer route list so the unauthenticated → 401 and role checks cover it.
- [ ] **Step 3: Run** `cargo test -p starstar-api --test printers_files --test auth_roles` → FAIL.
- [ ] **Step 4: Implement** `file_thumbnail` (same lookup steps as `file_info`, then `client.thumbnail(link)`) and the handler returning `([(CONTENT_TYPE, ct), (CACHE_CONTROL, "private, max-age=300")], bytes)`. Register the route in `routes/files.rs::router`.
- [ ] **Step 5: Run** `cargo test --workspace` (with `STARSTAR_TEST_DATABASE_URL` set) → PASS; clippy clean; `cargo fmt --check` clean.
- [ ] **Step 6: Commit** `Serve file thumbnails from the printer`.

### Task 3: Link print files to their details page

**Files:**
- Modify: `web/app/types/api.ts` (`FileEntry.has_thumbnail?: boolean`)
- Modify: `web/app/utils/paths.ts`
- Modify: `web/app/components/FileBrowser.vue`
- Test: `web/test/paths.test.ts`, `web/test/file-browser.test.ts`

**Interfaces:**
- Produces (`paths.ts`):
  - `thumbnailPath(name: string, storage: string, segments: string[]): string` → `/api/printers/{name}/thumbnails/{storage}/{segments…}` (each part encoded like `filePath`).
  - `fileDetailsRoute(name: string, storage: string, segments: string[]): string` → `/printers/{name}/files/{storage}/{segments…}` (each part encoded).
  - `folderQuery(storage: string, segments: string[]): { storage: string; dir?: string }` and its inverse `parseFolderQuery(q: LocationQuery): { storage?: string; segments: string[] }` (`dir` is the segments joined with `/`; empty → omitted).
- FileBrowser: `print_file` names render as `<NuxtLink :to="fileDetailsRoute(...)">`; on init, if `route.query.storage` names an available storage, start there and in `parseFolderQuery(...).segments`; when the folder changes, `router.replace({ query: folderQuery(...) })` so Back returns to it.

- [ ] **Step 1: Write failing tests.**
  - `paths.test.ts`: `fileDetailsRoute('mk4','usb',['My Parts','café #1?%.bgcode'])` → `/printers/mk4/files/usb/My%20Parts/caf%C3%A9%20%231%3F%25.bgcode`; `thumbnailPath` same segments → `/api/printers/mk4/thumbnails/usb/My%20Parts/caf%C3%A9%20%231%3F%25.bgcode`; `parseFolderQuery(folderQuery('usb',['a','b c']))` round-trips.
  - `file-browser.test.ts`: `print files link to their details page` — the `zebra.bgcode` row contains an `a` with `href="/printers/mk4/files/usb/zebra.bgcode"`; folders still render as buttons; inside `My Parts` the café file links to the encoded URL above. `opens the folder from the query` — mount with route query `{ storage: 'usb', dir: 'My Parts' }` → rows are the `sub` children.
- [ ] **Step 2: Run** `npm --prefix web test -- paths file-browser` → FAIL.
- [ ] **Step 3: Implement** as in Interfaces.
- [ ] **Step 4: Run** `npm --prefix web run typecheck && npm --prefix web test` → PASS.
- [ ] **Step 5: Commit** `Link print files to a details page`.

### Task 4: File details page

**Files:**
- Create: `web/app/pages/printers/[name]/files/[storage]/[...path].vue`
- Create: `web/app/utils/compat.ts` with `slicedMismatch(meta: PrintMeta | null | undefined, kind: PrinterKind, nozzle?: number | null, printer: string): string | null` (implements the Global Constraints rule and copy)
- Modify: `web/app/utils/api.ts` (`as: 'headers'`)
- Test: `web/test/file-details.test.ts`, `web/test/compat.test.ts`, `web/test/api.test.ts`

**Interfaces:**
- Consumes: Task 3 `thumbnailPath`, `fileDetailsRoute`, `folderQuery`; `filePath`, `printerPath`; `ConfirmDialog`; `usePrinterPoll(name)` for `state` (same `running` rule as FileBrowser: `PRINTING|PAUSED|ATTENTION|BUSY`); `GET printerPath(name)` → `PrinterSummary.kind`; `GET printerPath(name,'info')` → `nozzle_diameter`; `GET filePath(...)` → `FileEntry`; `HEAD filePath(...)` with a new `api` option `as: 'headers'` (added in `web/app/utils/api.ts`, returns the response `Headers`; covered by a new case in `web/test/api.test.ts`) → "Printing now: yes" when header `currently-printed` is `true`. A failed HEAD leaves the row out.
- Page sections, in order: back link (`NuxtLink` to `/printers/{name}?{folderQuery(storage, parent segments)}`), `<h1>` display name, preview figure (blob URL like `CameraSnapshot.vue`; revoke on unmount; only requested when `has_thumbnail`), mismatch warning, details `<dl>` (printer model, nozzle, layer height, filament type, filament g, nozzle/bed temp, estimated time via `formatDuration`, size via `formatBytes`, modified via `formatDate`), Print (operator+) and Delete (admin) buttons with `ConfirmDialog`. After Delete succeeds, `navigateTo` the back link. File-info 404 → `This file is no longer on the printer.` plus the back link.

- [ ] **Step 1: Write failing tests** (`compat.test.ts`): `MK4IS`/0.4 vs `mk4`/0.4 → null; `MK4S`/0.4 vs `mk4`/0.6 → message containing `0.4 mm` and `0.6 mm`; `MINIIS` vs `mk4` → message; missing meta → null.
- [ ] **Step 2: Write failing tests** (`file-details.test.ts`, stubbing fetch with `helpers.ts` like `printer-page.test.ts`):
  - `shows preview and details`: file with `has_thumbnail: true`, thumbnail route returns PNG → an `img` with a `blob:` src and alt `Preview of cube-12mm.bgcode`; `dl` shows `PLA`, `0.2 mm`, `50m`.
  - `no preview`: `has_thumbnail: false` → text `No preview in this file` and no thumbnail request; `has_thumbnail: true` but thumbnail route 404 → same text.
  - `role buttons`: viewer sees neither button; operator sees Print only; admin sees both; Print disabled when status state is `PRINTING`.
  - `print confirms and posts`: click Print → dialog title `Start printing cube-12mm.bgcode on mk4?` → confirm → `POST /api/printers/mk4/files/usb/cube-12mm.bgcode/print`.
  - `delete returns to folder`: admin deletes `My Parts/café #1?%.bgcode` → `DELETE` to the encoded path, then route is `/printers/mk4?storage=usb&dir=My%20Parts`.
  - `printing now`: HEAD returns `currently-printed: true` → `dl` shows `Printing now` `yes`.
  - `api.test.ts` `returns headers`: `api(url, { method: 'HEAD', as: 'headers' })` resolves to a `Headers` with the stubbed `currently-printed` value.
  - `missing file`: file info 404 → `This file is no longer on the printer.` and a back link.
  - `mismatch warning`: printer kind `mk4`, info nozzle 0.6, file nozzle 0.4 → warning text visible.
- [ ] **Step 3: Run** `npm --prefix web test -- file-details compat` → FAIL.
- [ ] **Step 4: Implement** the page and `compat.ts`.
- [ ] **Step 5: Run** `npm --prefix web run typecheck && npm --prefix web test` → PASS.
- [ ] **Step 6: Deep-link check.** `npm --prefix web run generate`, then run the API against a disposable Postgres (see README "Running the tests") with `STARSTAR_WEB_DIR=web/.output/public` and confirm `curl -s -o /dev/null -w '%{http_code} %{content_type}' http://127.0.0.1:8080/printers/mk4/files/usb/a/b.bgcode` prints `200 text/html`. If it does not, fix the static fallback in the API (`crates/api/tests/web.rs` gets a test for a nested path) in this task.
- [ ] **Step 7: Commit** `Add file details page with preview`.

### Task 5: Docs, full suite, deploy

**Files:**
- Modify: `README.md` (one line under the web UI / API sections: file details page and the thumbnail route)
- Modify: `skills/starstar/SKILL.md` only if it lists API routes (add the thumbnail route; no behaviour change for agents)

- [ ] **Step 1:** Update docs as above.
- [ ] **Step 2:** Run the full suite: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm --prefix web run typecheck`, `npm --prefix web test` → all PASS.
- [ ] **Step 3:** Commit `Document the file details page`.
- [ ] **Step 4 (needs the user's OK to push):** push `main`, wait for the publish run for that exact commit, pin its digest in `deploy/thor/kustomization.yaml`, run `deploy/thor/preflight.sh thor` and `kubectl --context thor apply -k deploy/thor/`, and verify in the browser that `cube-12mm.MK4.bgcode` on `mk4-2` shows its preview.
