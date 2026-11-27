# Build the static Nuxt application and the API into one runtime image.
#
# Base images are pinned to an exact tag plus its multi-arch index digest. Bump tag and
# digest together; resolve a digest with:
#   skopeo inspect --format '{{.Digest}}' docker://docker.io/library/<image>:<tag>
ARG NODE_IMAGE=docker.io/library/node:24.22.3-trixie-slim@sha256:4d00acb4bcc42c454c4eb801526f7ad7f772ea3bd4fd168c5cc2e384abcbb9d4
ARG RUST_IMAGE=docker.io/library/rust:1.100.0-trixie@sha256:e84068000361bf3d4a5ba2b7647f62893528b1eb9f847d7a23b493f5ce8ea254
ARG RUNTIME_IMAGE=docker.io/library/debian:trixie-20261123-slim@sha256:8aa5a7bf5a1955bddbe7d74fcdd0b91ab30e74362a8b73af672b0481b35b44a0

FROM ${NODE_IMAGE} AS web
ENV CI=1 NUXT_TELEMETRY_DISABLED=1
WORKDIR /src/web
# Lockfile-only layer: npm ci reruns only when package.json or package-lock.json change.
COPY web/package.json web/package-lock.json ./
RUN npm ci --ignore-scripts
# Only the allowlisted web files reach this COPY (see .dockerignore).
COPY web/ ./
RUN npx --no-install nuxt prepare && npm run generate

FROM ${RUST_IMAGE} AS api
ENV CARGO_TERM_COLOR=never CARGO_INCREMENTAL=0
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo build --locked --release -p starstar-api \
    && install -D -m 0755 target/release/starstar-api /out/starstar-api

FROM ${RUNTIME_IMAGE} AS runtime
LABEL org.opencontainers.image.title="starstar" \
      org.opencontainers.image.description="HTTP API and web UI for the starstar 3D printer management system" \
      org.opencontainers.image.source="https://github.com/hatchertechnology/starstar" \
      org.opencontainers.image.url="https://github.com/hatchertechnology/starstar" \
      org.opencontainers.image.licenses="AGPL-3.0-only"
# ca-certificates: sqlx verifies PostgreSQL TLS against the system roots. The package
# version floats within the pinned Debian release; it is refreshed only when this layer
# is rebuilt (cache miss or a base-digest bump).
# hadolint ignore=DL3008
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 starstar \
    && useradd --system --uid 10001 --gid 10001 --no-create-home \
        --home-dir /nonexistent --shell /usr/sbin/nologin starstar
WORKDIR /app
# Root-owned and read-only to the runtime user: the process cannot modify its own code.
COPY --from=api /out/starstar-api /usr/local/bin/starstar-api
COPY --from=web /src/web/.output/public /app/public
COPY LICENSE /usr/share/doc/starstar/LICENSE
ENV STARSTAR_BIND=0.0.0.0:8080 STARSTAR_WEB_DIR=/app/public
USER 10001:10001
EXPOSE 8080
# The binary is PID 1 and handles SIGTERM/SIGINT with a graceful drain once serving.
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/starstar-api"]
