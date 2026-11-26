# PostgreSQL and local container handoff

## Goal and decisions

Replace SQLite with PostgreSQL as Starstar's only database, then run the API and generated web UI together with PostgreSQL using Compose. There are no existing installations to migrate. Keep the option to run the API directly on the host against the Compose database for development. Thor, CloudNativePG and Tailscale deployment are subsequent work, not part of this handoff.

## Current state

- Commit `9dfcba9` added a multi-stage `Dockerfile`, `.dockerignore`, and `compose.yaml` containing a PostgreSQL 17 development service. The image builds the API and generated Nuxt UI, runs as a non-root user, and was smoke-tested with the **existing SQLite backend** (`/api/health` and `/` returned success).
- Compose starts **only PostgreSQL**; the API does not connect to it yet. The local development database was started and reported healthy. Its port is bound to `127.0.0.1:54329`; credentials in Compose are development-only, and its named volume persists across ordinary restarts.
- The Dockerfile currently sets `STARSTAR_DATABASE=/data/starstar.db`. This is temporary SQLite configuration and must be removed or replaced during the conversion. The API currently needs `STARSTAR_ALLOW_INSECURE_HTTP=1` for non-loopback binding with an HTTP public URL. For Compose, publish the app only on loopback and set its public URL to the host-facing URL.
- The container smoke test required a forced stop after the API did not exit promptly on SIGTERM. Add graceful shutdown and verify it.
- The API and UI are not yet running together via Compose. No PostgreSQL storage implementation or PostgreSQL integration tests have been added.

## Implementation plan

1. **Design the database boundary and configuration.** Replace the SQLite path setting with a documented PostgreSQL connection setting. Fail clearly for a missing or invalid connection string; do not log credentials. Use a bounded connection pool and asynchronous database access, or isolate blocking access from the async runtime. Set startup behavior and timeouts for an unavailable database. Select and document a versioned migration mechanism; ensure concurrent starts cannot race migrations and reject incompatible newer schemas.
2. **Port schema and storage.** Convert the printer registry and all auth stores, including users, sessions, OAuth clients, grants, codes and tokens. Replace SQLite-specific types, placeholders, `PRAGMA user_version`, insert/error handling and transaction semantics. Preserve foreign keys, cascades, uniqueness, time values, binary token hashes and existing externally visible behavior. Update startup, error mapping, dependencies and documentation; remove SQLite-specific implementation and tests. No data migration is needed.
3. **Preserve security invariants under concurrency.** PostgreSQL transactions do not automatically provide SQLite's `IMMEDIATE` locking semantics. Explicitly design and test serialization for first-admin creation and last-enabled-admin protection, authorization-code exchange and replay, and refresh-token rotation/reuse. Use appropriate database locks or atomic updates and handle conflicts consistently. Ensure tests exercise independent connections, not just one shared connection.
4. **Move tests to PostgreSQL.** Replace in-memory SQLite fixtures with isolated PostgreSQL test databases or schemas, safe for parallel tests. Document the test database URL and lifecycle. Retain behavioral coverage and add meaningful integration tests for migrations, rollback, persistence, constraints, and the concurrent security cases above. Verify the API fails cleanly when PostgreSQL is unavailable and recovers according to the chosen design.
5. **Finish containerization and Compose.** Update the image configuration for PostgreSQL; remove the SQLite data directory if no longer needed. Add the app service with a database URL using the Compose service name, a database health dependency and retry behavior, loopback-only host port, correct public URL, and development-only settings. Keep secrets out of the image and repository outside explicitly local development defaults. Add SIGTERM handling and graceful HTTP shutdown. Document startup, setup, logs, stopping, database reset and running the API outside the container.
6. **Verify the complete local workflow.** Build from a clean checkout; start Compose; confirm database health, API health and UI rendering; create the first admin and sign in; exercise a representative API and OAuth/MCP flow; restart services and confirm persistence; verify shutdown and recovery after a database restart. Run appropriate Rust and web checks and record exact commands and results. Avoid sending any commands to a physical printer during verification without explicit authorization.

## Constraints and follow-up

- Keep a single API replica for now. Pending authorization requests, rate limits and the setup code include in-memory state; PostgreSQL by itself does not make multi-replica operation safe.
- The database stores printer credentials and authentication data. Design backup, access control and secret management for the later Thor deployment. Compose credentials and plain HTTP are for local development only.
- Thor has an existing CloudNativePG operator. The later deployment should create a dedicated Starstar PostgreSQL cluster and namespace, configure off-node backups and test recovery, then expose the app through the existing Tailscale operator with TLS. Do not deploy to Thor as part of this local task.
- Repository workflow: work and commit directly on `main`; do not push unless asked.
