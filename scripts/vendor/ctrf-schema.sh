#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Vendors the CTRF JSON Schema at the specVersion the CTRF export pins, the
# schema the export's tests hold every emitted CTRF document to.
#
# The file lands inside the engine crate, beside the Arazzo schema, so the
# schema a published crate is tested against ships with it. The pin is the
# commit the upstream `v0.2.0` tag names plus the SHA-256 of the bytes it
# served when this script was last run, so a moved tag or a re-published file
# fails the run instead of changing the norm. The bytes are written verbatim.
# Re-run after moving the pin below, and update the PROVENANCE file beside the
# schema in the same change. The repository's LICENSE at the same commit lands
# beside the schema as LICENSE-ctrf, pinned by its own SHA-256.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST_DIR="$REPO_ROOT/app/veredictum/src/arazzo/vendor"
COMMIT="e325eb33c7662a208bfa4d093a41cc55439d6e6a"
URL="https://raw.githubusercontent.com/ctrf-io/ctrf/$COMMIT/schema/ctrf.schema.json"
SHA256="cb9bd075beb0e046076046793f96bb1ba72bd1b1d331b1fbd42ac0ea36776cf3"
FILE="ctrf-0.2.0.schema.json"
LICENSE_URL="https://raw.githubusercontent.com/ctrf-io/ctrf/$COMMIT/LICENSE"
LICENSE_SHA256="94b4ecd07157d9136e3b7369ece19ecf1c8196d09f8c8765e7e08cf8660f9e45"
LICENSE_FILE="LICENSE-ctrf"

TMP="$(mktemp -d)"
trap 'rm -r "$TMP"' EXIT

curl -fsSL --max-time 60 -o "$TMP/$FILE" "$URL"
actual="$(shasum -a 256 "$TMP/$FILE" | cut -d' ' -f1)"
if [[ "$actual" != "$SHA256" ]]; then
  echo "error: $URL served $actual, the pin is $SHA256" >&2
  exit 1
fi
curl -fsSL --max-time 60 -o "$TMP/$LICENSE_FILE" "$LICENSE_URL"
actual="$(shasum -a 256 "$TMP/$LICENSE_FILE" | cut -d' ' -f1)"
if [[ "$actual" != "$LICENSE_SHA256" ]]; then
  echo "error: $LICENSE_URL served $actual, the pin is $LICENSE_SHA256" >&2
  exit 1
fi
if ! grep -q "MIT License" "$TMP/$LICENSE_FILE"; then
  echo "error: $LICENSE_URL is not MIT; adjudicate before vendoring" >&2
  exit 1
fi
mkdir -p "$DEST_DIR"
cp "$TMP/$FILE" "$DEST_DIR/$FILE"
cp "$TMP/$LICENSE_FILE" "$DEST_DIR/$LICENSE_FILE"
echo "vendored $URL -> app/veredictum/src/arazzo/vendor/$FILE"
echo "vendored $LICENSE_URL -> app/veredictum/src/arazzo/vendor/$LICENSE_FILE"
