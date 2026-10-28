#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Changelog fragments: check them, preview them, fold them in at the cut (#766).
#
# A pull request records its user-visible change as one file under
# `changelog.d/` instead of as lines under the Unreleased heading of
# `CHANGELOG.md`, so two open pull requests never edit the same lines. The
# fragment format is documented in `changelog.d/README.md`, and this script is
# its one reader. No openEHR spec governs this — our own design, over Keep a
# Changelog 1.1.0 (https://keepachangelog.com/en/1.1.0/).
#
# Usage:
#   changelog.sh --check            every fragment is well formed
#   changelog.sh --preview          CHANGELOG.md with the fragments folded into
#                                   the Unreleased section, on stdout
#   changelog.sh --check-preview    changelog-structure.sh over that preview
#   changelog.sh --assemble <X.Y.Z> <YYYY-MM-DD>
#                                   the release cut: rewrites CHANGELOG.md and
#                                   deletes the fragments
#   changelog.sh --self-test        seeded fragments and a cut in a scratch tree
#
# The fold, for --preview and --assemble alike: every Keep a Changelog section
# is written in the canonical order (Added, Changed, Deprecated, Removed,
# Fixed, Security), the entries already under the Unreleased heading come
# first, and the fragments follow, ordered by issue number and then by slug,
# as one tight list, the way every section of CHANGELOG.md already reads. Each
# entry gains ` (#<issue>)` at the end of
# its first paragraph unless it already names that issue.
#
# Exit 0 is clean, 1 is a refused fragment or changelog, 2 is a usage error.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
readonly REPO
readonly NAME_RE='^([1-9][0-9]*)-([a-z0-9]+(-[a-z0-9]+)*)\.(added|changed|deprecated|removed|fixed|security)\.md$'
readonly SECTIONS="added changed deprecated removed fixed security"

usage() {
  sed -n '13,21p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
}

# check_fragments <root>: prints one line per defect on stderr and the number of
# fragments checked on stdout; returns 1 when any defect was found.
check_fragments() {
  local root="$1" dir="$1/changelog.d" bad=0 count=0 path name
  [[ -d "$dir" ]] || { echo "changelog: $dir does not exist" >&2; return 1; }
  while IFS= read -r path; do
    name="${path##*/}"
    [[ "$name" == "README.md" && -f "$path" ]] && continue
    count=$((count + 1))
    if [[ ! -f "$path" || -L "$path" ]]; then
      echo "changelog.d/$name: not a regular file; changelog.d/ holds fragments and README.md only" >&2
      bad=1
      continue
    fi
    if [[ ! "$name" =~ $NAME_RE ]]; then
      echo "changelog.d/$name: the name is not <issue>-<kebab-slug>.<section>.md (issue a positive integer without leading zeros; section one of: $SECTIONS)" >&2
      bad=1
      continue
    fi
    # The shape: list items opening with "- ", continuation lines indented by
    # two spaces, nothing else, and no blank line anywhere inside.
    if ! awk -v name="changelog.d/$name" '
      function refuse(why) { print name ":" FNR ": " why > "/dev/stderr"; failed = 1 }
      /\r/ { refuse("a carriage return; fragments use LF line endings") }
      /^[[:space:]]*$/ { refuse("a blank line; a fragment is one tight list with no blank line inside"); next }
      FNR == 1 && !/^- / { refuse("the first line must open a list item with \"- \" (no heading, no SPDX header)"); next }
      /^- [^[:space:]]/ { next }
      /^  / { next }
      { refuse("neither a list item (\"- text\") nor a continuation line indented by two spaces") }
      END {
        if (FNR == 0) refuse("the fragment is empty")
        exit failed
      }
    ' "$path"; then
      bad=1
    fi
  done < <(find "$dir" -mindepth 1 -maxdepth 1 | LC_ALL=C sort)
  echo "$count"
  return "$bad"
}

# sorted_fragments <root> <section>: the fragment paths of one section, ordered
# by issue number, then by slug.
sorted_fragments() {
  local root="$1" section="$2" path name
  find "$root/changelog.d" -mindepth 1 -maxdepth 1 -type f -name "*.$section.md" |
    while IFS= read -r path; do
      name="${path##*/}"
      [[ "$name" =~ $NAME_RE ]] || continue
      printf '%s\t%s\t%s\n' "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}" "$path"
    done | LC_ALL=C sort -t "$(printf '\t')" -k1,1n -k2,2 | cut -f3
}

