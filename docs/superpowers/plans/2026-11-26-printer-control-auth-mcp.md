# Printer control, auth, MCP, web UI — Implementation Plan (top level)

> **For agentic workers:** this is the controller's top-level plan. Each part
> gets its own detailed TDD plan (written by that part's planner to
> `../starstar.worktrees/plans/printers-<part>.md`, outside git), which is
> reviewed before implementation. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Expose every PrusaLink control through the starstar API, CLI and MCP, behind built-in users, roles and an OAuth 2.1 server, with a Nuxt web UI, a skills.sh CLI skill, and a sliced 12 mm test cube.

**Architecture:** One axum binary is the printer API, the OAuth authorization server, the MCP server (`rmcp`, streamable HTTP at `/mcp`) and the static host for a client-only Nuxt app. Handlers declare their minimum role with extractor types. Printer operations live in plain async functions that both HTTP handlers and MCP tools call.

**Tech Stack:** Rust 2024, axum 0.8, rusqlite, reqwest + diqwest, clap 4, rmcp, argon2, Nuxt 4 (`ssr: false`), CadQuery, PrusaSlicer CLI.

**Spec:** `docs/superpowers/specs/2026-11-26-printer-control-auth-mcp-design.md`. Its "Builder-check resolutions" section overrides anything earlier in the spec that conflicts with it.

## Global Constraints

