# Full printer control, built-in auth, MCP, web UI, CLI skill

Status: approved · 2026-11-26

## Intent

starstar is the print backend for a shop or household, used by a few people and
by agents that run unattended. Its job is fast turnaround on 3D printing and
prototyping alongside cad-skills: an agent models a part, slices it, sends it
to a printer, watches the job and reports back. People watch and intervene
from a browser or the CLI.

This round makes every control that PrusaLink offers available through the
starstar API, the CLI and MCP, puts all of it behind sign-in with roles, adds
a basic web UI, ships a skills.sh skill for the CLI, and prepares a small test
print. The endpoints are built and tested against a fake PrusaLink only. **No
real printer is driven in this round.**

## Goals

1. API endpoints that cover every operation in `docs/prusalink-openapi.yaml`,
   translated into starstar's own types (as the existing info/status/storage
   endpoints already are).
2. A matching CLI command for every endpoint.
3. Built-in users and roles, with starstar acting as its own OAuth 2.1
   authorization server, so the CLI, MCP clients and the web UI all sign in
   the same way.
4. An MCP endpoint at `/mcp` that MCP clients (such as Claude Code) can
   connect to with the standard MCP auth flow: discover, register, browser
   login and consent, localhost redirect, done.
5. A Nuxt web UI (designed with the impeccable skill) for sign-in, first-admin
   setup, consent, user and app management, and a basic printer dashboard.
6. A skill for the CLI in this repo that installs with
   `npx skills add kerryhatcher/starstar`.
7. A ~12 mm calibration cube modelled with CadQuery and sliced for MK4 and
   MINI, ready for the first real print.

## Non-goals

- Driving a real printer, uploading to one, or starting a real print.
- External OIDC providers (Authentik, Google, and so on). The design keeps
  that door open, but it is not built now.
- A print queue or scheduler across printers. Agents choose a printer
  themselves this round.
- TLS termination inside starstar. Run it behind a reverse proxy for HTTPS;
  see Constraints.
- Live streaming video. The camera endpoints work with snapshots.

## Anti-goals

These would make the work a failure even if it technically works:

- An agent token that can update firmware, delete files or manage users by
  default. Destructive actions need the admin role.
- Any endpoint other than health, the OAuth discovery and login endpoints,
  and the static UI that works without a token.
- Printer passwords, user passwords or tokens appearing in logs, API
  responses or `Debug` output.
- Tests that need a real printer or the network.

## Constraints

- Rust 2024 workspace: `crates/shared`, `crates/api` (axum 0.8, SQLite via
  rusqlite), `crates/cli` (clap, blocking reqwest). Build on what is there:
  the PrusaLink client in `crates/api/src/prusalink.rs`, the store in
  `store.rs`, and `ApiError`.
- Commits go directly on `main` (AGENTS.md), but parts can be built on
  worktree branches and merged into `main` locally. Nothing is pushed.
- The API binds `127.0.0.1:8080` by default. With auth in place it may be
  bound to the LAN (`STARSTAR_BIND`). Its public origin is set with
  `STARSTAR_PUBLIC_URL` (defaults to `http://127.0.0.1:8080`) and is used as
  the OAuth issuer and in redirects. Session cookies are `Secure` when the
  public URL is https.
- License AGPL-3.0-only.

## Approaches considered

- **A. The Rust API does everything; Nuxt is a static app it serves
  (chosen).** The axum API is the OAuth server, the printer API, the MCP
  server, and serves the built Nuxt app (`nuxt generate`, no SSR). One
  binary, one origin, all security logic in Rust with tests. The cost is no
  server-side rendering, which the dashboard doesn't need.
- **B. Nuxt runs its own server (Nitro)** for sessions and consent and
  proxies to the API. More idiomatic Nuxt, but two processes to deploy and
  auth logic split across TypeScript and Rust.
- **C. Bundle an off-the-shelf identity provider (such as Pocket ID).** Less
  custom security code, but an extra service to run. Dropped because the
  user asked for built-in auth for now.

