#!/usr/bin/env bash
# Fail-closed gate before every apply of deploy/thor (first deploy and upgrades).
#
#   set -o pipefail
#   deploy/thor/preflight.sh thor [verified-rollback-image|--first-deploy-retry] | tee "preflight-$(date +%Y%m%d-%H%M%S).log"
#
# Exits non-zero, and prints "PREFLIGHT FAILED: ..." on stderr, unless:
#   1. the first argument is the kube context "thor";
#   2. the rendered Deployment starstar-api has exactly one image and it is
#      ghcr.io/hatchertechnology/starstar@sha256:<64 hex>, not all zeros;
#   3. the registry returns that digest's manifest WITHOUT credentials and it
#      provides linux/amd64;
#   4. context "thor" reaches node "thor" (amd64).
# Prints the Ready pod spec image (or the supplied verified record) and the
# image that will be applied. It never applies or changes anything.
set -euo pipefail

fail() { echo "PREFLIGHT FAILED: $*" >&2; exit 1; }

[ "$#" -ge 1 ] && [ "$#" -le 2 ] || fail "usage: $0 thor [verified-rollback-image|--first-deploy-retry]"
CTX=$1
[ "$CTX" = thor ] || fail "context must be 'thor', got '$CTX'"

for tool in kubectl yq jq skopeo; do
  command -v "$tool" >/dev/null || fail "$tool not found"
done

DIR=$(cd "$(dirname "$0")" && pwd)
REPO=ghcr.io/hatchertechnology/starstar
ZERO=sha256:0000000000000000000000000000000000000000000000000000000000000000

# Kustomize rendering is offline; --context has no effect here.
RENDERED=$(kubectl kustomize "$DIR") || fail "kustomize render failed"

IMAGES=$(printf '%s\n' "$RENDERED" | yq -N \
  'select(.kind == "Deployment" and .metadata.name == "starstar-api")
   | (.spec.template.spec.initContainers // []) + .spec.template.spec.containers
   | .[].image')
[ -n "$IMAGES" ] || fail "no Deployment starstar-api in the render"
[ "$(printf '%s\n' "$IMAGES" | wc -l)" -eq 1 ] || fail "expected one image, got: $IMAGES"
IMAGE=$IMAGES

[[ $IMAGE =~ ^ghcr\.io/hatchertechnology/starstar@sha256:[0-9a-f]{64}$ ]] \
  || fail "image is not $REPO@sha256:<64 hex>: $IMAGE"
[ "${IMAGE#*@}" != "$ZERO" ] || fail "digest is still the all-zero placeholder"

RAW=$(skopeo inspect --raw --no-creds "docker://$IMAGE") \
  || fail "anonymous registry lookup failed for $IMAGE (package private or digest wrong?)"

if printf '%s' "$RAW" | jq -e 'has("manifests")' >/dev/null; then
  CHILD=$(printf '%s' "$RAW" | jq -r '[.manifests[] | select(.platform.os == "linux" and .platform.architecture == "amd64") | .digest] | first // empty')
  [[ $CHILD =~ ^sha256:[0-9a-f]{64}$ ]] || fail "image index has no valid linux/amd64 manifest digest"
  INSPECT_IMAGE="$REPO@$CHILD"
else
  INSPECT_IMAGE=$IMAGE
fi
CFG=$(skopeo inspect --no-creds "docker://$INSPECT_IMAGE") || fail "anonymous config lookup failed for $INSPECT_IMAGE"
printf '%s' "$CFG" | jq -e '.Os == "linux" and .Architecture == "amd64"' >/dev/null \
  || fail "selected image is not linux/amd64"

ARCH=$(kubectl --context "$CTX" get node thor -o jsonpath='{.status.nodeInfo.architecture}') \
  || fail "context $CTX cannot read node thor"
[ "$ARCH" = amd64 ] || fail "node thor reports architecture '$ARCH'"

# A template can already contain a failed rollout. Capture the pinned image
# from a Ready pod spec and retain it in the deploy record.
NS=$(kubectl --context "$CTX" get namespace starstar --ignore-not-found -o name) || fail "cannot read namespace"
DEPLOY=
if [ -n "$NS" ]; then
  DEPLOY=$(kubectl --context "$CTX" -n starstar get deploy starstar-api --ignore-not-found -o name) \
    || fail "cannot read the current Deployment"
fi
ROLLBACK=none
if [ -n "$DEPLOY" ]; then
  PODS=$(kubectl --context "$CTX" -n starstar get pods -l app.kubernetes.io/name=starstar,app.kubernetes.io/component=api -o json) \
    || fail "cannot read API pods"
  IMAGES_READY=$(printf '%s' "$PODS" | jq -r '[.items[] | select(.metadata.deletionTimestamp == null) | select(any(.status.conditions[]?; .type == "Ready" and .status == "True")) | select(any(.status.containerStatuses[]?; .name == "api" and .ready == true)) | .spec.containers[] | select(.name == "api") | .image] | unique | .[]')
  if [ -n "$IMAGES_READY" ]; then
    [ "$(printf '%s\n' "$IMAGES_READY" | wc -l)" -eq 1 ] || fail "multiple Ready pod images; resolve before deploying"
    ROLLBACK=$IMAGES_READY
    [[ $ROLLBACK =~ ^ghcr\.io/hatchertechnology/starstar@sha256:[0-9a-f]{64}$ ]] \
      && [ "${ROLLBACK#*@}" != "$ZERO" ] || fail "Ready pod has an invalid pinned image"
    echo "rollback source: Ready pod spec"
  elif [ "$#" -eq 2 ]; then
    if [ "$2" = --first-deploy-retry ]; then
      echo "first-deploy retry explicitly requested; confirm this Deployment has never had a Ready pod"
    else
      ROLLBACK=$2
      [[ $ROLLBACK =~ ^ghcr\.io/hatchertechnology/starstar@sha256:[0-9a-f]{64}$ ]] \
        && [ "${ROLLBACK#*@}" != "$ZERO" ] || fail "invalid verified rollback image"
      echo "rollback source: previous verified deployment record"
    fi
  else
    fail "no Ready API pod; supply a verified rollback image or --first-deploy-retry after confirming deploy history"
  fi
fi

echo "context:        $CTX"
echo "rollback image: $ROLLBACK"
echo "apply image:    $IMAGE"
echo "PREFLIGHT OK"
