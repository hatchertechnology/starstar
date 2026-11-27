#!/usr/bin/env bash
# Run from any directory; every gate must pass before the next stage.
set -euo pipefail
cd "$(dirname "$0")/../.."
[ "$#" -ge 1 ] && [ "$#" -le 2 ] && [ "$1" = thor ] || { echo "usage: $0 thor [verified-rollback-image|--first-deploy-retry]" >&2; exit 1; }
deploy/thor/preflight.sh "$@"
kubectl --context thor apply -f deploy/thor/namespace.yaml
kubectl --context thor apply -f deploy/thor/networkpolicy.yaml
kubectl --context thor apply -k deploy/thor/ --dry-run=server
kubectl --context thor apply -f deploy/thor/database.yaml
kubectl --context thor -n starstar wait cluster/starstar-pg --for=condition=Ready --timeout=10m
kubectl --context thor -n starstar get secret starstar-pg-app -o jsonpath='{.data.uri}' | grep -q .
kubectl --context thor -n starstar get secret starstar-pg-ca -o jsonpath='{.data.ca\.crt}' | grep -q .
kubectl --context thor -n starstar get configmap cnpg-default-monitoring -o name
kubectl --context thor apply -k deploy/thor/
kubectl --context thor -n starstar rollout status deploy/starstar-api --timeout=5m
echo 'FIRST DEPLOY OK; complete post-deploy verification before setup'