- Never contact a real printer. All PrusaLink behaviour is tested against an in-process fake (axum on an ephemeral port, digest challenge, as in `crates/api/tests/printers.rs`).
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` pass at every commit on every part branch.
- No secret (printer password, user password, token, setup code after use) in logs, `Debug` output or responses. Types holding secrets are not `Debug`.
- Each part only edits the files it owns (table below). If a part truly needs a change to a file it doesn't own, it makes the smallest possible edit and says so in its report.
- Commit messages end with `Co-Authored-By: Claude <model> <noreply@anthropic.com>`, naming the model that wrote the commit.
- Nothing is pushed.

## Interfaces already on `main` (scaffold commit)

- `starstar_shared::Role` { `Viewer` < `Operator` < `Admin` }: `as_str()`, `scope()` (`starstar:<role>`), `FromStr`, serde lowercase.
- `starstar_api::auth`:
  - `Caller { username: String, role: Role }` with `require(&self, min: Role) -> Result<(), ApiError>`.
  - Extractors `Viewer(Caller)`, `Operator(Caller)` and `Admin(Caller)` implement `FromRequestParts<AppState>` with `Rejection = ApiError`.
  - `authenticate(&mut Parts, &AppState) -> Result<Caller, ApiError>`.
  - `router(AppState) -> Router`, merged in `starstar_api::router`.
  - **Scaffold:** every request is treated as an anonymous admin until the auth part replaces it.
- `ApiError::Unauthorized(String)` (401) and `ApiError::Forbidden(String)` (403).
- `crates/api/tests/common/mod.rs`: `authorize(Builder, &AppState, Role) -> Builder`. Every integration test request goes through it. The auth part replaces its body.

## Parts, file ownership, dependencies

| Part | Owns | Depends on | Wave |
| --- | --- | --- | --- |
| `prusa` — every PrusaLink operation as API endpoints | `crates/api/src/prusalink.rs` (may become `prusalink/`), `crates/api/src/routes.rs` → `crates/api/src/routes/` (printers), new `crates/api/src/printers/` (operation functions), `crates/shared/src/printers.rs` (new; re-exported from shared lib), `crates/api/tests/printers*.rs`, `crates/api/tests/fake_prusalink/`, `crates/api/tests/roles.rs` | scaffold | 1 |
| `auth` — users, setup, sessions, OAuth AS, users/apps API | `crates/api/src/auth/**`, `crates/api/src/main.rs`, `crates/api/tests/common/mod.rs`, `crates/api/tests/auth*.rs`, `crates/api/tests/oauth*.rs`, `crates/shared/src/auth.rs` (new; request/response types for setup, session, users, apps, OAuth errors) | scaffold | 1 |
| `cube` — test print | `prints/cube-12mm/**` | none | 1 |
| `mcp` — `/mcp` endpoint | `crates/api/src/mcp/**`, `crates/api/tests/mcp*.rs` | prusa, auth | 2 |
| `cli` — every endpoint, login/logout/whoami, users, apps | `crates/cli/**` | prusa, auth | 2 |
| `web` — Nuxt UI and static serving | `web/**`, `crates/api/src/web.rs` (static files + SPA fallback) | prusa, auth | 2 |
| `skill` — skills.sh skill and docs | `skills/starstar/**`, `README.md`, `docs/README.md` | cli, mcp | 3 |

Shared edits that every part may make, keeping them to one line each where possible: `crates/api/src/lib.rs` (a `pub mod` line, `AppState` fields, merging a router), `crates/shared/src/lib.rs` (a `pub mod` line and re-export), `Cargo.toml` and crate `Cargo.toml` dependency lines. The integrator resolves conflicts in these files.

## Cross-part contracts each part plan must honour

- **prusa → mcp:** each printer operation is a `pub async fn` in `crates/api/src/printers/` taking `(&AppState, printer: &str, …typed args)` and returning `Result<SharedType, ApiError>`. HTTP handlers are thin wrappers over these functions plus the role extractor. MCP tools call the same functions after `Caller::require`.
- **prusa → auth:** every printer route takes exactly one role extractor matching the spec's endpoint table. `tests/roles.rs` holds a table of `(method, path, min role)` for every route and asserts that one role lower gets 403 and no credentials get 401. It is marked `#[ignore = "enabled when auth lands"]`; the integrator removes the ignore after both parts merge.
- **auth → everyone:** bearer tokens arrive as `Authorization: Bearer <token>`. Browser sessions use a cookie plus `X-CSRF-Token` on state-changing requests. Unauthenticated requests to `/api/*` and `/mcp` return 401 with `WWW-Authenticate: Bearer resource_metadata="{public_url}/.well-known/oauth-protected-resource"`. Exempt paths: `/api/health`, `/api/setup` (needs the setup code), `POST /api/session`, `/oauth/*` (each with its own checks), `/.well-known/*`, and static UI paths.
- **auth → mcp:** the auth part exposes `pub fn protected_resource_metadata_url(state) -> String` and serves `/.well-known/oauth-protected-resource` and `/.well-known/oauth-protected-resource/mcp`. `AppState` gains `auth: Arc<AuthStore>` and `config: Arc<Config>` (with `public_url`).
- **auth → cli/web:** endpoint paths and JSON shapes are those of the spec, with types in `crates/shared/src/auth.rs`. The OAuth endpoints use form-encoded RFC shapes.
- **prusa → cli/web:** JSON shapes are the types in `crates/shared/src/printers.rs`.

## Waves and process (sheepdog)

- Each part runs in its own worktree at `../starstar.worktrees/printers-<part>` on branch `sheepdog/printers-<part>`, created from integration branch `sheepdog/printers`. The integration branch merges into `main` locally when everything is in.
- **Planner (Claude Opus):** writes the part plan per `superpowers:writing-plans`. **Plan reviewer:** Gemini 4 Carbon (Antigravity `agy`).
- **Implementer (Claude Sonnet, by the user's choice while Codex quota is low):** follows the plan with TDD and commits on the part branch. **Code reviewer:** Claude Opus, read-only. Fixes loop until it passes.
- **Integrator (Claude Sonnet):** merges each reviewed part into `sheepdog/printers` and runs the full suite. Its conflict fixes are reviewed by Gemini.
- **Wave 1:** prusa, auth, cube. **Wave 2:** mcp, cli, web. **Wave 3:** skill, then the controller verifies end to end: the CLI login and MCP login against a local API with a fake printer, in Chrome, then merges to `main`.

## Part briefs

Detailed tasks are in the part plans. These are the outcomes each part must deliver; everything else comes from the spec.

### prusa
- [ ] Shared types for job, transfer, file/folder info, cameras (list, detail, config), update info, and file-upload options, modelled on the `components/schemas` in `docs/prusalink-openapi.yaml`.
- [ ] A PrusaLink client method for every path and verb in the spec, using digest auth. Uploads stream with `Content-Length`. Snapshots return bytes.
- [ ] Upstream status mapping: 404→404, 409→409, 403→403, 408→504, 503→502, else 502.
- [ ] Path safety: storage must be in the printer's storage list; paths reject `..`, empty segments and NUL.
- [ ] Every route in the spec's endpoint table, with the listed minimum role. No folder creation.
- [ ] A fake PrusaLink with every endpoint, and tests for each route's success path and at least one error mapping each.
- [ ] `tests/roles.rs`, ignored as described.

### auth
- [ ] Tables `users`, `sessions`, `oauth_clients`, `oauth_codes`, `oauth_grants` and `oauth_tokens` in an `AuthStore` on the same SQLite file.
- [ ] A setup code printed at startup while no users exist. `POST /api/setup` creates the first admin.
- [ ] argon2id password hashing. Rate-limited `POST /api/session`, with `GET` and `DELETE`. CSRF on cookie writes.
- [ ] Users API (admin): add, list, set role, disable/enable, remove, with the last-admin guard. `PUT /api/me/password`.
- [ ] Apps API: list and revoke my grants. Admins see and revoke all.
- [ ] OAuth: RFC 8414 and RFC 9728 metadata; RFC 7591 registration (public clients; loopback or https redirects); `/oauth/authorize` with mandatory S256 PKCE that hands off to the web consent page at `/consent?request=<id>` (or `/login?next=…`); `POST /oauth/authorize/decision`; `/oauth/token` with code and refresh grants, rotation and a 30 s reuse grace window; RFC 7009 revoke; the pre-seeded `starstar-cli` client; the `/oauth/code` copy-code page (a minimal server-rendered HTML page is fine); 90-day absolute grant lifetime.
- [ ] Real extractors: effective role = `min(grant role, user role)`, and disabled users are rejected.
- [ ] `main.rs` config: `STARSTAR_PUBLIC_URL`, the insecure-HTTP refusal on non-loopback binds, `STARSTAR_SETUP_TOKEN`, and pruning expired rows.
- [ ] `tests/common::authorize` creates a real user and token. Existing `tests/printers.rs` keeps passing through it.

### cube
- [ ] `prints/cube-12mm/cube.py` (CadQuery; 12 mm; 0.5 mm bottom chamfer; "SS" debossed 0.6 mm on top), run with `uv`. Exports STEP and STL.
- [ ] PrusaSlicer CLI produces binary G-code for MK4 (or MK4S, whichever profile is installed) and MINI with Prusament PLA at 0.20 mm. A `README.md` records the time and filament estimates and how to regenerate. Nothing is sent to a printer.

### mcp, cli, web, skill
Planned in wave 2 and 3 against the merged API. Briefs are in the spec sections "MCP endpoint", "CLI", "Web UI" and "CLI skill".

## Revision 2 — after plan review (overrides anything above that conflicts)

A Haiku review of this plan found gaps. These changes are already on
`sheepdog/printers` (scaffold v2) or are now binding on part plans.

**Scaffold v2 (on the branch):**
- `crates/api/src/config.rs`: `Config` (not `Debug`) with `database`, `bind`, `public_url` (no trailing slash), `allow_insecure_http`, `setup_token: Option<String>`, `max_upload_bytes`, `mcp_max_upload_bytes`, `web_dir`. It has `Config::from_env()` and `Default`. `AppState` gains `config: Arc<Config>`, and `main.rs` uses it. New settings go here only.
- `ApiError` gains `LengthRequired` (411), `PayloadTooLarge` (413), `TooManyRequests` (429), `Printer(StatusCode, String)` (passes through PrusaLink's 404, 409 and 403; never reuse `Forbidden`, which is about starstar roles), and `GatewayTimeout` (504).
- `tests/common`: `test_state() -> AppState` and `test_app() -> (Router, AppState)`. `tests/printers.rs` already uses `test_app`.

**Ownership changes:**
- `crates/api/src/error.rs` is scaffold-owned. Parts may add `From` impls and helpers but not change existing variants. prusa owns `From<PrusaError>`.
- `crates/api/src/store.rs` moves to **auth**, which adds `busy_timeout`, `PRAGMA user_version` migrations covering all tables, and either a shared connection or a second connection with WAL.
- `tests/printers.rs` stays **prusa's**. prusa changes `send` to take `&AppState` and a `Role` and route through `common::authorize`. auth changes only `tests/common/mod.rs`.

**prusa, binding:**
- **Uploads:** diqwest can't stream, because it clones the request. Use a bodyless `HEAD` on the same path to get the digest challenge, build the `Authorization` header with the `digest_auth` crate, then send the stream once with `Content-Length`. Require `Content-Length` on the incoming PUT (411 otherwise) and enforce `config.max_upload_bytes` (413). Uploads use a long per-request timeout. Test that a multi-chunk streamed body arrives intact with the right length.
- `PrusaError` distinguishes timeouts. The mapping is 404/409/403 → `ApiError::Printer`, 408 or timeout → `GatewayTimeout`, everything else → 502.
- Build URLs per path segment (`Url::path_segments_mut`). After decoding, reject `..`, `.`, `\`, empty segments and NUL. Reject an upload path ending in `/`, which would create a folder.
- **Printer operation functions take `&Caller`** and call `caller.require(ROLE)` themselves, with `ROLE` a `pub const` next to each function. HTTP handlers still use the extractor matching that const. MCP tools call the same functions, so roles can't drift.
- The fake PrusaLink verifies the digest response for real (wrong password → 401) and can inject an error status per route.
- `validate_registration`: `host` must be `host[:port]` only (no `@`, `?`, `#` or whitespace).

**auth, binding:**
- **Default deny:** an `auth::require_auth` layer (`from_fn_with_state`) is applied to `/api/*` and `/mcp`, with an explicit allowlist (health, setup, `POST /api/session`). It authenticates, inserts `Caller` into request extensions, and on 401 adds `WWW-Authenticate: Bearer resource_metadata="{public_url}/.well-known/oauth-protected-resource"`. Extractors read the `Caller` from extensions when it's present. `tests/auth_roles.rs` covers auth's own routes, and one test walks every registered route without credentials and expects 401 unless the route is allowlisted.
- **Refresh grace window:** reusing a rotated refresh token within 30 s mints a *new* pair on the same grant (old successors stay valid until they expire) instead of revoking. Reuse after 30 s revokes the grant. Test two concurrent refreshes, which must both succeed.
- **Atomic invariants:** first-admin creation is a single statement or `BEGIN IMMEDIATE` transaction (two concurrent setups → exactly one admin), and so is the last-enabled-admin guard. `/api/setup`, `POST /api/session` and `/oauth/token` are rate-limited. The setup code is 128-bit random, base32.
- Unknown users get a dummy argon2 verify, so timing doesn't reveal usernames.
- Carry over from the spec: `Secure` cookies when `public_url` is https; `ss_at_`/`ss_rt_` prefixes with SHA-256 storage; lifetimes (access 1 h, refresh 30 d sliding, code 60 s single use, grant 90 d absolute); RFC 8707 `resource` recorded; `starstar-cli` redirect rules and the copy-code page restriction; the decision endpoint caps the granted role server-side at `min(chosen, user role)` and grants admin only when explicitly chosen.
- `last_used` updates are throttled to at most one write per token per minute.
- Exposes `auth::protected_resource_metadata_url(&AppState) -> String`. MCP mounts `/mcp` under `require_auth` and reads `Caller` from extensions (rmcp's streamable-HTTP service passes the HTTP request `Parts` through to tool context; the mcp part verifies this with the pinned rmcp version).

**cube:** MK4 and MINI, as in the spec. Add MK4S only if both are trivially available.

**Wave 2 briefs, to include in their part plans:**
- **cli:** warn once on stderr when sending a token to a non-loopback `http://` URL. Take an advisory file lock on `credentials.json` during refresh.
- **mcp:** `upload_file` takes base64 only, capped at `config.mcp_max_upload_bytes`.
- **web:** polling backs off when the printer errors (5 s → 30 s). The SPA fallback router is merged last so it never shadows `/api`, `/oauth`, `/mcp` or `/.well-known`. The web part adds `web/.output` and `node_modules` to `.gitignore`. `nuxt build` and typecheck join the checks.

## Revision 3 — after part-plan reviews (overrides earlier text)

- `auth::protected_resource_metadata_url(state: &AppState, path: &str) -> String`. For `/mcp` paths it advertises `/.well-known/oauth-protected-resource/mcp`, whose `resource` is `{public_url}/mcp`.
- Refresh grace mints at most one new pair per rotated token. Any further replay revokes the grant. This deliberately replaces spec resolution 5's "same successor pair", which can't be built with hashed tokens.
- The consent decision endpoint rejects a role above `min(requested, user role)`. Admin needs an explicit `allow_admin: true`.
- `auth::security_headers` sets `X-Frame-Options: DENY` and `CSP: frame-ancestors 'none'` on every response. The web part must not loosen it.
- `Config.trusted_proxies` (`STARSTAR_TRUSTED_PROXIES`): `X-Forwarded-For` is honoured only from those peers. Unset with a loopback `STARSTAR_BIND` it defaults to `127.0.0.1/32,::1/128`; any explicit value (including a non-loopback list) replaces that default. `check_config` warns when `public_url` is https and the list is empty.
- A password change revokes all of that user's grants and their other sessions.

## Review Focus

1. A token or session for a disabled or demoted user must stop working on the very next request, both on `/api` and `/mcp`. Owner: auth.
2. Two CLI processes refreshing the same refresh token at once must both succeed (grace window) and must not revoke the grant. Owner: auth (server) and cli (file lock).
3. File paths with spaces, unicode, nested folders and URL-reserved characters (`#`, `?`, `%`) must round-trip to the printer encoded correctly. `..` must be refused. Owner: prusa.
4. A printer that is unreachable or slow while a dashboard polls must return 502 or 504 quickly and never hang the API. Owner: prusa (timeouts) and web (polling backoff).
5. The OAuth authorize request must reject a redirect URI that isn't registered for that client, even if it's loopback with a different path. Loopback matching ignores only the port (RFC 8252). Owner: auth.
