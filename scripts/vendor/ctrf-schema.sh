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
# schema in the same change.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST_DIR="$REPO_ROOT/app/veredictum/src/arazzo/vendor"
COMMIT="e325eb33c7662a208bfa4d093a41cc55439d6e6a"
URL="https://raw.githubusercontent.com/ctrf-io/ctrf/$COMMIT/schema/ctrf.schema.json"
SHA256="cb9bd075beb0e046076046793f96bb1ba72bd1b1d331b1fbd42ac0ea36776cf3"
FILE="ctrf-0.2.0.schema.json"

TMP="$(mktemp -d)"
trap 'rm -r "$TMP"' EXIT

curl -fsSL --max-time 60 -o "$TMP/$FILE" "$URL"
actual="$(shasum -a 256 "$TMP/$FILE" | cut -d' ' -f1)"
if [[ "$actual" != "$SHA256" ]]; then
  echo "error: $URL served $actual, the pin is $SHA256" >&2
  exit 1
fi
mkdir -p "$DEST_DIR"
cp "$TMP/$FILE" "$DEST_DIR/$FILE"
echo "vendored $URL -> app/veredictum/src/arazzo/vendor/$FILE"
