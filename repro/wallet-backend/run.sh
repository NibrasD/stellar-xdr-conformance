#!/usr/bin/env bash
# Runs wallet-backend's own participant code, unmodified, on every replay
# scene and compares what it records with the participants derived from the
# ledger meta (corpus/scenes.json).
#
#   repro/wallet-backend/run.sh [commit]
#   repro/wallet-backend/run.sh --control [commit]
#
# --control applies control-677.patch first: a small local change that reads
# participants from every change in the meta, as wallet-backend#677
# proposes. It is not an upstream fix. It shows the comparison can tell a
# correct participant set from a wrong one: the scene must pass with it and
# be dropped without it.
#
# This is a component-level reproduction: it calls
# ParticipantsProcessor.GetTransactionParticipants directly, without
# starting the service or a database. The full replay harness, which drives
# the running indexer, is a deliverable of the proposal.
set -euo pipefail

CONTROL=""
if [ "${1:-}" = "--control" ]; then
  CONTROL=1
  shift
fi
COMMIT="${1:-3e22515a44979fdc3a558fda3991ca2b48e38cef}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
work="${WALLET_BACKEND_DIR:-$root/target/wallet-backend}"

if [ ! -d "$work/.git" ]; then
  git clone --quiet --filter=blob:none https://github.com/stellar/wallet-backend "$work"
fi
git -C "$work" fetch --quiet origin "$COMMIT" 2>/dev/null || true
git -C "$work" checkout --quiet --force "$COMMIT"
git -C "$work" clean --quiet -fdx internal/indexer/processors
if [ -n "$CONTROL" ]; then
  git -C "$work" apply "$here/control-677.patch"
fi

cp "$here/scene_participants_test.go.txt" "$work/internal/indexer/processors/xdrc_scene_participants_test.go"
recorded="$(mktemp)"
(
  cd "$work"
  XDRC_CORPUS="$root/corpus" XDRC_OUT="$recorded" \
    go test ./internal/indexer/processors -run '^TestXDRConformanceScenes$' -count=1 >/dev/null
)
rm -f "$work/internal/indexer/processors/xdrc_scene_participants_test.go"
git -C "$work" checkout --quiet --force "$COMMIT"

mkdir -p "$root/results"
name="wallet-backend-${COMMIT:0:7}${CONTROL:+-control}"
node "$here/compare.mjs" "$root/corpus/scenes.json" "$recorded" "$COMMIT" ${CONTROL:+--control} \
  > "$root/results/$name.json"
rm -f "$recorded"
cat "$root/results/$name.json"
