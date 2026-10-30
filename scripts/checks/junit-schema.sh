#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Every pinned JUnit view holds to the published JUnit XML schemas.
#
# JUnit XML has no single owner, so the export is held to the two schemas the
# tools that read it publish, vendored by `scripts/vendor/junit-schemas.sh`:
# the whole document to the Jenkins xUnit plugin's junit-10 schema, which
# defines the <testsuites> root, and each <testsuite> to Apache Maven
# Surefire's test report schema. The documents are the insta snapshots the
# export's tests pin, so a change to the view that breaks either schema fails
# here in the same pull request. xmllint (libxml2) does the validating.
#
# Usage: scripts/checks/junit-schema.sh               # the pinned views
#        scripts/checks/junit-schema.sh --self-test   # seeded violations
set -euo pipefail
cd "$(dirname "$0")/../.."

VENDOR="app/veredictum/src/arazzo/vendor"
DOCUMENT_XSD="$VENDOR/junit-10-xunit-3.1.7.xsd"
SUITE_XSD="$VENDOR/surefire-test-report-3.6.1.xsd"
SNAPSHOTS=(
  app/veredictum/tests/it/snapshots/it__results_export__the_junit_view_is_pinned.snap
  app/veredictum/tests/it/snapshots/it__results_export__the_junit_view_with_its_placement_is_pinned.snap
)

if ! command -v xmllint >/dev/null 2>&1; then
  echo "junit-schema: xmllint is not installed (libxml2-utils)" >&2
  exit 2
fi

check() { # $1: a JUnit XML file; prints findings, returns 1 on any
  local file="$1" count i tmp found=0
  if ! xmllint --noout --schema "$DOCUMENT_XSD" "$file" 2>"$file.err"; then
    sed 's/^/junit-schema: /' "$file.err" >&2
    found=1
  fi
  count="$(xmllint --xpath 'count(/testsuites/testsuite)' "$file")"
  tmp="$(mktemp)"
  for ((i = 1; i <= count; i++)); do
    {
      echo '<?xml version="1.0" encoding="UTF-8"?>'
      xmllint --xpath "/testsuites/testsuite[$i]" "$file"
    } > "$tmp"
    if ! xmllint --noout --schema "$SUITE_XSD" "$tmp" 2>"$file.err"; then
      sed "s|^|junit-schema: testsuite $i of $file: |" "$file.err" >&2
      found=1
    fi
  done
  rm -f "$tmp" "$file.err"
  return "$found"
}

# The XML a snapshot pins: everything after its second `---` line.
body() {
  awk 'n >= 2 { print } /^---$/ { n++ }' "$1"
}

if [[ "${1:-}" == "--self-test" ]]; then
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  body "${SNAPSHOTS[0]}" > "$tmp/good.xml"
  if ! check "$tmp/good.xml" 2>/dev/null; then
    echo "self-test: the pinned view was refused" >&2
    exit 1
  fi
  sed 's/ time="0"\/>/\/>/' "$tmp/good.xml" > "$tmp/no-time.xml"
  sed 's/<testsuites name="ehr" tests="5"/<testsuites name="ehr" skipped="1" tests="5"/' "$tmp/good.xml" > "$tmp/root-skipped.xml"
  for seeded in no-time root-skipped; do
    if check "$tmp/$seeded.xml" 2>/dev/null; then
      echo "self-test: the seeded violation '$seeded' passed" >&2
      exit 1
    fi
  done
  echo "junit-schema: self-test OK (the pinned view holds; a testcase without time and a root with skipped are refused)."
  exit 0
fi

status=0
for snapshot in "${SNAPSHOTS[@]}"; do
  tmp="$(mktemp)"
  body "$snapshot" > "$tmp"
  if check "$tmp"; then
    echo "junit-schema: $snapshot holds to both schemas"
  else
    echo "junit-schema: $snapshot does not hold" >&2
    status=1
  fi
  rm -f "$tmp"
done
exit "$status"
