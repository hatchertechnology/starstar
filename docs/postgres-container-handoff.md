# PostgreSQL and local container handoff

## Goal and decisions

Replace SQLite with PostgreSQL as Starstar's only database, then run the API and generated web UI together with PostgreSQL using Compose. There are no existing installations to migrate. Keep the option to run the API directly on the host against the Compose database for development. Thor, CloudNativePG and Tailscale deployment are subsequent work, not part of this handoff.

## Completed state

- **PostgreSQL storage (`sqlx` / `PgPool`):** Replaced `rusqlite` across `starstar-api`. Configured via `STARSTAR_DATABASE_URL`, `STARSTAR_DB_MAX_CONNECTIONS` (default `10`, range `1..=100`), and `STARSTAR_DB_CONNECT_TIMEOUT_SECS` (default `30`); startup fails clearly on missing or invalid URLs and logs redacted connection URLs without credentials.
- **Versioned migrations:** `schema_migrations` (`v1` printers, `v2` auth) executes atomically under `pg_advisory_xact_lock` scoped to `current_schema()`, preventing concurrent startup races and rejecting newer incompatible schemas.
- **Async stores and concurrency invariants:** Ported `PrinterStore` and all auth stores (`users`, `sessions`, `clients`, `codes`, `grants`) to async PostgreSQL queries. First-admin creation and last-enabled-admin update/removal serialize under a schema-scoped `pg_advisory_xact_lock`; authorization-code replay uses `SELECT ... FOR UPDATE`; OAuth refresh token rotation locks parent `oauth_grants ... FOR UPDATE` before `oauth_tokens ... FOR UPDATE OF t`.
- **Health check and graceful shutdown:** `GET /api/health` pings PostgreSQL with a 2-second timeout and returns `503` when the database is unavailable. `starstar-api` handles `SIGINT` and Unix `SIGTERM`, stops background prune tasks, drains HTTP connections cleanly, and closes the `PgPool`.
- **Container image and Compose stack:** Removed `/data` and `STARSTAR_DATABASE` from `Dockerfile` and added the `app` service to `compose.yaml` (`depends_on: db` healthy, loopback host port `127.0.0.1:8080:8080`, `STARSTAR_DATABASE_URL=postgres://starstar:starstar-dev@db:5432/starstar`). Updated `README.md` with Compose, host-run API, database reset, and `STARSTAR_TEST_DATABASE_URL` test documentation.
- **Test suite:** All unit and integration tests run against isolated per-test PostgreSQL schemas (`starstar_test_<hex>`) cleaned up via `TestSchemaGuard`, including independent-connection concurrency tests in `crates/api/tests/postgres_store.rs`.

## Verification record

- `cargo fmt --check`: passed (`0` diffs).
- `cargo clippy --workspace --all-targets`: passed (`0` warnings/errors).
- `cargo test --workspace` (against local Postgres 17 via `STARSTAR_TEST_DATABASE_URL`): `477 passed, 0 failed` (`starstar-api`: 99 unit + 240 integration; `starstar-cli`: 25 unit + 107 integration; `starstar-shared`: 6 unit), with 0 leftover `starstar_test_*` schemas verified via `psql`.
- `CI=1 NUXT_TELEMETRY_DISABLED=1 npm --prefix web test && npm --prefix web run typecheck`: `22` test files / `131` tests passed; typecheck passed.
- `podman compose down -v && podman compose up --build -d`: built `starstar-api` + generated Nuxt UI cleanly and started `db` (`postgres:17-bookworm`, healthy) and `app`.
  - Startup logs showed redacted `database: postgres://db:5432/starstar` with no credentials.
  - `GET /api/health` returned `200` (`{"status":"ok","app":"starstar","version":"0.1.0"}`); `GET /` returned `200` (`text/html`).
  - End-to-end local auth/OAuth/MCP smoke test passed (`POST /api/setup` -> `201`, `POST /api/session` -> `200`, `GET /api/me` -> `200`, `GET /api/users` -> `200`, `GET /.well-known/oauth-authorization-server` -> `200`, `POST /oauth/register` -> `201`, PKCE `GET /oauth/authorize` + `POST /oauth/authorize/decision` + `POST /oauth/token` -> `200`, `POST /mcp` `tools/list` -> `200`).
  - `podman compose stop app`: exited `0` in `0.443s` (`shutdown signal received, draining connections`, `starstar api stopped`); `GET /api/me` (`Bearer`) and `POST /api/session` continued returning `200` after `podman compose start app`.
  - Database restart recovery: `podman compose stop db` -> `GET /api/health` returned `503`; `podman compose start db` -> `GET /api/health` and `GET /api/me` recovered to `200` without restarting `app`.
  - Host API against Compose DB (`STARSTAR_DATABASE_URL=postgres://starstar:starstar-dev@127.0.0.1:54329/starstar ./target/debug/starstar-api`): `GET /api/health` and `GET /api/me` returned `200`, redacted log printed `database: postgres://127.0.0.1:54329/starstar`, and `SIGTERM` exited `0` in `0.002s`. No commands were sent to any physical printer.

## Constraints and follow-up

- Keep a single API replica for now. Pending authorization requests, rate limits and the setup code include in-memory state; PostgreSQL by itself does not make multi-replica operation safe.
- The database stores printer credentials and authentication data. Design backup, access control and secret management for the later Thor deployment. Compose credentials and plain HTTP are for local development only.
- Thor has an existing CloudNativePG operator. The later deployment should create a dedicated Starstar PostgreSQL cluster and namespace, configure off-node backups and test recovery, then expose the app through the existing Tailscale operator with TLS. Do not deploy to Thor as part of this local task.
- Repository workflow: work and commit directly on `main`; do not push unless asked.
