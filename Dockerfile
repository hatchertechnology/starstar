# Build the static Nuxt application and the API into one runtime image.
FROM node:24-bookworm-slim AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run generate

FROM rust:1.90-bookworm AS api
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo build --locked --release -p starstar-api

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home starstar \
    && mkdir /data && chown starstar:starstar /data
WORKDIR /app
COPY --from=api /src/target/release/starstar-api /usr/local/bin/starstar-api
COPY --from=web /src/web/.output/public /app/public
ENV STARSTAR_BIND=0.0.0.0:8080 STARSTAR_WEB_DIR=/app/public STARSTAR_DATABASE=/data/starstar.db
USER 10001:10001
EXPOSE 8080
ENTRYPOINT ["starstar-api"]
