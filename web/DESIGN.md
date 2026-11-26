# starstar web UI: design system

A dev document describing what shipped. It is not served. Source of truth: `app/assets/css/tokens.css`, `base.css` and the components in `app/components/`.

## Concept: the shadow board

Every printer has a place on a pale painted board. A printer that answers fills its place. One that doesn't leaves only its outline. The same silhouette geometry draws the card glyph, the offline shadow and the empty-board placeholder (`PrinterSilhouette.vue`, variants `filled` and `shadow`).

## Tokens (`tokens.css`)

| Token | Light | Dark |
|---|---|---|
| `--board` (ground) | `oklch(95% .012 235)` | `oklch(22% .02 255)` |
| `--tool` (cards, panels, inputs) | `oklch(99.4% .004 235)` | `oklch(27% .02 255)` |
| `--ink` / `--outline` | `oklch(24% .025 255)` | `oklch(96% .01 240)` / `oklch(80% .02 240)` |
| `--bar` (top bar, primary buttons) | `oklch(40% .12 258)` | `oklch(33% .1 258)` |
| `--st-active` printing | `oklch(60% .15 150)` | `oklch(72% .16 150)` |
| `--st-hold` paused / attention | `oklch(80% .15 80)`, ink text | same |
| `--st-fault` error | `oklch(55% .2 27)` | `oklch(66% .19 27)` |
| `--danger` | `oklch(55% .2 27)` | `oklch(60% .2 27)` |

Dark follows `prefers-color-scheme`. State colours always come with a text label and an icon shape. `--danger` appears only in confirmation dialogs (Stop print, Delete, Remove, Disconnect).

- **Type:** Atkinson Hyperlegible Next (400, 600, 700), self-hosted through `@fontsource`. Scale ratio 1.2: `--step--1` .833rem, `--step-0` 1rem, `--step-1` 1.2rem, `--step-2` 1.44rem, `--step-3` 1.728rem. Line height 1.4 for text, 1.1 for readouts. Numbers use `.num` (tabular figures). No italics.
- **Spacing:** `--s1`..`--s8` = 4..32 px in 4 px steps. `--peg` = 24 px (the dot-grid pitch).
- **Shape:** 2 px outline (`--edge`), 6 px radius. 1 px hairlines inside tools.
- **Motion:** 150 ms (`--t-fast`) and 180 ms (`--t-state`), ease-out, on state changes only (shadow fill, dialog open, button colour). All transitions and animations are off under `prefers-reduced-motion: reduce`.

## Shared classes (`base.css`)

`.btn` with `.btn-primary` (filled bar blue), `.btn-secondary` (outline) and `.btn-danger` (filled red, dialogs only), `.btn-small`. States: default, hover, `:focus-visible` (3 px ring, 2 px offset), active, disabled, `aria-busy` (progress cursor). `.tool` (card/panel), `.board-ground` (dot-grid ground, dashboard and bare pages), `.board` (whole-cell grid, `repeat(auto-fill, minmax(18rem, 1fr))`, one column under 40 rem), `.field`, `.hairline`, `.num`, `.muted`, `.chip`, `.notice` and `.notice-error`, `.table-wrap`, `.visually-hidden`, `.skip-link`. Native `<dialog>`, `<table>`, `<select>` and `<input>` are styled directly.

## Layouts

- `default.vue`: the blue bar (wordmark, nav, username, role chip, Sign out), a skip link, and `<main id="main" class="page">`. Under 40 rem the nav becomes a horizontal scroll row.
- `bare.vue`: board ground with one centred column (sign-in, setup, consent).

## Components

| Component | Notes |
|---|---|
| `PrinterCard` | Fixed height (15 rem) in every state. Skeleton, filled, and shadow (`is-shadow`: transparent fill, dashed outline, dashed silhouette, "Not answering · retrying in N s · last seen …"). Whole card is one link. Owns its poller. |
| `StateBadge` | Icon shape plus label: play (active), pause bars (hold), triangle (fault), ring (rest). |
| `TempReadout` | Large current value, small target, tabular. |
| `JobBlock` | Same three lines whether a job exists or not ("No job" keeps the layout). Middle-truncated file name, `<progress>`, "about … left" when estimates are inaccurate. |
| `JobControls` | Buttons from `jobActions`. Pause, resume and continue act at once. Stop opens `ConfirmDialog`. Hidden below operator. |
| `ConfirmDialog` | `<dialog>` via `showModal()`. Cancel takes focus, Escape cancels, busy state, inline error that keeps it open. |
| `CameraSnapshot` | 4:3 frame, manual refresh, revokes object URLs. |
| `FileBrowser` | Storage tabs, breadcrumb, table (folders first), Print (operator) and Delete (admin) behind confirmation. |
| `ConsentForm` | Native form posts to `/oauth/authorize/decision`. Radios cap at operator. Admin is a separate checkbox; while ticked the radios are disabled and a hidden `role=admin` plus `allow_admin=true` is the only role sent. Deny is its own form and sends no role. |
| `FieldRow`, `Notice` | Label, control, hint and error wiring (`aria-describedby`); `role=status` or `role=alert`. |

## Behaviour that the design depends on

- Polling (`utils/poll.ts`): 5 s while visible, doubling to 30 s after failures, reset on success, never overlapping, paused while hidden.
- The CSRF token lives only in `useState('session')`. Nothing secret reaches storage, the URL or the console.
- The UI hides what the role can't do; the server still enforces it.
- `next` on `/login` passes through `safeNext` (same-origin app paths only).

## Not shipped

Adding printers, uploads, camera settings, firmware, folder creation and live video are CLI-only for now.
