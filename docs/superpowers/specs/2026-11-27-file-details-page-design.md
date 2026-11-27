# File details page with preview

Date: 2026-11-27. Size: small change (short design).

## Intent

In a printer's file list in the web UI, clicking a print file's name opens a page for that file. The page answers "is this the right part?" by showing the preview image (the same thumbnail the printer's screen shows) and the slicer details. From the same page the user can start the print or delete the file.

## Goals

- Print file names in `FileBrowser.vue` link to a details page.
- The page shows the thumbnail, the slicer metadata, size, modified time and "printing now".
- The page has Print and Delete buttons. They follow the same roles, confirmations and disabled states as the list.

## Non-goals

- Thumbnails in the file list itself (can follow later; the new endpoint makes it easy).
- CLI or MCP changes.
- Extracting thumbnails in starstar, or showing a G-code toolpath preview.
- Details pages for folders, firmware or other non-print files.

## Anti-goals

- The browser never talks to the printer directly or sees PrusaLink credentials.
- Downloading whole G-code files just to show a picture.

## Approaches

1. **API proxies the printer's own thumbnail (chosen).** PrusaLink file info carries `refs.thumbnail` (and `refs.icon`). Starstar reads that link and streams the image back to the signed-in browser. It is cheap, and it shows exactly what the printer screen shows.
2. API downloads the G-code and extracts the thumbnail. Rejected: transfers the whole file on every view and duplicates firmware work.
3. Browser loads the image from the printer. Rejected: exposes the printer and its credentials to the browser.

## Design

**API (Rust)**
- `prusalink/wire.rs`: `WireFile` reads `refs { thumbnail, icon }`. `FileEntry` gains `has_thumbnail: bool` (default false, so older clients keep working), true when the printer reports a thumbnail or icon link.
- New route `GET /api/printers/{name}/thumbnails/{storage}/{*path}`, Viewer role. It looks up the file on the printer, takes `refs.thumbnail` (falls back to `refs.icon`), fetches it from PrusaLink with the printer's credentials and returns the bytes with the printer's `image/*` content type. It returns 404 when the file has no thumbnail. Responses carry `Cache-Control: private, max-age=300`. The upstream link is a path on the same printer only; anything else is rejected. The path rules match the existing file routes.
- A separate prefix (`/thumbnails/`) is used because the file routes capture the rest of the path with `{*path}`.

**Web UI (Nuxt)**
- New page `pages/printers/[name]/files/[storage]/[...path].vue`.
- `FileBrowser.vue`: the name of a `print_file` becomes a `NuxtLink` to that page. Folders keep their current behaviour.
- Page content, top to bottom:
  - A back link to the printer, opening at the same folder.
  - The file's display name.
  - The preview, loaded like `CameraSnapshot.vue` (signed-in fetch to a blob URL). Without a thumbnail it shows "No preview in this file".
  - A details list: printer model, nozzle, layer height, filament type and grams, temperatures, estimated time, size, modified, printing now.
  - Print (operator and up) and Delete (admin) buttons with the existing `ConfirmDialog` texts. Print is disabled while a job runs. After a delete the page returns to the folder.
- My call: when the file's `printer_model` does not match the printer's kind (for example an MK4S file on an MK4), the page shows a warning next to the Print button. This supports "is this the right part" at no real cost.

**Tests**
- Rust: wire parsing of `refs`, `has_thumbnail` mapping, the thumbnail route (ok, no thumbnail → 404, unknown file → 404, unauthenticated → 401) against the existing PrusaLink test double.
- Web: the list links print files only; the page renders details, the no-preview state, role-dependent buttons and the model-mismatch warning.

## Left to the builder

Styling within the existing design tokens, exact copy, and how the folder position is carried back (query string or path).
