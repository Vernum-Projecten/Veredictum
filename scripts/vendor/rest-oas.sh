#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Vendors the released ITS-REST OpenAPI documents into specs/rest-oas/: the
# upstream `computable/OAS/` directory of openEHR/specifications-ITS-REST at
# the pinned commit, copied verbatim, plus the repository's `LICENSE` from the
# same commit.
#
# The tree is derived from the ITS-REST repository at the ITS-REST pin in
# scripts/vendor/spec-docs.sh (tag Release-1.1.0), so the OAS and the docs text
# under specs/openehr/ITS-REST/ are one release. PROVENANCE.md beside the tree
# is hand-written and names the same commit, so move the three together.
#
# Idempotent: removes everything in the tree except PROVENANCE.md and
# re-vendors it. Re-run after moving the pin below.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST="$REPO_ROOT/specs/rest-oas"
REPO="https://github.com/openEHR/specifications-ITS-REST.git"
TAG="Release-1.1.0"
SHA="24058992d5fa96e8dfbd855d9c133f328387fc09"

TMP="$(mktemp -d)"
trap 'rm -r "$TMP"' EXIT

echo "==> specifications-ITS-REST @ $TAG, $SHA"
src="$TMP/src"
git init -q "$src"
git -C "$src" remote add origin "$REPO"
git -C "$src" fetch -q --depth 1 origin "$SHA"
git -C "$src" checkout -q FETCH_HEAD
if [[ "$(git -C "$src" rev-parse HEAD)" != "$SHA" ]]; then
  echo "error: the fetch did not resolve to $SHA" >&2
  exit 1
fi
# The tag must still name the pin: a moved release tag is a refusal, never a
# silent move of the vendored contract.
# An annotated tag's peeled `^{}` line names its commit; a lightweight tag has
# only the one line, which already does.
tagged="$(git ls-remote "$REPO" "refs/tags/$TAG^{}" | awk '{print $1}')"
if [[ -z "$tagged" ]]; then
  tagged="$(git ls-remote "$REPO" "refs/tags/$TAG" | awk '{print $1}')"
fi
if [[ "$tagged" != "$SHA" ]]; then
  echo "error: $TAG names ${tagged:-nothing}, the pin is $SHA" >&2
  exit 1
fi
if [[ ! -d "$src/computable/OAS" ]]; then
  echo "error: no computable/OAS/ directory at $SHA" >&2
  exit 1
fi
if [[ ! -f "$src/LICENSE" ]] || ! grep -q "Apache License" "$src/LICENSE"; then
  echo "error: the LICENSE at $SHA is not Apache-2.0; adjudicate before vendoring" >&2
  exit 1
fi

mkdir -p "$DEST"
find "$DEST" -mindepth 1 -maxdepth 1 ! -name PROVENANCE.md -exec rm -r {} +
cp -R "$src/computable/OAS/." "$DEST/"
cp "$src/LICENSE" "$DEST/LICENSE"
count="$(find "$DEST" -type f -name '*.openapi.yaml' | wc -l | tr -d ' ')"
echo "    $count OpenAPI documents and the LICENSE -> specs/rest-oas"
