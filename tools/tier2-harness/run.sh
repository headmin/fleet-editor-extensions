#!/usr/bin/env bash
#
# Run the flint Tier 2 harness against Fleet's real `fleetctl gitops` command.
#
# The harness must compile INSIDE Fleet's own package, because the mock setup
# it needs (setupEmptyGitOpsMocks) lives in a _test.go and is unimportable.
# So: take the pinned Fleet module out of the module cache, make a writable
# copy, drop the harness in, and run `go test` there.
#
# Deliberately NOT a `git clone` of fleetdm/fleet. The module cache copy is
# the exact source `go.sum` verifies and the exact source the Tier 1 oracle
# links, so both tiers are guaranteed to be auditing the same Fleet. A clone
# could silently drift from the pin.
#
# Usage:
#   ./run.sh                                  # contract tests only
#   FLINT_TIER2_REPO=/path/to/repo ./run.sh   # also dry-run a real repo
#
# Requires: go. No MySQL, no Redis, no Docker.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ORACLE_DIR="$HERE/../gitops-oracle"
HARNESS="$HERE/flint_tier2_harness_test.go"

# Single source of truth for the Fleet version: the oracle's go.mod. Both
# tiers must audit the same Fleet or their verdicts are not comparable.
FLEET_VERSION="$(awk '/github.com\/fleetdm\/fleet\/v4 v/ {print $2; exit}' "$ORACLE_DIR/go.mod")"
if [[ -z "$FLEET_VERSION" ]]; then
    echo "error: could not read the Fleet version from $ORACLE_DIR/go.mod" >&2
    exit 1
fi
echo "==> Fleet $FLEET_VERSION (pinned by tools/gitops-oracle/go.mod)"

# Resolve the module in the cache, downloading it if this is a cold machine.
# GOSUMDB=off is needed only for a first download: sum.golang.org 404s on
# Fleet's go.mod. go.sum still verifies the contents.
if ! MODDIR="$(cd "$ORACLE_DIR" && go list -m -f '{{.Dir}}' github.com/fleetdm/fleet/v4 2>/dev/null)" \
   || [[ -z "$MODDIR" || ! -d "$MODDIR" ]]; then
    echo "==> downloading Fleet $FLEET_VERSION into the module cache"
    (cd "$ORACLE_DIR" && GOSUMDB=off go mod download github.com/fleetdm/fleet/v4)
    MODDIR="$(cd "$ORACLE_DIR" && go list -m -f '{{.Dir}}' github.com/fleetdm/fleet/v4)"
fi
echo "==> module cache: $MODDIR"

# The module cache is read-only (0444/0555) by design. Copy to a scratch tree
# and restore write permission so `go test` can build there.
WORK="$(mktemp -d)"
cleanup() { chmod -R u+w "$WORK" 2>/dev/null || true; rm -rf "$WORK"; }
trap cleanup EXIT

echo "==> staging a writable copy"
cp -R "$MODDIR/." "$WORK/"
chmod -R u+w "$WORK"

cp "$HARNESS" "$WORK/cmd/fleetctl/fleetctl/"

echo "==> go test ./cmd/fleetctl/fleetctl -run 'TestTier2'"
cd "$WORK"
# -count=1 defeats the test cache: the harness asserts on EXTERNAL state (the
# repo under FLINT_TIER2_REPO), which Go's cache cannot see change.
go test ./cmd/fleetctl/fleetctl -run 'TestTier2' -count=1 "$@"
