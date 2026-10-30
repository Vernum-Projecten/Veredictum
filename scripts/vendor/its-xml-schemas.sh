#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Vendors the released ITS-XML schemas into specs/its-xml-schemas/, one
# directory per lineage, each the upstream `components/` tree at its pinned
# commit, copied verbatim. The table below is the pin; PROVENANCE.md beside the
# tree is hand-written and names the same commits, so move both together.
#
# Four upstream files carry CRLF line endings. The repository's
# `* text=auto` attribute stores them as LF, so a re-run over an unchanged pin
# leaves `git status` clean.
#
# Idempotent: wipes and re-vendors each lineage directory. Re-run after moving
# a pin below. PROVENANCE.md is not touched.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST="$REPO_ROOT/specs/its-xml-schemas"
REPO="https://github.com/openEHR/specifications-ITS-XML.git"

# vendored dir | upstream tag | pinned commit
LINEAGES=(
  "its-xml-1.0.2-nsv1|Release-1.0.2v2|f7a937778bf9ea43b01b0f9d8a616e47f35017c1"
  "its-xml-2.0.0-nsv2|Release-2.0.0v2|de8b37ba6c9a5e126623a063cafba3b58ebf1107"
)

TMP="$(mktemp -d)"
trap 'rm -r "$TMP"' EXIT

for entry in "${LINEAGES[@]}"; do
  IFS='|' read -r dir tag sha <<<"$entry"
  echo "==> $dir (specifications-ITS-XML @ $tag, $sha)"
  src="$TMP/$dir"
  git init -q "$src"
  git -C "$src" remote add origin "$REPO"
  git -C "$src" fetch -q --depth 1 origin "$sha"
  git -C "$src" checkout -q FETCH_HEAD
  if [[ "$(git -C "$src" rev-parse HEAD)" != "$sha" ]]; then
    echo "error: $tag did not resolve to $sha" >&2
    exit 1
  fi
  if [[ ! -d "$src/components" ]]; then
    echo "error: $tag has no components/ tree at $sha" >&2
    exit 1
  fi
  if ! grep -q "Apache License" "$src/LICENSE"; then
    echo "error: $tag LICENSE at $sha is not Apache-2.0; adjudicate before vendoring" >&2
    exit 1
  fi

  out="$DEST/$dir"
  if [[ -d "$out" ]]; then
    rm -r "$out"
  fi
  mkdir -p "$out"
  cp -R "$src/components/." "$out/"
  count="$(find "$out" -type f -name '*.xsd' | wc -l | tr -d ' ')"
  echo "    $count XSDs -> specs/its-xml-schemas/$dir"
done
