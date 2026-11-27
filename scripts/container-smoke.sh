#!/usr/bin/env bash
# Smoke-test a starstar image: start it with the Thor pod's restrictions on the host
# network, check /api/health, the UI, the runtime user and the effective privileges, then
# check that SIGTERM drains and exits 0.
#
# Usage: scripts/container-smoke.sh IMAGE DATABASE_URL
#   CONTAINER_ENGINE  docker (default) or podman
#   SMOKE_PORT        loopback port for the app (default 18080)
#
# Prints only allowlisted log lines. Removes only the container it created.
set -euo pipefail

image=$1
db_url=$2
engine=${CONTAINER_ENGINE:-docker}
port=${SMOKE_PORT:-18080}
name=starstar-smoke-$(od -An -N4 -tx4 /dev/urandom | tr -d ' ')
url=http://127.0.0.1:${port}
work=$(mktemp -d)
existing=$("$engine" ps -aq --filter "name=^${name}$")
if [ -n "$existing" ]; then
  echo "container ${name} already exists; refusing to touch it" >&2
  rm -rf "$work"
  exit 1
fi

diagnostics() {
  # Do not print app logs: errors may contain credentials supplied by the caller.
  "$engine" inspect -f 'state={{.State.Status}} exit={{.State.ExitCode}}' "$name" || true
}
cleanup() {
  local status=$?
  if [ "$status" -ne 0 ]; then diagnostics; fi
  "$engine" rm -f "$name" >/dev/null 2>&1 || true
  rm -rf "$work"
  exit "$status"
}
trap cleanup EXIT

# A fixed, throwaway setup token stops the binary from logging a random setup code.
"$engine" run -d --name "$name" --network host \
  --read-only --tmpfs /tmp:rw,noexec,nosuid,nodev,size=16m \
  --cap-drop ALL --security-opt no-new-privileges \
  -e STARSTAR_DATABASE_URL="$db_url" \
  -e STARSTAR_LIBRARY_DIR=/tmp/starstar-library \
  -e STARSTAR_BIND="127.0.0.1:${port}" \
  -e STARSTAR_PUBLIC_URL="$url" \
  -e STARSTAR_ALLOW_INSECURE_HTTP=1 \
  -e STARSTAR_SETUP_TOKEN="smoke-only-setup-token-${name}" \
  "$image" >/dev/null

ready=
for _ in $(seq 1 90); do
  if curl -fsS --max-time 3 -o "$work/health" "$url/api/health"; then ready=1; break; fi
  [ "$("$engine" inspect -f '{{.State.Running}}' "$name")" = true ] || break
  sleep 1
done
[ -n "$ready" ] || { echo "app did not become healthy" >&2; exit 1; }

# Bodies go to files first: `curl | grep -q` can fail under pipefail after a match.
grep -q '"status":"ok"' "$work/health"
curl -fsS --max-time 5 -o "$work/index" "$url/"
grep -qi '<!doctype html' "$work/index"

test "$("$engine" exec "$name" id -u):$("$engine" exec "$name" id -g)" = "10001:10001"
"$engine" exec "$name" grep -qx $'NoNewPrivs:\t1' /proc/1/status
"$engine" exec "$name" grep -qx $'CapEff:\t0000000000000000' /proc/1/status
test "$("$engine" inspect -f '{{index .Config.Labels "org.opencontainers.image.source"}}' "$image")" \
  = "https://github.com/hatchertechnology/starstar"
"$engine" logs "$name" >"$work/log" 2>&1
grep -q '^starstar api listening' "$work/log"

# SIGTERM to PID 1 must drain and exit 0 well inside the grace period.
"$engine" stop -t 20 "$name" >/dev/null
test "$("$engine" inspect -f '{{.State.ExitCode}}' "$name")" = 0
"$engine" logs "$name" >"$work/log" 2>&1
grep -q '^starstar api stopped' "$work/log"

echo "smoke test passed: $image"