The user picked A.

## Decisions

### Users and roles

- Users have a username, an argon2id password hash, a role, and a disabled
  flag.
- Roles are ordered: `viewer` < `operator` < `admin`.
  - **viewer:** read printers, status, jobs, files and storage listings,
    transfers, cameras, snapshots.
  - **operator:** everything a viewer can do, plus upload files, start,
    pause, resume, continue and stop jobs, cancel transfers, and take a
    snapshot.
  - **admin:** everything an operator can do, plus add and remove printers,
    delete files and folders, change camera settings, order, registration
    and Connect linking, check and apply firmware updates, and manage users.
- **Bootstrap:** while there are no users, `POST /api/setup` (and the web
  UI's setup page) creates the first user as admin. After that it returns
  409, registration is closed, and only admins create users.
- Users can change their own password. Admins can add, disable, re-enable,
  change the role of, and remove users. The last enabled admin can't be
  demoted, disabled or removed.

### OAuth 2.1 authorization server

starstar implements the subset of OAuth that the MCP authorization spec
requires:

- `GET /.well-known/oauth-authorization-server` (RFC 8414 metadata).
- `GET /.well-known/oauth-protected-resource` and
  `/.well-known/oauth-protected-resource/mcp` (RFC 9728). Unauthenticated
  requests to `/mcp` and `/api/*` return 401 with a `WWW-Authenticate:
  Bearer resource_metadata="…"` header.
- `POST /oauth/register`: dynamic client registration (RFC 7591), public
  clients only (`token_endpoint_auth_method: none`). Redirect URIs must be
  loopback (`http://127.0.0.1:*`, `http://localhost:*`, `http://[::1]:*`,
  any port per RFC 8252) or https.
- `GET /oauth/authorize`: authorization code flow with mandatory PKCE
  (S256 only). It validates the client and redirect URI, then hands off to
  the web UI's consent page. Without a browser session it sends the user to
  the login page first and returns to consent afterwards.
- `POST /oauth/authorize/decision`: called by the consent page with the
  user's Allow or Deny and the granted role. Protected by the session cookie
  and a CSRF token. Redirects to the client with `code` and `state`, or with
  `error=access_denied`.
- `POST /oauth/token`: `authorization_code` (with `code_verifier`) and
  `refresh_token` grants. Refresh tokens rotate on every use, and reusing an
  old refresh token revokes the whole grant.
- `POST /oauth/revoke` (RFC 7009).
- **Scopes:** `starstar:viewer`, `starstar:operator`, `starstar:admin`. The
  consent page shows the requested role, and the user can lower it. The
  granted role is capped at the user's own role, and each request is checked
  against the lower of the grant's role and the user's current role, so
  demoting or disabling a user takes effect immediately.
- **Tokens:** opaque random strings (256-bit, with a `ss_at_` or `ss_rt_`
  prefix), stored as SHA-256 hashes. Access tokens last 1 hour and refresh
  tokens 30 days (sliding). Authorization codes last 60 seconds and are
  single use.
- **Resource indicators (RFC 8707):** a `resource` parameter is accepted and
  recorded. Tokens are valid for both `/api` and `/mcp`, since they share one
  origin and role model.

### Browser sessions

- `POST /api/session` (username and password) sets an HttpOnly,
  `SameSite=Lax` session cookie. `DELETE /api/session` signs out, and `GET
  /api/session` returns the current user.
- The web UI calls `/api/*` with the cookie. Cookie-authenticated requests
  that change state must send the `X-CSRF-Token` header (double-submit
  value).
- Login failures are rate-limited per username and IP, and the error message
  is the same for an unknown user and a wrong password.

### Who calls with what

| Caller | How it gets a token | Sends |
| --- | --- | --- |
| Web UI | `POST /api/session` | cookie plus CSRF header |
| CLI (`starstar login`) | OAuth code plus PKCE with a loopback redirect, its own registered client | `Authorization: Bearer` |
| MCP client | standard MCP auth discovery, then dynamic registration, then code plus PKCE | `Authorization: Bearer` |
| Unattended agent with no MCP | `starstar login` once on that machine. The CLI refreshes tokens itself. | `Authorization: Bearer` |

- **CLI token storage:** `$XDG_CONFIG_HOME/starstar/credentials.json` (mode
  0600), keyed by API URL. `STARSTAR_TOKEN` overrides it.
- **Headless machines:** `starstar login --no-browser` prints the authorize
  URL, the user opens it on any device, and after consent the page shows the
  code to paste back into the CLI. This is the `urn:ietf:wg:oauth:2.0:oob`
  style flow, implemented as a starstar-hosted "copy this code" redirect
  target, which is allowed only for the CLI's own client.
- **Connected apps:** the web UI and API list a user's grants (client name,
  role, created, last used) and can revoke them. Admins can see and revoke
  everyone's grants.

