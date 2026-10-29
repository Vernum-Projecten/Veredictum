#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The citation guard (#812): `CITATION.cff` `version` and the `version` of the
# generated `.zenodo.json` are the workspace engine's `[package]` version, on
# every pull request.
#
# The release pipeline's `plan` compares `CITATION.cff` with the tag, but a
# tag is never re-pointed, so a drift found there is found too late: the
# recovery is the next version. A deposit's files freeze at publication
# (https://help.zenodo.org/docs/github/describe-software/zenodo-json/), so the
# pull request that moves the version is the place to hold all three to one
# value. A release candidate's `X.Y.Z-rcN` is compared as written, like any
# other version.
#
# The engine version is read from `app/veredictum/Cargo.toml`, the one
# `[package]` the workspace publishes (the root manifest is virtual), exactly
# as `scripts/release/check-console-pin.sh` reads it.
#
# Usage:  citation-version.sh              check this tree
#         citation-version.sh --self-test  seeded trees, each judged as expected
#
# Exits 0 when the three agree, 1 naming each one that does not, and 2 on a
# usage error.
set -euo pipefail

SELF="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
readonly SELF

# engine_version <root>: the `[package]` table's own version, never the first
# `version = ` line, because the manifest carries dependency versions too.
engine_version() {
  awk -F'"' '/^\[package\]/{p=1} p && /^version = /{print $2; exit}' "$1/app/veredictum/Cargo.toml"
}

# cff_version <root>: the top-level `version:` scalar, quoted or not.
cff_version() {
  sed -nE 's/^version:[[:space:]]*"?([^"]+)"?[[:space:]]*$/\1/p' "$1/CITATION.cff" | head -1
}

# zenodo_version <root>: the top-level `"version"` member. The file is
# rendered by `scripts/render/zenodo-json.sh` with jq's two-space indentation,
# so the top-level member is the one at that depth.
zenodo_version() {
  sed -nE 's/^  "version":[[:space:]]*"([^"]*)",?[[:space:]]*$/\1/p' "$1/.zenodo.json" | head -1
}

# guard <root>
guard() {
  local root="$1" engine cff zenodo status=0
  for file in app/veredictum/Cargo.toml CITATION.cff .zenodo.json; do
    [[ -f "$root/$file" ]] || { echo "::error::citation-version: $file is missing" >&2; return 1; }
  done
  engine="$(engine_version "$root")"
  cff="$(cff_version "$root")"
  zenodo="$(zenodo_version "$root")"
  if [[ -z "$engine" ]]; then
    echo "::error::citation-version: could not read the [package] version from app/veredictum/Cargo.toml" >&2
    return 1
  fi
  if [[ "$cff" != "$engine" ]]; then
    echo "::error::CITATION.cff version is '${cff}' while the workspace engine version is ${engine}. The pull request that moves the version moves CITATION.cff \`version\` and \`date-released\` with it." >&2
    status=1
  fi
  if [[ "$zenodo" != "$engine" ]]; then
    echo "::error::.zenodo.json version is '${zenodo}' while the workspace engine version is ${engine}. .zenodo.json is generated: update CITATION.cff and run scripts/render/zenodo-json.sh." >&2
    status=1
  fi
  if [[ "$status" -eq 0 ]]; then
    echo "citation-version: CITATION.cff and .zenodo.json carry the workspace engine version ${engine}. OK."
  fi
  return "$status"
}

self_test() {
  local tmp fail=0 n=0
  tmp="$(mktemp -d)"
  # shellcheck disable=SC2064 # expanded now on purpose: the path is fixed for this run
  trap "rm -rf '$tmp'" EXIT

  # seed <dir> <engine> <cff> <zenodo>: a tree carrying the three versions,
  # plus a dependency `version` after the `[package]` one and a nested
  # `version` member in `.zenodo.json`, so neither reader takes the wrong one.
  seed() {
    mkdir -p "$1/app/veredictum"
    printf '[package]\nname = "veredictum"\nversion = "%s"\n\n[dependencies]\nserde = { version = "1" }\n' \
      "$2" >"$1/app/veredictum/Cargo.toml"
    printf 'cff-version: 1.2.0\ntitle: Veredictum\nversion: %s\ndate-released: 2026-10-29\n' \
      "$3" >"$1/CITATION.cff"
    printf '{\n  "title": "Veredictum",\n  "version": "%s",\n  "creators": [\n    {\n      "version": "nested"\n    }\n  ]\n}\n' \
      "$4" >"$1/.zenodo.json"
  }
  # case_ <label> <want exit> <engine> <cff> <zenodo>
  case_() {
    local label="$1" want="$2" got=0
    n=$((n + 1))
    seed "$tmp/case$n" "$3" "$4" "$5"
    guard "$tmp/case$n" >"$tmp/out" 2>&1 || got=$?
    if [[ "$got" -ne "$want" ]]; then
      echo "self-test: '$label' exited $got, wanted $want:" >&2
      sed 's/^/  /' "$tmp/out" >&2
      fail=1
    fi
  }

  case_ "all three agree" 0 0.2.0 0.2.0 0.2.0
  case_ "a release candidate, all three agree" 0 0.2.0-rc2 0.2.0-rc2 0.2.0-rc2
  case_ "CITATION.cff behind the engine" 1 0.2.0-rc2 0.2.0-rc1 0.2.0-rc2
  case_ ".zenodo.json behind the engine" 1 0.2.0-rc2 0.2.0-rc2 0.2.0-rc1
  case_ "both behind the engine" 1 0.3.0 0.2.0 0.2.0
  case_ "a candidate citation for a final engine" 1 0.2.0 0.2.0-rc2 0.2.0
  case_ "a v-prefixed citation" 1 0.2.0 v0.2.0 0.2.0

  n=$((n + 1))
  seed "$tmp/case$n" 0.2.0 0.2.0 0.2.0
  printf 'cff-version: 1.2.0\ntitle: Veredictum\nversion: "0.2.0"\n' >"$tmp/case$n/CITATION.cff"
  if ! guard "$tmp/case$n" >"$tmp/out" 2>&1; then
    echo "self-test: a quoted CITATION.cff version was refused:" >&2
    sed 's/^/  /' "$tmp/out" >&2
    fail=1
  fi

  n=$((n + 1))
  seed "$tmp/case$n" 0.2.0 0.2.0 0.2.0
  rm "$tmp/case$n/.zenodo.json"
  if guard "$tmp/case$n" >"$tmp/out" 2>&1; then
    echo "self-test: a tree with no .zenodo.json passed" >&2
    fail=1
  fi

  [[ "$fail" -eq 0 ]] || { echo "citation-version: self-test FAILED" >&2; return 1; }
  echo "citation-version: self-test OK ($n seeded trees judged as expected)."
}

case "${1:-}" in
  --self-test)
    [[ $# -eq 1 ]] || { echo "citation-version: --self-test takes no arguments" >&2; exit 2; }
    self_test
    ;;
  "")
    guard "$(cd "$(dirname "$SELF")/../.." && pwd)"
    ;;
  *)
    echo "citation-version: unknown argument $1" >&2
    exit 2
    ;;
esac
