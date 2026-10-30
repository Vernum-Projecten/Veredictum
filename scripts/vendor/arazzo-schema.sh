#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Vendors the official Arazzo 1.1 JSON Schema, the schema `validate` holds
# every Arazzo document under catalogue/cases/ to.
#
# The file lands INSIDE the engine crate, beside the module that embeds it with
# `include_str!`, because the crate publishes to crates.io and a package may
# only embed files it ships. Every other vendored tree is read at run time from
# a path the caller passes; this one is compiled in, so `validate` never
# fetches anything and a hosted image needs no extra tree.
#
# The OpenAPI Initiative publishes each schema revision under a dated,
# immutable URL. The pin is that URL plus the SHA-256 of the bytes it served
# when this script was last run, so a silent upstream re-publication fails the
# run instead of changing the norm. The bytes are written verbatim, never
# re-formatted. Re-run after moving the pin below, and update the PROVENANCE
# file beside the schema in the same change.
#
# The upstream repository's LICENSE lands beside the schema as
# LICENSE-Arazzo-Specification. The dated URL names no commit, so the LICENSE
# is pinned on its own: a commit of the repository and the SHA-256 of the file
# there.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST_DIR="$REPO_ROOT/app/veredictum/src/arazzo/vendor"
URL="https://spec.openapis.org/arazzo/1.1/schema/2026-04-15"
SHA256="37be908409bdb2f7bffe61fa23685c7e84cbeebfafac475a1d01dbc50ff7ab9e"
FILE="arazzo-1.1-schema-2026-04-15.json"
LICENSE_COMMIT="e9ee1c49414780441fef0c5196c52f8222357a0a"
LICENSE_URL="https://raw.githubusercontent.com/OAI/Arazzo-Specification/$LICENSE_COMMIT/LICENSE"
LICENSE_SHA256="c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4"
LICENSE_FILE="LICENSE-Arazzo-Specification"

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
if ! grep -q "Apache License" "$TMP/$LICENSE_FILE"; then
  echo "error: $LICENSE_URL is not Apache-2.0; adjudicate before vendoring" >&2
  exit 1
fi
mkdir -p "$DEST_DIR"
cp "$TMP/$FILE" "$DEST_DIR/$FILE"
cp "$TMP/$LICENSE_FILE" "$DEST_DIR/$LICENSE_FILE"
echo "vendored $URL -> app/veredictum/src/arazzo/vendor/$FILE"
echo "vendored $LICENSE_URL -> app/veredictum/src/arazzo/vendor/$LICENSE_FILE"