### Printer endpoints

All live under `/api/printers/{name}` and use starstar types from
`crates/shared`. "Role" is the minimum role required.

| starstar endpoint | PrusaLink | Role |
| --- | --- | --- |
| `GET /api/printers`, `GET …/{name}`, `…/info`, `…/status`, `…/storage` | existing | viewer |
| `POST /api/printers`, `DELETE …/{name}` | existing | admin |
| `GET …/job` (current job, or 204) | `GET /api/v1/job` | viewer |
| `DELETE …/job/{id}` (stop) | `DELETE /api/v1/job/{id}` | operator |
| `PUT …/job/{id}/pause`, `/resume`, `/continue` | same | operator |
| `GET …/transfer` (or 204) | `GET /api/v1/transfer` | viewer |
| `DELETE …/transfer/{id}` | same | operator |
| `GET …/files/{storage}/{*path}` (metadata or folder listing) | `GET /api/v1/files/…` | viewer |
| `HEAD …/files/{storage}/{*path}` | `HEAD` | viewer |
| `PUT …/files/{storage}/{*path}` (upload body; `?overwrite=true`, `?print=true`) | `PUT` with `Overwrite` and `Print-After-Upload` | operator |
| `POST …/files/{storage}/{*path}/print` | `POST` | operator |
| `DELETE …/files/{storage}/{*path}` (`?force=true`) | `DELETE` with `Force` | admin |
| `GET …/cameras` | `GET /api/v1/cameras` | viewer |
| `PUT …/cameras/order` | `PUT /api/v1/cameras` | admin |
| `GET …/cameras/{id}` | `GET` | viewer |
| `POST …/cameras/{id}` (register), `DELETE …/cameras/{id}` | same | admin |
| `GET …/cameras/snap` (default camera), `GET …/cameras/{id}/snap` | same (PNG passthrough) | viewer |
| `POST …/cameras/{id}/snap` (take snapshot) | same | operator |
| `PATCH …/cameras/{id}/config`, `DELETE …/cameras/{id}/config` | same | admin |
| `POST …/cameras/{id}/connection`, `DELETE …` | same | admin |
| `GET …/update/{env}`, `POST …/update/{env}` | `/api/v1/update/{env}` | admin |

- **Upstream status mapping:** PrusaLink 404 becomes 404, 409 becomes 409
  (with a message such as "printer is busy"), 403 becomes 403, and 503 or 408
  become 502 or 504. Anything else is 502 as today.
- **Uploads stream** from the client through to the printer without
  buffering the whole file. The upload size limit is configurable
  (`STARSTAR_MAX_UPLOAD_MB`, default 512).
- **Path safety:** storage names are validated against the printer's storage
  list, and file paths reject `..`, empty segments and NUL bytes.
- **Timeouts:** the PrusaLink client keeps its 10-second timeout for control
  calls and uses a longer one for uploads.

### MCP endpoint

