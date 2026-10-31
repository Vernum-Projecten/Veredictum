#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The SMART lane (#921, #745): every SMART on openEHR document runs end to end
# against a CDR in the SMART posture, and no row is left unjudged.
#
# It brings up deploy/docker/e2e-ferroehr-smart.yml (FerroEHR as the resource
# server, pinned by digest, trusting the committed static test issuer), runs
# `veredictum test` over every document under catalogue/cases/smart/ with the
# Platform base URL and the environment's statement and IXIT, minting each
# row's scoped token with the committed test key, and holds the run to:
#
#   - exit status 0: no row counts against the server and none is
#     inconclusive;
#   - every row of those documents run: none skipped, so none is
#     `not-declared`;
#   - at least one row per document.
#
# A red row here is attributed before anything changes (the attribution law),
# with the run's results.json and recording.har, which the lane writes into
# its output directory and CI keeps on failure. The test issuer's key is
# committed test material, so the recording holds nothing secret.
#
# No openEHR specification governs this lane. It is our own design; the
# behaviour it exercises is ITS-REST docs/smart_app_launch/.
#
# Usage:
#   scripts/smart-lane.sh [out-dir]   # default target/smart-lane
#
# Env:
#   VEREDICTUM_BIN        the engine binary (default target/debug/veredictum,
#                         built with `cargo build --locked` when missing)
#   SMART_LANE_KEEP_UP    skip the teardown (local debugging)
#   FERROEHR_SMART_PORT   the host port (default 8094)
set -euo pipefail
cd "$(dirname "$0")/.."

readonly COMPOSE_FILE=deploy/docker/e2e-ferroehr-smart.yml
readonly PROJECT=veredictum-smart-lane
readonly KEY=catalogue/keys/smart-test-issuer/cnf-smart-test.key.pem
out="${1:-target/smart-lane}"
port="${FERROEHR_SMART_PORT:-8094}"
bin="${VEREDICTUM_BIN:-target/debug/veredictum}"

compose() {
  FERROEHR_SMART_PORT="$port" docker compose -p "$PROJECT" -f "$COMPOSE_FILE" "$@"
}

teardown() {
  if [[ -z "${SMART_LANE_KEEP_UP:-}" ]]; then
    compose down --volumes --remove-orphans >/dev/null 2>&1 || true
  fi
}

if [[ ! -x "$bin" ]]; then
  cargo build --locked --quiet --bin veredictum
  bin=target/debug/veredictum
fi

documents=()
for file in catalogue/cases/smart/*.arazzo.yaml; do
  name="$(basename "$file" .arazzo.yaml)"
  documents+=("$name")
done
[[ "${#documents[@]}" -gt 0 ]] || { echo "smart-lane: no document under catalogue/cases/smart/" >&2; exit 1; }

trap teardown EXIT
compose up --detach --wait
base="http://127.0.0.1:${port}/ferroehr/rest"

rm -rf "$out"
mkdir -p "$out"
args=(test --root catalogue --auth basic --base-url "${base}/openehr/v1" --platform-url "$base"
  --statement deploy/docker/smart/statement.json --ixit deploy/docker/smart/ixit.json --out "$out")
for name in "${documents[@]}"; do
  args+=(--document "$name")
done

code=0
VEREDICTUM_CDR_USER=ferroehr VEREDICTUM_CDR_PASSWORD=ferroehr \
  VEREDICTUM_SCOPED_TOKEN_KEY_FILE="$KEY" "$bin" "${args[@]}" > "$out/run.log" 2>&1 || code=$?
tail -n 3 "$out/run.log"

fail=0
if [[ "$code" != 0 ]]; then
  echo "smart-lane: the run exited ${code}, not 0; attribute every red row in ${out}/results.json" >&2
  fail=1
fi
results="$out/results.json"
if [[ ! -f "$results" ]]; then
  echo "smart-lane: the run wrote no results; ${out}/run.log says why" >&2
  exit 1
fi
for name in "${documents[@]}"; do
  rows="$(jq --arg d "$name/" '[.rows[] | select(.case | startswith($d))] | length' "$results")"
  skipped="$(jq -r --arg d "$name/" '.rows[] | select((.case | startswith($d)) and .status == "skipped")
    | "\(.case) row \(.row): \(.skip.reason)"' "$results")"
  if [[ "$rows" == 0 ]]; then
    echo "smart-lane: ${name} ran no row" >&2
    fail=1
  fi
  if [[ -n "$skipped" ]]; then
    echo "smart-lane: ${name} skipped rows, none of which may be:" >&2
    printf '  %s\n' "$skipped" >&2
    fail=1
  fi
  echo "smart-lane: ${name}: ${rows} row(s)"
done
jq -r '.rows[] | "\(.status)\t\(.case) row \(.row)"' "$results"
if [[ "$fail" -ne 0 ]]; then
  exit 1
fi
echo "smart-lane: every SMART document ran end to end, with no row skipped"
