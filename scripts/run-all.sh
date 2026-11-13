#!/usr/bin/env bash
# Runs the whole pipeline locally, in the same order as CI:
# corpus reproducibility, manifest validation, every SDK runner, report.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

echo "== generator: corpus reproducibility"
cargo run --quiet -p generator -- --check

echo "== tools: manifest validation"
(cd tools && npm ci --silent --no-audit --no-fund)
node tools/validate-manifest.mjs

rm -rf results
mkdir -p results

echo "== runner: rust"
cargo run --quiet -p conformance-runner-rust -- --corpus corpus --out results

echo "== runner: go"
(cd runners/go && go run . -corpus ../../corpus -out ../../results)

echo "== runner: js"
(cd runners/js && npm ci --silent --no-audit --no-fund && node run.mjs)

echo "== runner: python"
(
  cd runners/python
  [ -d .venv ] || python3 -m venv .venv
  .venv/bin/pip install --quiet --disable-pip-version-check -r requirements.txt
  .venv/bin/python run.py
)

echo "== report"
node tools/report.mjs > /dev/null
echo "REPORT.md written; every outcome matches expectations/known-outcomes.json"