- `/mcp` uses the official Rust SDK (`rmcp`) over streamable HTTP, mounted in
  the same axum router, behind the same bearer-token middleware.
- Tools map 1:1 onto the printer endpoints, named by verb, for example
  `list_printers`, `printer_status`, `get_job`, `pause_job`, `resume_job`,
  `stop_job`, `list_files`, `upload_file`, `start_print`, `delete_file`,
  `camera_snapshot`, `check_update`, `apply_update`. Each tool checks the same
  minimum role as its endpoint and carries MCP annotations (`readOnlyHint`,
  `destructiveHint`).
- `upload_file` takes either base64 content (small files) or a URL the API
  fetches from. Large G-code from agents normally goes through the CLI.
- Camera snapshots are returned as MCP image content.

### Web UI (`web/`, Nuxt)

- Nuxt 4, `ssr: false`, built with `nuxt generate`. The API serves the
  output from `STARSTAR_WEB_DIR` (default `web/.output/public`), with SPA
  fallback to `index.html` for paths not under `/api`, `/oauth`, `/mcp` or
  `/.well-known`.
- **Pages:** setup (first admin), sign in, consent (client name, requested
  role with an option to lower it, Allow or Deny), dashboard, printer detail,
  users (admin), connected apps, account (change password).
- **Dashboard:** a card per printer with its state, nozzle and bed
  temperatures, and current job (file name, progress, time remaining). It
  polls every 5 seconds while visible. The printer detail page adds a
  camera snapshot that refreshes on demand, pause, resume and stop buttons
  for operators (stop asks for confirmation), and the file list.
- Visual design is done with the impeccable skill.
- During development, `nuxt dev` proxies `/api`, `/oauth`, `/mcp` and
  `/.well-known` to the API.

### CLI

- `starstar login [--no-browser]`, `logout`, `whoami`.
- `starstar user add|list|set-role|disable|enable|remove|passwd`.
- `starstar apps list|revoke`.
- `starstar printer …` (existing) plus:
  - `job show|pause|resume|continue|stop <printer> [--id N]`. Without
    `--id`, the CLI looks up the current job id and shows it in its output.
  - `transfer show|cancel <printer>`.
  - `files ls|info|upload|print|rm <printer> <storage>/<path>`, where upload
    takes `--overwrite` and `--print`.
  - `camera ls|show|snap|order|register|remove|config|reset|connect|disconnect`.
    `snap` writes a PNG with `-o`.
  - `update check|apply <printer> [--env prusalink]`. `apply` requires
    `--yes`.
- Every command takes `--json` for machine-readable output, which agents use.
- Destructive commands (`rm`, `stop`, `update apply`, `printer remove`,
  `user remove`) ask for confirmation when run interactively. `--yes` skips
  the prompt, and without a TTY they refuse unless `--yes` is given.

### CLI skill

- Lives at `skills/starstar/SKILL.md` (the skills.sh layout), with
  frontmatter `name: starstar` and a description that triggers on 3D
  printing, sending prints, and printer status.
