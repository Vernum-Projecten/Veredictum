#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Every vendored tree has a vendor script and carries its upstream LICENSE
# (#855).
#
# A vendored tree is a directory holding a `PROVENANCE.md`, and every directory
# directly under `specs/` or `specs/openehr/` must be one. For each tree:
#
# - its PROVENANCE.md names a `scripts/vendor/<name>.sh` that exists and names
#   the tree's path or its parent's, since a script is the only way a vendored
#   tree is refreshed;
# - the tree root holds its upstream licence as `LICENSE`, or as one
#   `LICENSE-<upstream>` per upstream where one directory carries files from
#   several, and each such file is named both in PROVENANCE.md and in the
#   vendor script, which is what fetches it: a licence nobody fetched is a
#   hand-written one.
#
# A tree whose upstream publishes no licence file at all is listed in EXEMPT
# with the reason, and an exemption that no longer matches a licence-less tree
# is itself a finding. A script under `scripts/vendor/` that no PROVENANCE.md
# names is a finding too.
#
# Usage: scripts/checks/vendored-trees.sh               # this repository
#        scripts/checks/vendored-trees.sh --self-test   # seeded violations
set -euo pipefail
cd "$(dirname "$0")/../.."

# tree | why its upstream has no licence file to vendor
EXEMPT=(
  "perf/payloads/ckm|the openEHR CKM REST API publishes no repository licence; each template carries its archetypes' licence metadata, which PROVENANCE.md tallies"
  "specs/openehr/REQUIREMENTS|a release-site PDF whose terms are its own copyright notice, quoted in PROVENANCE.md; no licence file accompanies it"
)

exempt_reason() { # $1: tree; prints the reason when exempt
  local entry
  for entry in "${EXEMPT[@]}"; do
    if [[ "${entry%%|*}" == "$1" ]]; then
      printf '%s\n' "${entry#*|}"
      return 0
    fi
  done
  return 1
}

trees() { # $1: root; prints each vendored tree relative to the root, sorted
  # A git checkout lists only its tracked files, so local worktrees and build
  # output under the root are never read; the self-test's scratch root is no
  # repository and is walked instead.
  if [[ "$(git -C "$1" rev-parse --show-toplevel 2>/dev/null)" == "$(cd "$1" && pwd -P)" ]]; then
    git -C "$1" ls-files -- ':(glob)**/PROVENANCE.md' | sed -e 's|/PROVENANCE.md$||' | sort
  else
    (cd "$1" && find . -name PROVENANCE.md -type f -not -path './.git/*' -not -path './target/*' \
      -not -path '*/node_modules/*' | sed -e 's|^\./||' -e 's|/PROVENANCE.md$||' | sort)
  fi
}

names_tree() { # $1: script, $2: tree; true when the script names the tree
  # Either the tree's own path, or its parent's path as a whole token beside
  # the tree's basename as a word: spec-docs.sh writes every component of
  # specs/openehr from one destination and a table of component names.
  local parent
  parent="$(dirname "$2")"
  if grep -qF "$2" "$1"; then
    return 0
  fi
  grep -qE "(^|[^A-Za-z0-9._-])${parent}([^A-Za-z0-9._/-]|\$)" "$1" &&
    grep -qwF "$(basename "$2")" "$1"
}

