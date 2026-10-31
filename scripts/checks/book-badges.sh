#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The book's badge gallery is what the current binary draws.
#
# `website/book/src/badges/` holds the SVGs the Badges page shows, drawn by
# `scripts/render/book-badges.sh` with `veredictum badge` from the badge golden
# tests' fixtures. This guard redraws them into a temporary directory and
# compares the whole tree, so a changed badge, a badge added or removed, or a
# stray file under the directory fails the build and the page never shows a
# badge the instrument no longer draws.
#
# Usage: scripts/checks/book-badges.sh
#   Reads `target/debug/veredictum` when that binary exists (the CI test job has
#   already built it), otherwise builds through `cargo run --quiet --locked`.
set -euo pipefail
cd "$(dirname "$0")/../.."

GALLERY=website/book/src/badges

rendered="$(mktemp -d)"
trap 'rm -rf "$rendered"' EXIT

bash scripts/render/book-badges.sh "$rendered/badges" >/dev/null

if ! diff -ru "$GALLERY" "$rendered/badges"; then
  echo "::error::${GALLERY} is stale — redraw it with \`bash scripts/render/book-badges.sh\` and commit the result" >&2
  exit 1
fi

echo "book-badges: ${GALLERY} is byte-identical to what the binary draws — OK."
