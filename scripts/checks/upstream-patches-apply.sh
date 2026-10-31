#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Every proposed fix under `upstream/` applies to the pinned `master` snapshot
# of its openEHR repository (#906).
#
# A patch under `upstream/<repository>/` is written against that repository's
# `master`, vendored at a pinned commit under
# `specs/openehr-development/<repository>/` with the upstream relative paths.
# Each patch is checked with `git apply --check --directory=` against that
# snapshot, so a patch that no longer applies, or one whose repository has no
# snapshot to apply to, is a finding. Nothing is ever written: `--check` only
# reports. A re-vendored snapshot that moves the text a patch edits fails here
# until the patch is re-written against it.
#
# Usage: scripts/checks/upstream-patches-apply.sh               # this repository
#        scripts/checks/upstream-patches-apply.sh --self-test   # seeded cases
set -euo pipefail
cd "$(dirname "$0")/../.."

SNAPSHOTS=specs/openehr-development

check() { # $1: root holding upstream/ and the snapshots; prints findings, returns 1 on any
  local root="$1" patch dir repo out found=0 seen=0
  if [[ ! -d "$root/upstream" ]]; then
    echo "upstream-patches-apply: ${root}/upstream does not exist" >&2
    return 1
  fi
  while IFS= read -r patch; do
    [[ -n "$patch" ]] || continue
    seen=$((seen + 1))
    dir="$(dirname "$patch")"
    repo="${dir#upstream}"
    repo="${repo#/}"
    if [[ -z "$repo" || "$repo" == */* ]]; then
      echo "upstream-patches-apply: ${patch} is not directly under upstream/<repository>/" >&2
      found=1
      continue
    fi
    if [[ ! -d "$root/$SNAPSHOTS/$repo" ]]; then
      echo "upstream-patches-apply: ${patch} has no snapshot of ${repo} under ${SNAPSHOTS}/" >&2
      found=1
      continue
    fi
    if ! out="$(cd "$root" && git apply --check --whitespace=nowarn \
      --directory="$SNAPSHOTS/$repo" "$patch" 2>&1)"; then
      echo "upstream-patches-apply: ${patch} does not apply to ${SNAPSHOTS}/${repo}:" >&2
      printf '%s\n' "$out" | sed -e 's/^/  /' >&2
      found=1
    fi
  done < <(cd "$root" && find upstream -name '*.patch' -type f | LC_ALL=C sort)
  echo "upstream-patches-apply: ${seen} patch(es) checked against ${SNAPSHOTS}/"
  return "$found"
}

if [[ "${1:-}" == "--self-test" ]]; then
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  for t in good bad; do
    mkdir -p "$tmp/$t/upstream/specifications-X" "$tmp/$t/$SNAPSHOTS/specifications-X/docs"
    printf 'one\ntwo\nthree\n' > "$tmp/$t/$SNAPSHOTS/specifications-X/docs/a.adoc"
  done
  # A patch written against the snapshot's text applies.
  cat > "$tmp/good/upstream/specifications-X/0001-applies.patch" <<'PATCH'
diff --git a/docs/a.adoc b/docs/a.adoc
--- a/docs/a.adoc
+++ b/docs/a.adoc
@@ -1,3 +1,3 @@
 one
-two
+TWO
 three
PATCH
  # A patch whose removed line the snapshot does not carry does not apply.
  sed -e 's/^-two$/-zwei/' "$tmp/good/upstream/specifications-X/0001-applies.patch" \
    > "$tmp/bad/upstream/specifications-X/0002-stale.patch"
  # A patch for a repository with no snapshot directory.
  mkdir -p "$tmp/bad/upstream/specifications-Y"
  cp "$tmp/good/upstream/specifications-X/0001-applies.patch" "$tmp/bad/upstream/specifications-Y/0003-orphan.patch"
  if ! check "$tmp/good" >/dev/null 2>&1; then
    echo "self-test: a patch that applies to its snapshot was refused" >&2
    exit 1
  fi
  out="$(check "$tmp/bad" 2>&1 || true)"
  for want in "0002-stale.patch does not apply" "0003-orphan.patch has no snapshot of specifications-Y"; do
    if [[ "$out" != *"$want"* ]]; then
      echo "self-test: '${want}' was not reported" >&2
      exit 1
    fi
  done
  if [[ "$out" != *"2 patch(es) checked"* ]]; then
    echo "self-test: the bad tree did not count both patches" >&2
    exit 1
  fi
  if [[ "$(cat "$tmp/good/$SNAPSHOTS/specifications-X/docs/a.adoc")" != $'one\ntwo\nthree' ]]; then
    echo "self-test: the check wrote to the snapshot" >&2
    exit 1
  fi
  echo "upstream-patches-apply: self-test OK (a patch that applies passes; a stale patch and a patch with no snapshot are refused; the snapshot is never written)."
  exit 0
fi

check .
