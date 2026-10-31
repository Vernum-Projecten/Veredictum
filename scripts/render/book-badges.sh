#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Draw the book's badge gallery with the instrument itself.
#
# The Badges page shows what each badge looks like, and every image on it is
# `veredictum badge` run over a committed fixture, never a drawing. The
# fixtures are the documents the badge golden tests in
# `app/veredictum/tests/it/badges.rs` draw from:
#
#   passing/      the passing result, judged against the release
#   failing/      the failing result
#   development/  the passing result, judged against the development snapshot
#   perf/         `app/veredictum/examples/perf-result.example.json` (a knee
#                 bracket) as `bracket.svg`, and the same record read as a
#                 lower bound of a pinned workload as `lower-bound.svg`
#
# The three results documents and the lower-bound record are pinned as insta
# snapshots by `the_badge_fixtures_are_pinned_as_documents`; this script reads
# the snapshot body (everything after the second `---` line), so the gallery
# and the golden tests can never draw from different documents. Only the SVGs
# are kept: the page shows images, and the endpoint `.json` files are pinned by
# the tests already.
#
# Usage:
#   scripts/render/book-badges.sh            # write website/book/src/badges
#   scripts/render/book-badges.sh <out-dir>  # write the gallery elsewhere
#     Reads `target/debug/veredictum` when that binary exists, otherwise
#     builds through `cargo run --quiet --locked`.
set -euo pipefail
cd "$(dirname "$0")/../.."

OUT="${1:-website/book/src/badges}"
SNAPSHOTS=app/veredictum/tests/it/snapshots
PERF_EXAMPLE=app/veredictum/examples/perf-result.example.json

if [[ $# -gt 1 ]]; then
  echo "usage: $0 [<out-dir>]" >&2
  exit 2
fi

veredictum() {
  if [[ -x target/debug/veredictum ]]; then
    target/debug/veredictum "$@"
  else
    cargo run --quiet --locked -- "$@"
  fi
}

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# snapshot_body <snapshot> <file>: the document an insta snapshot pins.
snapshot_body() {
  if [[ ! -f "$1" ]]; then
    echo "book-badges: missing fixture snapshot $1" >&2
    exit 1
  fi
  awk 'fences >= 2 { print; next } /^---$/ { fences++ }' "$1" >"$2"
}

# draw <document> <dir>: every badge of the document, SVGs only.
draw() {
  mkdir -p "$2"
  veredictum badge --results "$1" --out "$work/drawn" >/dev/null
  cp "$work"/drawn/*.svg "$2"/
  rm -rf "$work/drawn"
}

rm -rf "$work/out"
for kind in passing failing development; do
  snapshot_body "$SNAPSHOTS/it__badges__${kind}-results.json.snap" "$work/$kind.json"
  draw "$work/$kind.json" "$work/out/$kind"
done

mkdir -p "$work/out/perf"
draw "$PERF_EXAMPLE" "$work/perf-bracket"
mv "$work/perf-bracket/perf.svg" "$work/out/perf/bracket.svg"
snapshot_body "$SNAPSHOTS/it__badges__perf-lower-bound-record.json.snap" "$work/lower-bound.json"
draw "$work/lower-bound.json" "$work/perf-lower"
mv "$work/perf-lower/perf.svg" "$work/out/perf/lower-bound.svg"

rm -rf "$OUT"
mkdir -p "$(dirname "$OUT")"
cp -R "$work/out" "$OUT"
echo "book-badges: wrote $(find "$OUT" -name '*.svg' | wc -l | tr -d ' ') badges under $OUT"