check() { # $1: root; prints findings, returns 1 on any
  local root="$1" found=0 tree dir script lic named entry writes fetched count=0
  local -a all=() scripts=()
  while IFS= read -r tree; do
    [[ -n "$tree" ]] && all+=("$tree")
  done < <(trees "$root")

  for dir in "$root"/specs/*/ "$root"/specs/openehr/*/; do
    [[ -d "$dir" ]] || continue
    dir="${dir#"$root"/}"
    dir="${dir%/}"
    [[ "$dir" == "specs/openehr" ]] && continue
    if [[ ! -f "$root/$dir/PROVENANCE.md" ]]; then
      echo "vendored-trees: $dir has no PROVENANCE.md" >&2
      found=1
    fi
  done

  for tree in "${all[@]}"; do
    count=$((count + 1))
    scripts=()
    while IFS= read -r script; do
      [[ -n "$script" ]] || continue
      if [[ -f "$root/$script" ]]; then
        scripts+=("$script")
      else
        echo "vendored-trees: $tree/PROVENANCE.md names $script, which does not exist" >&2
        found=1
      fi
    done < <(grep -oE 'scripts/vendor/[A-Za-z0-9._-]+\.sh' "$root/$tree/PROVENANCE.md" | sort -u || true)
    if ! grep -qE 'scripts/vendor/[A-Za-z0-9._-]+\.sh' "$root/$tree/PROVENANCE.md"; then
      echo "vendored-trees: $tree/PROVENANCE.md names no scripts/vendor/ script" >&2
      found=1
    elif [[ "${#scripts[@]}" -gt 0 ]]; then
      writes=0
      for script in "${scripts[@]}"; do
        if names_tree "$root/$script" "$tree"; then
          writes=1
        fi
      done
      if [[ "$writes" -eq 0 ]]; then
        echo "vendored-trees: no script $tree/PROVENANCE.md names writes $tree" >&2
        found=1
      fi
    fi

    named=0
    for lic in "$root/$tree"/LICENSE "$root/$tree"/LICENSE-*; do
      [[ -f "$lic" ]] || continue
      named=$((named + 1))
      lic="$(basename "$lic")"
      if ! grep -qF "\`$lic\`" "$root/$tree/PROVENANCE.md"; then
        echo "vendored-trees: $tree/$lic is not named in its PROVENANCE.md" >&2
        found=1
      fi
      fetched=0
      for script in "${scripts[@]+"${scripts[@]}"}"; do
        if grep -qF "$lic" "$root/$script"; then
          fetched=1
        fi
      done
      if [[ "$fetched" -eq 0 ]]; then
        echo "vendored-trees: $tree/$lic is not fetched by its vendor script" >&2
        found=1
      fi
    done
    if exempt_reason "$tree" >/dev/null; then
      if [[ "$named" -gt 0 ]]; then
        echo "vendored-trees: $tree carries a licence file but is listed as exempt; remove the exemption" >&2
        found=1
      fi
    elif [[ "$named" -eq 0 ]]; then
      echo "vendored-trees: $tree carries no upstream LICENSE" >&2
      found=1
    fi
  done

  for entry in "${EXEMPT[@]}"; do
    tree="${entry%%|*}"
    if [[ ! -f "$root/$tree/PROVENANCE.md" ]]; then
      echo "vendored-trees: the exemption for $tree names no vendored tree" >&2
      found=1
    fi
  done

  for script in "$root"/scripts/vendor/*.sh; do
    [[ -f "$script" ]] || continue
    script="${script#"$root"/}"
    named=0
    for tree in "${all[@]}"; do
      if grep -qF "$script" "$root/$tree/PROVENANCE.md"; then
        named=1
        break
      fi
    done
    if [[ "$named" -eq 0 ]]; then
      echo "vendored-trees: $script is named by no PROVENANCE.md" >&2
      found=1
    fi
  done

  echo "vendored-trees: ${count} vendored tree(s) checked in ${root}"
  return "$found"
}

# The seeded files are literal text: their backticks and `$` must not expand.
# shellcheck disable=SC2016
if [[ "${1:-}" == "--self-test" ]]; then
  tmp="$(mktemp -d)"
  trap 'rm -r "$tmp"' EXIT
  seed() { # $1: root; a tree that passes every check
    mkdir -p "$1/scripts/vendor" "$1/specs/good" "$1/specs/openehr/RM" "$1/perf/payloads/ckm" \
      "$1/specs/openehr/REQUIREMENTS"
    printf 'DEST=specs/good\ncp LICENSE "$DEST/LICENSE"\n' > "$1/scripts/vendor/good.sh"
    printf 'Vendored by `scripts/vendor/good.sh`; licence `LICENSE`.\n' > "$1/specs/good/PROVENANCE.md"
    printf 'Apache License\n' > "$1/specs/good/LICENSE"
    printf 'DEST="$ROOT/specs/openehr"\nCOMPONENTS=(RM)\nREQ="$DEST/REQUIREMENTS"\ncp LICENSE x\n' > "$1/scripts/vendor/docs.sh"
    printf 'By `scripts/vendor/docs.sh`, `LICENSE` beside.\n' > "$1/specs/openehr/RM/PROVENANCE.md"
    printf 'CC\n' > "$1/specs/openehr/RM/LICENSE"
    printf 'Re-run `scripts/vendor/docs.sh`.\n' > "$1/specs/openehr/REQUIREMENTS/PROVENANCE.md"
    printf 'OUT=perf/payloads/ckm\n' > "$1/scripts/vendor/ckm.sh"
    printf 'By `scripts/vendor/ckm.sh`.\n' > "$1/perf/payloads/ckm/PROVENANCE.md"
  }
  seed "$tmp/good"
  # Two upstreams in one directory, each licence named in both places.
  mkdir -p "$tmp/good/app/vendor"
  printf 'DEST=app/vendor\nLICENSE-a\nLICENSE-b\n' > "$tmp/good/scripts/vendor/two.sh"
  printf '`scripts/vendor/two.sh` fetches `LICENSE-a` and `LICENSE-b`.\n' > "$tmp/good/app/vendor/PROVENANCE.md"
  printf 'a\n' > "$tmp/good/app/vendor/LICENSE-a"
  printf 'b\n' > "$tmp/good/app/vendor/LICENSE-b"
  if ! check "$tmp/good" >/dev/null 2>&1; then
    check "$tmp/good" || true
    echo "self-test: a complete layout was refused" >&2
    exit 1
  fi

  seed "$tmp/bad"
  # No LICENSE, and a provenance that names no script.
  mkdir -p "$tmp/bad/specs/nolicence"
  printf 'Fetched by hand.\n' > "$tmp/bad/specs/nolicence/PROVENANCE.md"
  # A script that does not exist.
  mkdir -p "$tmp/bad/specs/ghost"
  printf 'By `scripts/vendor/ghost.sh`, `LICENSE`.\n' > "$tmp/bad/specs/ghost/PROVENANCE.md"
  printf 'x\n' > "$tmp/bad/specs/ghost/LICENSE"
  # A LICENSE the provenance does not name and the script does not fetch, by a
  # script that names another tree.
  mkdir -p "$tmp/bad/specs/handwritten"
  printf 'DEST=specs/elsewhere\n' > "$tmp/bad/scripts/vendor/hand.sh"
  printf 'By `scripts/vendor/hand.sh`.\n' > "$tmp/bad/specs/handwritten/PROVENANCE.md"
  printf 'x\n' > "$tmp/bad/specs/handwritten/LICENSE"
  # A specs directory with no provenance at all.
  mkdir -p "$tmp/bad/specs/bare"
  # An exempt tree that now carries a licence, and a script nobody names.
  printf 'x\n' > "$tmp/bad/perf/payloads/ckm/LICENSE"
  printf 'echo orphan\n' > "$tmp/bad/scripts/vendor/orphan.sh"
  # The other exemption's tree is gone.
  rm "$tmp/bad/specs/openehr/REQUIREMENTS/PROVENANCE.md"
  out="$(check "$tmp/bad" 2>&1 || true)"
  for want in \
    "specs/nolicence/PROVENANCE.md names no scripts/vendor/ script" \
    "specs/nolicence carries no upstream LICENSE" \
    "names scripts/vendor/ghost.sh, which does not exist" \
    "no script specs/handwritten/PROVENANCE.md names writes specs/handwritten" \
    "specs/handwritten/LICENSE is not named in its PROVENANCE.md" \
    "specs/handwritten/LICENSE is not fetched by its vendor script" \
    "specs/bare has no PROVENANCE.md" \
    "perf/payloads/ckm carries a licence file but is listed as exempt" \
    "scripts/vendor/orphan.sh is named by no PROVENANCE.md" \
    "specs/openehr/REQUIREMENTS has no PROVENANCE.md" \
    "the exemption for specs/openehr/REQUIREMENTS names no vendored tree"; do
    if [[ "$out" != *"$want"* ]]; then
      echo "self-test: '${want}' was not reported" >&2
      printf '%s\n' "$out" >&2
      exit 1
    fi
  done
  echo "vendored-trees: self-test OK (a complete layout passes; a missing licence, script or provenance, an unfetched or unnamed licence, a stale exemption and an orphan script are refused)."
  exit 0
fi

check .
