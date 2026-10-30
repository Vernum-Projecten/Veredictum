#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Vendors the two XML Schemas the JUnit XML export is validated against.
#
# JUnit XML has no single owner, so the export is held to the two schemas the
# tools that read it publish: Apache Maven Surefire's test report schema, which
# defines one <testsuite>, and the Jenkins xUnit plugin's junit-10 schema,
# which defines the <testsuites> root that wraps several. Each is pinned to
# the commit of its latest release tag plus the SHA-256 of the bytes it served
# when this script was last run, so a moved tag or a re-published file fails
# the run instead of changing the norm. The bytes are written verbatim.
#
# Surefire's licence is its repository's LICENSE at the same commit, vendored
# as LICENSE-maven-surefire. The xUnit plugin's repository has no LICENSE
# file; its MIT text is the header comment of the schema itself, which this
# script copies out of the fetched bytes into LICENSE-xunit-plugin.
# Re-run after moving a pin below, and update the PROVENANCE file beside the
# schemas in the same change.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST_DIR="$REPO_ROOT/app/veredictum/src/arazzo/vendor"

SUREFIRE_COMMIT="5bf0dcf034365f13dc7fdc7a2776e936e636bc52"
SUREFIRE_URL="https://raw.githubusercontent.com/apache/maven-surefire/$SUREFIRE_COMMIT/maven-surefire-plugin/src/site/resources/xsd/surefire-test-report.xsd"
SUREFIRE_SHA256="f612ebfbd7cb285ac1af2fa9c103f2b83080feffffc6b3dfcc5ab6673d14f052"
SUREFIRE_FILE="surefire-test-report-3.6.1.xsd"
SUREFIRE_LICENSE_URL="https://raw.githubusercontent.com/apache/maven-surefire/$SUREFIRE_COMMIT/LICENSE"
SUREFIRE_LICENSE_SHA256="cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30"
SUREFIRE_LICENSE_FILE="LICENSE-maven-surefire"

XUNIT_COMMIT="5fbba4002e02a66032804c5cace4cf51b20491cf"
XUNIT_URL="https://raw.githubusercontent.com/jenkinsci/xunit-plugin/$XUNIT_COMMIT/src/main/resources/org/jenkinsci/plugins/xunit/types/model/xsd/junit-10.xsd"
XUNIT_SHA256="a1a816f58d1bf95ebabf371994df0b9246dee66ea9572fbec4f9296f1b2c0ff6"
XUNIT_FILE="junit-10-xunit-3.1.7.xsd"
XUNIT_LICENSE_FILE="LICENSE-xunit-plugin"

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

fetch "$SUREFIRE_URL" "$SUREFIRE_FILE" "$SUREFIRE_SHA256"
fetch "$SUREFIRE_LICENSE_URL" "$SUREFIRE_LICENSE_FILE" "$SUREFIRE_LICENSE_SHA256"
if ! grep -q "Apache License" "$TMP/$SUREFIRE_LICENSE_FILE"; then
  echo "error: $SUREFIRE_LICENSE_URL is not Apache-2.0; adjudicate before vendoring" >&2
  exit 1
fi
fetch "$XUNIT_URL" "$XUNIT_FILE" "$XUNIT_SHA256"
# The licence text is the schema's first comment, between its `<!--` and `-->`.
sed -n '/<!--/,/-->/{/<!--/d;/-->/d;p;}' "$TMP/$XUNIT_FILE" > "$TMP/$XUNIT_LICENSE_FILE"
if ! grep -q "The MIT License (MIT)" "$TMP/$XUNIT_LICENSE_FILE"; then
  echo "error: $XUNIT_URL does not carry the MIT licence in its header; adjudicate before vendoring" >&2
  exit 1
fi

mkdir -p "$DEST_DIR"
for file in "$SUREFIRE_FILE" "$SUREFIRE_LICENSE_FILE" "$XUNIT_FILE" "$XUNIT_LICENSE_FILE"; do
  cp "$TMP/$file" "$DEST_DIR/$file"
  echo "vendored $file -> app/veredictum/src/arazzo/vendor/$file"
done
