#!/usr/bin/env bash
# Run compose.yaml with a prebuilt image under its own project name and host ports, and
# assert the real health transitions and effective hardening through the Compose provider.
#
# Usage: scripts/compose-check.sh ENGINE IMAGE     (ENGINE: docker or podman)
#   COMPOSE_PROJECT     project name (default starstar-verify-<random>); must not exist yet
#   STARSTAR_HTTP_PORT  app host port (default 18080)
#   STARSTAR_DB_PORT    db host port (default 15432)
#
# Removes only the project it created (containers, network and its own volume).
set -euo pipefail
cd "$(dirname "$0")/.."

engine=$1
image=$2
project=${COMPOSE_PROJECT:-starstar-verify-$(od -An -N4 -tx4 /dev/urandom | tr -d ' ')}
[[ $project =~ ^[a-z0-9][a-z0-9_-]*$ ]] || { echo 'invalid project name' >&2; exit 1; }
export STARSTAR_HTTP_PORT=${STARSTAR_HTTP_PORT:-18080}
export STARSTAR_DB_PORT=${STARSTAR_DB_PORT:-15432}
export STARSTAR_SETUP_TOKEN="verify-only-setup-token-${project}"
export STARSTAR_DB_PASSWORD="verify-only-${project}"
export STARSTAR_DATABASE_URL="postgres://starstar:${STARSTAR_DB_PASSWORD}@db:5432/starstar"
work=$(mktemp -d)
touch "$work/empty.env"
cat >"$work/override.yaml" <<EOF
services:
  app:
    image: ${image}
    pull_policy: never
EOF

compose() { "$engine" compose --env-file "$work/empty.env" -p "$project" -f compose.yaml -f "$work/override.yaml" "$@"; }
cid() {
  "$engine" ps -aq --filter "label=com.docker.compose.project=${project}" \
    --filter "label=com.docker.compose.service=$1"
}

# Check all project resources before installing the destructive cleanup trap. Check
# names as well as labels because provider versions differ in their labeling.
containers=$("$engine" ps -aq --filter "label=com.docker.compose.project=${project}")
volumes=$("$engine" volume ls -q)
networks=$("$engine" network ls --format '{{.Name}}')
project_volumes=$("$engine" volume ls -q --filter "label=com.docker.compose.project=${project}")
project_networks=$("$engine" network ls -q --filter "label=com.docker.compose.project=${project}")
if [ -n "$containers$project_volumes$project_networks" ] || printf '%s\n' "$volumes" "$networks" | grep -Eq "^${project}([_-]|$)"; then
  echo "compose project ${project} already exists; refusing to touch it" >&2
  rm -rf "$work"
  exit 1
fi
cleanup() {
  local status=$?
  compose down -v --remove-orphans >/dev/null 2>&1 || true
  rm -rf "$work"
  exit "$status"
}
trap cleanup EXIT

health() { "$engine" inspect -f '{{.State.Health.Status}}' "$(cid app)"; }
wait_health() {
  local want=$1 limit=$2
  for _ in $(seq 1 "$limit"); do
    [ "$(health)" = "$want" ] && return 0
    sleep 1
  done
  echo "app health is '$(health)', wanted '${want}' within ${limit}s" >&2
  return 1
}

compose up -d --no-build
wait_health healthy 150

curl -fsS --max-time 5 -o "$work/health" "http://127.0.0.1:${STARSTAR_HTTP_PORT}/api/health"
grep -q '"status":"ok"' "$work/health"
app=$(cid app)
"$engine" exec "$app" grep -qx $'NoNewPrivs:\t1' /proc/1/status
"$engine" exec "$app" grep -qx $'CapEff:\t0000000000000000' /proc/1/status
test "$("$engine" inspect -f '{{.HostConfig.ReadonlyRootfs}}' "$app")" = true

compose stop db
wait_health unhealthy 90      # 3 retries x 10 s interval + 5 s timeout, with margin
compose start db
wait_health healthy 120

compose stop app
test "$("$engine" inspect -f '{{.State.ExitCode}}' "$app")" = 0
echo "compose check passed (${engine}, project ${project})"
