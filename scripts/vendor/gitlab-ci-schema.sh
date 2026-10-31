#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Vendors GitLab's CI/CD configuration JSON Schema, the schema the GitLab CI
# template (`ci/gitlab/veredictum.yml`) is validated against by the
# `gitlab_template` integration test.
#
# The file is `app/assets/javascripts/editor/schema/ci.json` of the GitLab
# repository, the schema GitLab's own pipeline editor validates a
# `.gitlab-ci.yml` with. The pin is a commit plus the SHA-256 of the bytes it
# served when this script was last run, so a re-published file fails the run
# instead of changing the norm. The bytes are written verbatim into
# ci/gitlab/schema, beside the repository's LICENSE at the same commit, pinned
# by its own SHA-256. Re-run after moving the pin below, and update
# ci/gitlab/schema/PROVENANCE.md in the same change.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST_DIR="$REPO_ROOT/ci/gitlab/schema"
COMMIT="28aa2bf90771a11ecc2f86525bc8a70a49c4d810"
URL="https://gitlab.com/gitlab-org/gitlab/-/raw/$COMMIT/app/assets/javascripts/editor/schema/ci.json"
SHA256="1c6eeb7d3a34c39996afbfe1ade220480eb05bbeae674e86d3c745dfeeded7fd"
FILE="ci.json"
LICENSE_URL="https://gitlab.com/gitlab-org/gitlab/-/raw/$COMMIT/LICENSE"
LICENSE_SHA256="62dfe4bdd76e08992c09cf335b2374b3e6acd4f2b959b4971760727d9b785ab4"
LICENSE_FILE="LICENSE"

TMP="$(mktemp -d)"
trap 'rm -r "$TMP"' EXIT

fetch() { # $1: url, $2: file, $3: pinned sha256
  curl -fsSL --max-time 60 -o "$TMP/$2" "$1"
  local actual
  actual="$(shasum -a 256 "$TMP/$2" | cut -d' ' -f1)"
  if [[ "$actual" != "$3" ]]; then
    echo "error: $1 served $actual, the pin is $3" >&2
    exit 1
  fi
}

fetch "$URL" "$FILE" "$SHA256"
fetch "$LICENSE_URL" "$LICENSE_FILE" "$LICENSE_SHA256"
if ! grep -q '"MIT Expat" license' "$TMP/$LICENSE_FILE"; then
  echo "error: $LICENSE_URL no longer licenses this path as MIT Expat; adjudicate before vendoring" >&2
  exit 1
fi
mkdir -p "$DEST_DIR"
cp "$TMP/$FILE" "$DEST_DIR/$FILE"
cp "$TMP/$LICENSE_FILE" "$DEST_DIR/$LICENSE_FILE"
echo "vendored $URL -> ci/gitlab/schema/$FILE"
echo "vendored $LICENSE_URL -> ci/gitlab/schema/$LICENSE_FILE"