# render_section <root> <section>: the section's fragments as they will read in
# CHANGELOG.md, as a tight list.
render_section() {
  local root="$1" section="$2" paths=()
  local path
  while IFS= read -r path; do
    [[ -n "$path" ]] && paths+=("$path")
  done < <(sorted_fragments "$root" "$section")
  [[ ${#paths[@]} -gt 0 ]] || return 0
  awk '
    # The first paragraph of an entry ends where a nested list opens, so the
    # reference goes on the line before the first nested item.
    function flush(   i, last, named) {
      if (n == 0) return
      last = n
      for (i = 2; i <= n; i++) if (line[i] ~ /^ +([-*+]|[0-9]+[.)]) /) { last = i - 1; break }
      named = 0
      for (i = 1; i <= n; i++) if ((line[i] "\n") ~ ("#" issue "[^0-9]")) named = 1
      if (!named) line[last] = line[last] " (#" issue ")"
      for (i = 1; i <= n; i++) print line[i]
      n = 0
    }
    FNR == 1 {
      flush()
      issue = FILENAME
      sub(/^.*\//, "", issue)
      sub(/-.*$/, "", issue)
    }
    /^- / { flush() }
    { line[++n] = $0 }
    END { flush() }
  ' "${paths[@]}"
}

# fold <root> <mode> [<version> <date>]: CHANGELOG.md with the fragments folded
# in, on stdout. mode is preview or assemble.
fold() {
  local root="$1" mode="$2" version="${3:-}" date="${4:-}" work section
  work="$(mktemp -d)"
  for section in $SECTIONS; do
    render_section "$root" "$section" >"$work/$section"
  done
  local status=0
  awk -v mode="$mode" -v ver="$version" -v date="$date" -v frag="$work" '
    function refuse(why) { print "changelog: CHANGELOG.md:" FNR ": " why > "/dev/stderr"; failed = 1 }
    function slurp(path,   text, row, any) {
      text = ""
      any = 0
      while ((getline row < path) > 0) { text = any ? text "\n" row : row; any = 1 }
      close(path)
      return text
    }
    function trim(text) {
      sub(/^([ \t]*\n)+/, "", text)
      sub(/(\n[ \t]*)+$/, "", text)
      if (text ~ /^[ \t]*$/) return ""
      return text
    }
    function flush(   i, sec, mine, theirs, kept, wrote) {
      if (mode == "assemble") {
        print "## [Unreleased]"
        print ""
        print "## [" ver "] - " date
      } else {
        print "## [Unreleased]"
      }
      print ""
      kept = trim(pre)
      if (kept != "") { print kept; print "" }
      wrote = 0
      for (i = 1; i <= 6; i++) {
        sec = order[i]
        mine = trim(body[sec])
        theirs = trim(slurp(frag "/" tolower(sec)))
        if (mine == "" && theirs == "") continue
        print "### " sec
        print ""
        if (mine != "") print mine
        if (theirs != "") print theirs
        print ""
        wrote = 1
      }
      if (mode == "assemble" && !wrote && kept == "")
        refuse("nothing to release: the Unreleased section is empty and changelog.d/ holds no fragment")
    }
    BEGIN {
      split("Added Changed Deprecated Removed Fixed Security", order, " ")
      for (i = 1; i <= 6; i++) canon[order[i]] = 1
    }
    # The heading itself, matched as a whole line: a mention of the word in
    # prose is never rewritten.
    /^## \[Unreleased\][[:space:]]*$/ {
      if (state != 0) refuse("a second Unreleased heading")
      state = 1
      cur = ""
      next
    }
    state == 1 && /^## / { flush(); state = 2 }
    state == 1 && /^### / {
      cur = $0
      sub(/^### /, "", cur)
      sub(/[[:space:]]+$/, "", cur)
      if (!(cur in canon)) refuse("\"### " cur "\" under Unreleased is not a Keep a Changelog section")
      if (cur in seen) refuse("a second \"### " cur "\" under Unreleased")
      seen[cur] = 1
      next
    }
    state == 1 {
      if (cur == "") pre = pre "\n" $0
      else body[cur] = body[cur] "\n" $0
      next
    }
    mode == "assemble" && /^\[unreleased\]: / {
      url = $0
      sub(/^\[unreleased\]: /, "", url)
      if (!match(url, /\/compare\/[^\/]+\.\.\.HEAD$/)) {
        refuse("the [unreleased] link is not <repository>/compare/<previous tag>...HEAD")
        print
        next
      }
      base = substr(url, 1, RSTART - 1)
      prev = substr(url, RSTART + 9, RLENGTH - 16)
      print "[unreleased]: " base "/compare/v" ver "...HEAD"
      print "[" ver "]: " base "/compare/" prev "...v" ver
      links++
      next
    }
    { print }
    END {
      if (state == 0) { refuse("no \"## [Unreleased]\" heading") }
      if (state == 1) flush()
      if (mode == "assemble" && links != 1) refuse("expected exactly one [unreleased] link reference, found " links + 0)
      exit failed
    }
  ' "$root/CHANGELOG.md" || status=1
  rm -rf "$work"
  return "$status"
}

cmd_check() {
  local root="$1" count
  count="$(check_fragments "$root")" || { echo "changelog: fragment check FAILED (changelog.d/README.md has the format)" >&2; return 1; }
  echo "changelog: ${count} fragment(s) well formed."
}

cmd_preview() {
  local root="$1" out status=0
  check_fragments "$root" >/dev/null || return 1
  out="$(mktemp)"
  fold "$root" preview >"$out" || status=1
  [[ "$status" -eq 0 ]] && cat "$out"
  rm -f "$out"
  return "$status"
}

cmd_check_preview() {
  local root="$1" out status=0
  out="$(mktemp)"
  cmd_preview "$root" >"$out" || status=1
  if [[ "$status" -eq 0 ]]; then
    bash "$REPO/scripts/checks/changelog-structure.sh" "$out" >/dev/null || status=1
  fi
  rm -f "$out"
  [[ "$status" -eq 0 ]] || { echo "changelog: the preview of the next release section is malformed" >&2; return 1; }
  echo "changelog: the preview with every fragment folded in keeps its Keep a Changelog structure."
}

cmd_assemble() {
  local root="$1" version="$2" date="$3" out path
  [[ "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] ||
    { echo "changelog: '$version' is not X.Y.Z (no leading v, no pre-release suffix)" >&2; return 2; }
  [[ "$date" =~ ^[0-9]{4}-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])$ ]] ||
    { echo "changelog: '$date' is not YYYY-MM-DD" >&2; return 2; }
  check_fragments "$root" >/dev/null || return 1
  if grep -qE "^## \[${version//./\\.}\]" "$root/CHANGELOG.md"; then
    echo "changelog: CHANGELOG.md already has a [$version] section" >&2
    return 1
  fi
  out="$(mktemp)"
  if ! fold "$root" assemble "$version" "$date" >"$out" ||
    ! bash "$REPO/scripts/checks/changelog-structure.sh" "$out" >/dev/null; then
    rm -f "$out"
    echo "changelog: the cut was refused; CHANGELOG.md and changelog.d/ are unchanged" >&2
    return 1
  fi
  cat "$out" >"$root/CHANGELOG.md"
  rm -f "$out"
  local removed=0
  while IFS= read -r path; do
    [[ "${path##*/}" == "README.md" ]] && continue
    rm -f "$path"
    removed=$((removed + 1))
  done < <(find "$root/changelog.d" -mindepth 1 -maxdepth 1 -type f | LC_ALL=C sort)
  echo "changelog: [$version] - $date written to CHANGELOG.md; ${removed} fragment(s) folded in and deleted."
}

# ── The self-test ─────────────────────────────────────────────────────────────

# seed_tree <root>: a scratch CHANGELOG.md with entries already under the
# Unreleased heading, a prose mention of that heading the cut must leave alone,
# and an empty changelog.d/ beside it.
seed_tree() {
  mkdir -p "$1/changelog.d"
  printf 'The format.\n' >"$1/changelog.d/README.md"
  cat >"$1/CHANGELOG.md" <<'EOF'
# Changelog

New entries reach the [Unreleased] heading through changelog.d/, and a cut
that rewrote a textual match of ## [Unreleased] would rewrite this line.

## [Unreleased]

### Added

- An entry already here (#1).

### Fixed

- A fix already here (#2).

## [0.1.0] - 2026-01-01

### Added

- The first release.

[unreleased]: https://example.invalid/r/compare/v0.1.0...HEAD
[0.1.0]: https://example.invalid/r/compare/v0.0.9...v0.1.0
EOF
}

self_test() {
  local tmp fail=0
  tmp="$(mktemp -d)"
  # shellcheck disable=SC2064 # expanded now on purpose: the path is fixed for this run
  trap "rm -rf '$tmp'" EXIT

  expect_ok() { # <label> <command…>
    local label="$1"
    shift
    if ! "$@" >"$tmp/out" 2>"$tmp/err"; then
      echo "self-test: '$label' was refused:" >&2
      sed 's/^/  /' "$tmp/err" >&2
      fail=1
    fi
  }
  expect_refused() { # <label> <stderr substring> <command…>
    local label="$1" want="$2"
    shift 2
    if "$@" >"$tmp/out" 2>"$tmp/err"; then
      echo "self-test: '$label' was accepted; the check does not fire" >&2
      fail=1
    elif ! grep -qF -- "$want" "$tmp/err"; then
      echo "self-test: '$label' was refused for the wrong reason (wanted '$want'):" >&2
      sed 's/^/  /' "$tmp/err" >&2
      fail=1
    fi
  }
  expect_same() { # <label> <want file> <got file>
    if ! diff -u "$2" "$3" >"$tmp/diff"; then
      echo "self-test: $1 differs from the expected text:" >&2
      sed 's/^/  /' "$tmp/diff" >&2
      fail=1
    fi
  }

  # The good tree. Issue 12 sorts after issue 3 only under a numeric sort, and
  # issue 76 naming #766 must still gain its own reference.
  local good="$tmp/good"
  seed_tree "$good"
  printf -- '- Twelve, over\n  two lines.\n' >"$good/changelog.d/12-twelve.added.md"
  printf -- '- Three.\n- A second item that names #3 already.\n' >"$good/changelog.d/3-three.added.md"
  printf -- '- Three again, a later slug.\n' >"$good/changelog.d/3-zeta.added.md"
  printf -- '- A change with points:\n  - one point\n  - another\n' >"$good/changelog.d/5-change.changed.md"
  printf -- '- A hardened path.\n' >"$good/changelog.d/7-hardening.security.md"
  printf -- '- Issue 76 names #766, not its own number.\n' >"$good/changelog.d/76-near-miss.fixed.md"

  expect_ok "--check over well-formed fragments" cmd_check "$good"
  grep -qF '6 fragment(s)' "$tmp/out" || { echo "self-test: --check did not count 6 fragments" >&2; fail=1; }

  cat >"$tmp/want-section" <<'EOF'
### Added

- An entry already here (#1).
- Three. (#3)
- A second item that names #3 already.
- Three again, a later slug. (#3)
- Twelve, over
  two lines. (#12)

### Changed

- A change with points: (#5)
  - one point
  - another

### Fixed

- A fix already here (#2).
- Issue 76 names #766, not its own number. (#76)

### Security

- A hardened path. (#7)

EOF
  {
    sed -n '1,5p' "$good/CHANGELOG.md"
    printf '## [Unreleased]\n\n'
    cat "$tmp/want-section"
    sed -n '/^## \[0\.1\.0\]/,$p' "$good/CHANGELOG.md"
  } >"$tmp/want-preview"
  expect_ok "--preview over the good tree" cmd_preview "$good"
  expect_same "--preview" "$tmp/want-preview" "$tmp/out"
  expect_ok "--check-preview over the good tree" cmd_check_preview "$good"

  {
    sed -n '1,5p' "$good/CHANGELOG.md"
    printf '## [Unreleased]\n\n## [0.2.0] - 2026-10-28\n\n'
    cat "$tmp/want-section"
    sed -n '/^## \[0\.1\.0\]/,/^\[unreleased\]/p' "$good/CHANGELOG.md" | sed '$d'
    printf '[unreleased]: https://example.invalid/r/compare/v0.2.0...HEAD\n'
    printf '[0.2.0]: https://example.invalid/r/compare/v0.1.0...v0.2.0\n'
    printf '[0.1.0]: https://example.invalid/r/compare/v0.0.9...v0.1.0\n'
  } >"$tmp/want-cut"
  expect_ok "--assemble 0.2.0 over the good tree" cmd_assemble "$good" 0.2.0 2026-10-28
  expect_same "the assembled CHANGELOG.md" "$tmp/want-cut" "$good/CHANGELOG.md"
  local left
  left="$(find "$good/changelog.d" -type f | sed 's|.*/||' | LC_ALL=C sort | tr '\n' ' ')"
  [[ "$left" == "README.md " ]] || { echo "self-test: the cut left '$left' in changelog.d/" >&2; fail=1; }
  expect_refused "a second cut of the same version" "already has a [0.2.0] section" \
    cmd_assemble "$good" 0.2.0 2026-10-29
  expect_refused "a cut with nothing to release" "nothing to release" \
    cmd_assemble "$good" 0.2.1 2026-10-29

  # Each bad fragment alone in its own tree, with the reason it must be refused.
  local n=0 name body want bad
  while IFS='|' read -r name body want; do
    n=$((n + 1))
    bad="$tmp/bad$n"
    seed_tree "$bad"
    # shellcheck disable=SC2059 # the seeded body is a printf format on purpose: it carries \n, \t and \r
    printf -- "$body" >"$bad/changelog.d/$name"
    expect_refused "--check over $name" "$want" cmd_check "$bad"
    expect_refused "--assemble over $name" "$want" cmd_assemble "$bad" 0.2.0 2026-10-28
  done <<'EOF'
abc-slug.added.md|- x\n|the name is not
0-slug.added.md|- x\n|the name is not
012-slug.added.md|- x\n|the name is not
5-Bad_Slug.added.md|- x\n|the name is not
5-slug.feature.md|- x\n|the name is not
5-slug.Added.md|- x\n|the name is not
5-slug.added.txt|- x\n|the name is not
5-slug-.added.md|- x\n|the name is not
5-slug.added.md||the fragment is empty
5-slug.added.md|### Added\n- x\n|must open a list item
5-slug.added.md|<!-- SPDX-License-Identifier: Apache-2.0 -->\n- x\n|must open a list item
5-slug.added.md|- x\n\n- y\n|a blank line
5-slug.added.md|- x\n\n|a blank line
5-slug.added.md|-x\n|must open a list item
5-slug.added.md|- x\n continued by one space\n|neither a list item
5-slug.added.md|- x\n\tcontinued by a tab\n|neither a list item
5-slug.added.md|- x\r\n|a carriage return
EOF
  mkdir "$tmp/bad-dir"
  seed_tree "$tmp/bad-dir"
  mkdir "$tmp/bad-dir/changelog.d/5-slug.added.md"
  expect_refused "--check over a directory" "not a regular file" cmd_check "$tmp/bad-dir"

  # Changelogs the cut must refuse, each with its fragments well formed.
  local broken="$tmp/broken"
  seed_tree "$broken"
  printf -- '- x\n' >"$broken/changelog.d/5-slug.added.md"
  expect_refused "a pre-release version" "no pre-release suffix" cmd_assemble "$broken" 0.2.0-rc1 2026-10-28
  expect_refused "a v-prefixed version" "no leading v" cmd_assemble "$broken" v0.2.0 2026-10-28
  expect_refused "a malformed date" "is not YYYY-MM-DD" cmd_assemble "$broken" 0.2.0 2026-13-01
  sed -i.bak 's/^\[unreleased\]: .*$//' "$broken/CHANGELOG.md"
  expect_refused "no [unreleased] link" "exactly one [unreleased] link" cmd_assemble "$broken" 0.2.0 2026-10-28
  seed_tree "$broken"
  sed -i.bak 's/^## \[Unreleased\]$/## [Unreleased] (next)/' "$broken/CHANGELOG.md"
  expect_refused "no Unreleased heading" "no \"## [Unreleased]\" heading" cmd_assemble "$broken" 0.2.0 2026-10-28
  expect_refused "--preview with no Unreleased heading" "no \"## [Unreleased]\" heading" cmd_preview "$broken"
  seed_tree "$broken"
  sed -i.bak 's/^### Fixed$/### Bugfixes/' "$broken/CHANGELOG.md"
  expect_refused "an unknown subsection under Unreleased" "not a Keep a Changelog section" cmd_preview "$broken"
  seed_tree "$broken"
  sed -i.bak 's/^### Fixed$/### Added/' "$broken/CHANGELOG.md"
  expect_refused "a duplicated subsection under Unreleased" "a second \"### Added\"" cmd_preview "$broken"
  grep -qF -- '- x' "$broken/changelog.d/5-slug.added.md" ||
    { echo "self-test: a refused cut deleted a fragment" >&2; fail=1; }

  [[ "$fail" -eq 0 ]] || { echo "changelog: self-test FAILED" >&2; return 1; }
  echo "changelog: self-test OK (a preview and a cut folded exactly; $n seeded bad fragments, a directory and every broken cut refused)."
}

case "${1:-}" in
  --check) [[ $# -eq 1 ]] || usage; cmd_check "$REPO" ;;
  --preview) [[ $# -eq 1 ]] || usage; cmd_preview "$REPO" ;;
  --check-preview) [[ $# -eq 1 ]] || usage; cmd_check_preview "$REPO" ;;
  --assemble) [[ $# -eq 3 ]] || usage; cmd_assemble "$REPO" "$2" "$3" ;;
  --self-test) [[ $# -eq 1 ]] || usage; self_test ;;
  *) usage ;;
esac
