#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The committed example record is what its generator writes (#466, #888).
#
# `app/veredictum/examples/perf-result.example.json` is produced by
# `cargo run --example make_example_perf_result`. The views and charts of a
# perf-result are pinned against it, so a stale copy would pin them against
# numbers no build derives, and it embeds the crate version, so it drifts at
# every release unless something holds it. The published schema set has
# `schema_drift` in the Rust suite for this property; a cargo example cannot
# have one, because it is a binary no integration test can import (the Book,
# ch11.3), so the equivalent gate is this script.
#
# It renders each document to a temporary path — each generator takes one
# optional argument for that — and byte-compares the committed copy, so a check
# never writes into the tree.
#
# Usage: scripts/checks/example-results-drift.sh
set -euo pipefail
cd "$(dirname "$0")/../.."

rendered="$(mktemp)"
trap 'rm -f "$rendered"' EXIT

status=0
for pair in make_example_perf_result:perf-result.example.json; do
  generator="${pair%%:*}"
  example="app/veredictum/examples/${pair#*:}"
  cargo run --quiet --locked --example "$generator" -- "$rendered" >/dev/null
  if ! diff -u "$example" "$rendered" --label "$example (committed)" --label "$example (generated)"; then
    echo "::error::${example} is stale — regenerate it with \`cargo run --example ${generator}\` and commit the result" >&2
    status=1
    continue
  fi
  echo "example-results-drift: ${example} is byte-identical to its generator's output — OK."
done
exit "$status"
