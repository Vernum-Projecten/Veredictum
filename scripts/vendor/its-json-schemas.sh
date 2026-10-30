#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Vendors the ITS-JSON schemas into specs/its-json-schemas/: the upstream
# `components/` tree of openEHR/specifications-ITS-JSON at the pinned commit,
# copied verbatim to this directory's root, plus the repository's `LICENSE`
# from the same commit.
#
# ITS-JSON has no numbered release, so the pin is a frozen master SHA, the
# same one scripts/vendor/spec-docs.sh vendors the ITS-JSON docs text at.
# PROVENANCE.md beside the tree is hand-written and names the same commit, so
# move both together.
#
# Idempotent: removes everything in the tree except PROVENANCE.md and
# re-vendors it. Re-run after moving the pin below.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST="$REPO_ROOT/specs/its-json-schemas"
REPO="https://github.com/openEHR/specifications-ITS-JSON.git"
SHA="5acae056248e917a4b4c56f7e712f4fcfeb616a6"

TMP="$(mktemp -d)"
trap 'rm -r "$TMP"' EXIT

echo "==> specifications-ITS-JSON @ $SHA"
src="$TMP/src"
git init -q "$src"
git -C "$src" remote add origin "$REPO"
git -C "$src" fetch -q --depth 1 origin "$SHA"
git -C "$src" checkout -q FETCH_HEAD
if [[ "$(git -C "$src" rev-parse HEAD)" != "$SHA" ]]; then
  echo "error: the fetch did not resolve to $SHA" >&2
  exit 1
fi
if [[ ! -d "$src/components" ]]; then
  echo "error: no components/ tree at $SHA" >&2
  exit 1
fi
if [[ ! -f "$src/LICENSE" ]] || ! grep -q "Apache License" "$src/LICENSE"; then
  echo "error: the LICENSE at $SHA is not Apache-2.0; adjudicate before vendoring" >&2
  exit 1
fi

mkdir -p "$DEST"
find "$DEST" -mindepth 1 -maxdepth 1 ! -name PROVENANCE.md -exec rm -r {} +
cp -R "$src/components/." "$DEST/"
cp "$src/LICENSE" "$DEST/LICENSE"
count="$(find "$DEST" -type f -name '*.json' | wc -l | tr -d ' ')"
echo "    $count JSON schemas and the LICENSE -> specs/its-json-schemas"
