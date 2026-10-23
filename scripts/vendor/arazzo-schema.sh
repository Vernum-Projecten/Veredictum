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
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST_DIR="$REPO_ROOT/app/veredictum/src/arazzo/vendor"
URL="https://spec.openapis.org/arazzo/1.1/schema/2026-04-15"
SHA256="37be908409bdb2f7bffe61fa23685c7e84cbeebfafac475a1d01dbc50ff7ab9e"
FILE="arazzo-1.1-schema-2026-04-15.json"

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