- Covers installing the CLI, `starstar login`, the cad-skills to prusaslicer
  to `starstar files upload --print` loop, monitoring with `job show --json`,
  safe behaviour (check status before printing, never `--yes` on destructive
  commands without the user's go-ahead), and connecting the MCP endpoint
  instead.

### Test cube

- `prints/cube-12mm/cube.py`: a CadQuery 12 mm cube with a 0.5 mm bottom
  chamfer (to counter elephant's foot) and "SS" debossed on top. It exports
  `cube-12mm.step` and `cube-12mm.stl`.
- Sliced with PrusaSlicer CLI to binary G-code for MK4 and MINI with stock
  Prusament PLA, 0.2 mm, profiles. The G-code is committed under
  `prints/cube-12mm/` along with the slicer's time and filament estimates.
  It is not sent to any printer.

### Storage changes

New SQLite tables (created with `CREATE TABLE IF NOT EXISTS` like the
existing one; a `schema_version` pragma for future migrations): `users`,
`sessions`, `oauth_clients`, `oauth_codes`, `oauth_grants` (a grant ties a
user, a client and a role together), `oauth_tokens` (hashed access and
refresh tokens linked to a grant). Expired rows are pruned on a timer.

### Testing

- A fake PrusaLink server in the API's tests (axum on an ephemeral port,
  with digest auth) for every new PrusaLink call, including error mappings.
- Auth tests: setup only once, role enforcement per endpoint (a table test
  over every route), the PKCE flow end to end, refresh rotation and reuse
  detection, revocation, and a disabled user's tokens failing immediately.
- MCP tests: a tool list per role, a tool call with an insufficient role
  failing.
- The CLI login flow is checked once in Chrome against a local API with a
  fake printer, and the MCP flow once with Claude Code against the local
  API. Neither touches a real printer.
- `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`, and
  `nuxt build`/`typecheck` must pass.

## Builder-check resolutions (controller's calls)

A builder check (Claude Haiku) raised these. The user approved the design and
delegated these calls. Each one overrides anything above that conflicts with
it.

1. **Setup code.** While no users exist, the API prints a one-time setup code
   to stdout at startup (or uses `STARSTAR_SETUP_TOKEN` if set).
   `POST /api/setup` and the setup page require it. The code is compared in
   constant time and is discarded once the first admin exists. `/api/setup`
   is the one other unauthenticated endpoint the anti-goal allows.
2. **Consent defaults.** The consent page pre-selects
   `min(requested role, operator, user's role)`. Granting admin needs a
   separate, deliberate choice ("also allow admin actions"), unchecked by
   default. The page shows the client name marked "self-declared", the
   redirect URI host, and the date the client was registered. There is no
   approval queue for client registration.
3. **No URL fetching.** MCP `upload_file` takes base64 content only, capped at
   `STARSTAR_MCP_MAX_UPLOAD_MB` (default 25). Larger files go through the CLI
   or HTTP upload. This removes the SSRF surface entirely.
4. **Plain HTTP on the LAN.** Startup refuses a non-loopback `STARSTAR_BIND`
   when `STARSTAR_PUBLIC_URL` is not https, unless
   `STARSTAR_ALLOW_INSECURE_HTTP=1`. When that override is used, it logs a
   warning at startup. The CLI warns once (on stderr) when it sends a token to
   a non-loopback `http://` URL.
5. **Refresh rotation.** Reuse detection has a 30-second grace window: a
   refresh token reused within 30 s of its rotation returns the same
   successor pair instead of revoking the grant. Grants have an absolute
   lifetime of 90 days. The CLI also takes an advisory file lock on
   `credentials.json` while refreshing.
6. **The CLI's OAuth client** is pre-seeded as public client `starstar-cli`.
   Its allowed redirects are any loopback port, plus
   `{public_url}/oauth/code` (the copy-code page). The copy-code page is only
   valid for that client_id. A fixed ID is not a secret; the protection is
   that the code is shown only to the signed-in user who consented. The page
   tells them to paste it only into their own terminal.
7. **Folder creation** is out of scope this round. The spec doesn't say how
   PrusaLink tells a folder from a file, and guessing could create bogus
   files on a real printer. Recorded as a known gap.
8. **Web UI visual direction** is chosen by the builder with impeccable,
   without a sign-off step.

## Left to the builder

- Exact Rust crates for argon2, random tokens and SHA-256 (`argon2`, `rand`,
  `sha2` are the expected picks), and the `rmcp` version and its axum
  integration.
- The module layout inside `crates/api` (for example `auth/`, `oauth/`,
  `mcp.rs`, `routes/printers/*.rs`).
- Exact shared type names and fields for jobs, files, cameras and updates,
  following the PrusaLink schemas in the spec.
- Nuxt component structure and styling, done with the impeccable skill.
- The rate-limit numbers.
