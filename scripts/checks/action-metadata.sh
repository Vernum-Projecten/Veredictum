#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The root `action.yml` meets GitHub Marketplace's listing requirements, so a
# release can be published to the Marketplace from the release form (#802).
#
# The Marketplace reads the metadata from the TAGGED commit, and a release tag
# here is never re-pointed, so a description one character too long costs a
# whole release. The limits are GitHub's own: `name`, `description`, `author` and
# `branding` present (https://docs.github.com/en/actions/how-tos/create-and-publish-actions/publish-in-github-marketplace),
# and the release form refuses a description of 125 characters or more.
#
# What it deliberately does NOT check: that the name is free on the
# Marketplace. Only the release form can answer that.
#
# Usage:
#   scripts/checks/action-metadata.sh [action.yml]
#   scripts/checks/action-metadata.sh --self-test
set -euo pipefail

cd "$(dirname "$0")/../.."

readonly MAX_DESCRIPTION=124

check() {
  local file="$1" bad=0 key value
  for key in name description author branding.icon branding.color; do
    value="$(yq -r ".${key} // \"\"" "$file")"
    if [[ -z "$value" ]]; then
      echo "action-metadata: $file has no \`$key\`" >&2
      bad=1
    fi
  done
  value="$(yq -r '.description // ""' "$file")"
  if (( ${#value} > MAX_DESCRIPTION )); then
    echo "action-metadata: $file's description is ${#value} characters; the Marketplace takes at most $MAX_DESCRIPTION" >&2
    bad=1
  fi
  return "$bad"
}

self_test() {
  local dir
  dir="$(mktemp -d)"
  trap 'rm -rf "$dir"' RETURN
  local long
  long="$(printf 'x%.0s' $(seq 1 125))"
  printf 'name: a\ndescription: b\nauthor: c\nbranding:\n  icon: d\n  color: e\n' >"$dir/ok.yml"
  printf 'name: a\ndescription: %s\nauthor: c\nbranding:\n  icon: d\n  color: e\n' "$long" >"$dir/long.yml"
  printf 'name: a\ndescription: b\nauthor: c\n' >"$dir/nobrand.yml"
  check "$dir/ok.yml" || { echo "self-test: a valid file was refused" >&2; return 1; }
  if check "$dir/long.yml" 2>/dev/null; then
    echo "self-test: a 125-character description passed" >&2
    return 1
  fi
  if check "$dir/nobrand.yml" 2>/dev/null; then
    echo "self-test: a file without branding passed" >&2
    return 1
  fi
  echo "action-metadata: self-test OK"
}

if [[ "${1:-}" == "--self-test" ]]; then
  self_test
  exit
fi

check "${1:-action.yml}"
echo "action-metadata: OK"
