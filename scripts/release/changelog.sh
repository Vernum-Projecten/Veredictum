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
#   changelog.sh --assemble <X.Y.Z-rcN> <YYYY-MM-DD>
#                                   a release-candidate cut: the same section,
#                                   and the fragments and the entries under the
#                                   Unreleased heading stay for the next cut
#   changelog.sh --covered <version>
#                                   the [<version>] section holds every fragment
#                                   and every entry under the Unreleased heading
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
# A release candidate is `X.Y.Z-rcN` with N >= 1, and no other pre-release
# suffix is accepted. Its section carries every change since the last release,
# as the final X.Y.Z section will, so its cut keeps what it folded in, and its
# link reference compares from the last release rather than from the previous
# candidate. The final cut after one or more candidates folds and deletes as
# any release cut does.
#
# Exit 0 is clean, 1 is a refused fragment or changelog, 2 is a usage error.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
readonly REPO
readonly NAME_RE='^([1-9][0-9]*)-([a-z0-9]+(-[a-z0-9]+)*)\.(added|changed|deprecated|removed|fixed|security)\.md$'
readonly SECTIONS="added changed deprecated removed fixed security"
readonly RELEASE_RE='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
readonly CANDIDATE_RE='^((0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*))-rc[1-9][0-9]*$'

usage() {
  sed -n '13,30p' "$0" | sed 's/^# \{0,1\}//' >&2
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

# fold <root> <mode> [<version> <date> <keep> <since>]: CHANGELOG.md with the
# fragments folded in, on stdout. mode is preview or assemble; keep is 1 when
# the entries under the Unreleased heading stay there (a release candidate), and
# since, when set, is the tag the new link reference compares from.
fold() {
  local root="$1" mode="$2" version="${3:-}" date="${4:-}" keep="${5:-0}" since="${6:-}" work section
  work="$(mktemp -d)"
  for section in $SECTIONS; do
    render_section "$root" "$section" >"$work/$section"
  done
  local status=0
  awk -v mode="$mode" -v ver="$version" -v date="$date" -v frag="$work" \
    -v keep="$keep" -v since="$since" '
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
        held = trim(raw)
        if (keep && held != "") { print held; print "" }
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
    state == 1 { raw = raw "\n" $0 }
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
      prev = since != "" ? since : substr(url, RSTART + 9, RLENGTH - 16)
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

# check_version <version>: returns 0 for X.Y.Z, 3 for X.Y.Z-rcN, and 2 with the
# reason on stderr for anything else.
check_version() {
  local version="$1"
  [[ "$version" =~ $RELEASE_RE ]] && return 0
  [[ "$version" =~ $CANDIDATE_RE ]] && return 3
  if [[ "$version" == v* ]]; then
    echo "changelog: '$version' is not X.Y.Z or X.Y.Z-rcN (no leading v)" >&2
  elif [[ "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)-(.*)$ ]]; then
    echo "changelog: '$version' carries the pre-release suffix '-${BASH_REMATCH[4]}'; the only one accepted is a release candidate, -rcN with N >= 1 and no leading zero (no alpha, beta or other suffix)" >&2
  else
    echo "changelog: '$version' is not X.Y.Z or X.Y.Z-rcN" >&2
  fi
  return 2
}

# has_section <changelog> <version>: the file carries a `## [<version>]` heading.
has_section() {
  local escaped="${2//./\\.}"
  grep -qE "^## \[${escaped}\]" "$1"
}

# last_release <changelog>: the tag the `[unreleased]` link compares from,
# followed back through release-candidate links to the last release, so a
# candidate's or a final cut's link spans every change since that release.
# Prints nothing when the link is absent, which the fold then refuses.
last_release() {
  local file="$1" tag next escaped hops=0
  tag="$(sed -nE 's|^\[unreleased\]: .*/compare/([^/]+)\.\.\.HEAD$|\1|p' "$file" | head -1)"
  while [[ "${tag#v}" =~ $CANDIDATE_RE ]]; do
    hops=$((hops + 1))
    [[ "$hops" -le 100 ]] || { echo "changelog: the release-candidate link references form a cycle" >&2; return 1; }
    escaped="${tag//./\\.}"
    next="$(sed -nE "s|^\[${escaped#v}\]: .*/compare/([^/]+)\.\.\.${escaped}$|\1|p" "$file" | head -1)"
    [[ -n "$next" ]] ||
      { echo "changelog: the [unreleased] link compares from $tag, and no [${tag#v}] link reference names the tag before it" >&2; return 1; }
    tag="$next"
  done
  printf '%s\n' "$tag"
}

cmd_assemble() {
  local root="$1" version="$2" date="$3" out path keep=0 since kind=0
  check_version "$version" || kind=$?
  case "$kind" in
    0) ;;
    3) keep=1 ;;
    *) return 2 ;;
  esac
  [[ "$date" =~ ^[0-9]{4}-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])$ ]] ||
    { echo "changelog: '$date' is not YYYY-MM-DD" >&2; return 2; }
  check_fragments "$root" >/dev/null || return 1
  if has_section "$root/CHANGELOG.md" "$version"; then
    echo "changelog: CHANGELOG.md already has a [$version] section" >&2
    return 1
  fi
  if [[ "$keep" -eq 1 ]] && has_section "$root/CHANGELOG.md" "${version%-rc*}"; then
    echo "changelog: CHANGELOG.md already has a [${version%-rc*}] section, so $version would follow its own release" >&2
    return 1
  fi
  since="$(last_release "$root/CHANGELOG.md")" || return 1
  out="$(mktemp)"
  if ! fold "$root" assemble "$version" "$date" "$keep" "$since" >"$out" ||
    ! bash "$REPO/scripts/checks/changelog-structure.sh" "$out" >/dev/null; then
    rm -f "$out"
    echo "changelog: the cut was refused; CHANGELOG.md and changelog.d/ are unchanged" >&2
    return 1
  fi
  cat "$out" >"$root/CHANGELOG.md"
  rm -f "$out"
  if [[ "$keep" -eq 1 ]]; then
    echo "changelog: [$version] - $date written to CHANGELOG.md; the fragments and the Unreleased entries stay for the next cut."
    return 0
  fi
  local removed=0
  while IFS= read -r path; do
    [[ "${path##*/}" == "README.md" ]] && continue
    rm -f "$path"
    removed=$((removed + 1))
  done < <(find "$root/changelog.d" -mindepth 1 -maxdepth 1 -type f | LC_ALL=C sort)
  echo "changelog: [$version] - $date written to CHANGELOG.md; ${removed} fragment(s) folded in and deleted."
}

# cmd_covered <root> <version>: every fragment, rendered as the cut renders it,
# and every entry under the Unreleased heading appear in the [<version>]
# section as unbroken runs of lines. A release-candidate tag runs this, because
# its cut keeps both behind for the final cut.
cmd_covered() {
  local root="$1" version="$2" work kind=0 status=0 section
  check_version "$version" || kind=$?
  [[ "$kind" -eq 0 || "$kind" -eq 3 ]] || return 2
  check_fragments "$root" >/dev/null || return 1
  if ! has_section "$root/CHANGELOG.md" "$version"; then
    echo "changelog: CHANGELOG.md has no [$version] section" >&2
    return 1
  fi
  work="$(mktemp -d)"
  awk -v ver="$version" '
    index($0, "## [" ver "]") == 1 { found = 1; next }
    found && (/^## / || /^\[[^]]+\]: /) { exit }
    found { print }
  ' "$root/CHANGELOG.md" >"$work/section"
  awk '
    /^## \[Unreleased\][[:space:]]*$/ { found = 1; next }
    found && /^## / { exit }
    found && !/^### / { print }
  ' "$root/CHANGELOG.md" >"$work/entries"
  for section in $SECTIONS; do
    render_section "$root" "$section" >>"$work/entries"
  done
  awk -v ver="$version" -v path="$work/section" '
    function check(   key) {
      if (block == "") return
      key = "\n" block "\n"
      if (!index(text, key)) {
        print "changelog: [" ver "] does not carry: " first > "/dev/stderr"
        missing++
      }
      block = ""
      first = ""
    }
    BEGIN {
      text = "\n"
      while ((getline row < path) > 0) text = text row "\n"
      close(path)
    }
    /^- / { check(); block = $0; first = $0; next }
    /^[[:space:]]*$/ { check(); next }
    { block = block == "" ? $0 : block "\n" $0; if (first == "") first = $0 }
    END {
      check()
      if (missing) exit 1
    }
  ' "$work/entries" || status=1
  rm -rf "$work"
  [[ "$status" -eq 0 ]] || { echo "changelog: the [$version] section does not carry every change since the last release; re-run the cut" >&2; return 1; }
  echo "changelog: the [$version] section carries every fragment and every Unreleased entry."
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
  local rc="$tmp/rc"
  cp -R "$good" "$rc"

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

  # Release candidates over the same tree. The first writes the section the
  # release cut above wrote and keeps the fragments and the Unreleased entries.
  local base='https://example.invalid/r/compare'
  {
    sed -n '1,5p' "$rc/CHANGELOG.md"
    sed -n '/^## \[Unreleased\]/,/^## \[0\.1\.0\]/p' "$rc/CHANGELOG.md" | sed '$d'
    printf '## [0.2.0-rc1] - 2026-10-28\n\n'
    cat "$tmp/want-section"
    sed -n '/^## \[0\.1\.0\]/,/^\[unreleased\]/p' "$rc/CHANGELOG.md" | sed '$d'
    printf '[unreleased]: %s/v0.2.0-rc1...HEAD\n' "$base"
    printf '[0.2.0-rc1]: %s/v0.1.0...v0.2.0-rc1\n' "$base"
    printf '[0.1.0]: %s/v0.0.9...v0.1.0\n' "$base"
  } >"$tmp/want-rc1"
  local fragments_before
  fragments_before="$(find "$rc/changelog.d" -type f | sed 's|.*/||' | LC_ALL=C sort | tr '\n' ' ')"
  expect_ok "--assemble 0.2.0-rc1 over the good tree" cmd_assemble "$rc" 0.2.0-rc1 2026-10-28
  expect_same "the CHANGELOG.md of a release candidate" "$tmp/want-rc1" "$rc/CHANGELOG.md"
  left="$(find "$rc/changelog.d" -type f | sed 's|.*/||' | LC_ALL=C sort | tr '\n' ' ')"
  [[ "$left" == "$fragments_before" ]] ||
    { echo "self-test: the candidate cut left '$left' in changelog.d/, not '$fragments_before'" >&2; fail=1; }
  expect_ok "--covered 0.2.0-rc1 right after its cut" cmd_covered "$rc" 0.2.0-rc1
  expect_refused "a second cut of the same candidate" "already has a [0.2.0-rc1] section" \
    cmd_assemble "$rc" 0.2.0-rc1 2026-10-29

  # A fragment merged after the first candidate: the first no longer covers
  # it, and the second candidate carries everything, linked from the release.
  printf -- '- A late fix.\n' >"$rc/changelog.d/9-late.fixed.md"
  expect_refused "--covered 0.2.0-rc1 with a later fragment" "does not carry: - A late fix. (#9)" \
    cmd_covered "$rc" 0.2.0-rc1
  # The blank line that follows a version heading opens the section text.
  {
    echo
    sed '/^- A fix already here (#2)\.$/a\
- A late fix. (#9)
' "$tmp/want-section"
  } >"$tmp/want-section-rc2"
  expect_ok "--assemble 0.2.0-rc2 after a later fragment" cmd_assemble "$rc" 0.2.0-rc2 2026-10-29
  section_of() { # <changelog> <version>
    awk -v ver="$2" 'index($0, "## [" ver "]") == 1 { f = 1; next } f && (/^## / || /^\[[^]]+\]: /) { exit } f { print }' "$1"
  }
  section_of "$rc/CHANGELOG.md" 0.2.0-rc2 >"$tmp/got-rc2"
  expect_same "the [0.2.0-rc2] section" "$tmp/want-section-rc2" "$tmp/got-rc2"
  local link
  for link in "[unreleased]: $base/v0.2.0-rc2...HEAD" "[0.2.0-rc2]: $base/v0.1.0...v0.2.0-rc2" \
    "[0.2.0-rc1]: $base/v0.1.0...v0.2.0-rc1"; do
    grep -qxF -- "$link" "$rc/CHANGELOG.md" || { echo "self-test: the second candidate lacks the link '$link'" >&2; fail=1; }
  done
  expect_ok "--covered 0.2.0-rc2 right after its cut" cmd_covered "$rc" 0.2.0-rc2

  # The final cut after the candidates folds and deletes, and its link spans
  # every change since the last release.
  expect_ok "--assemble 0.2.0 after two candidates" cmd_assemble "$rc" 0.2.0 2026-10-30
  section_of "$rc/CHANGELOG.md" 0.2.0 >"$tmp/got-final"
  expect_same "the [0.2.0] section after two candidates" "$tmp/want-section-rc2" "$tmp/got-final"
  sed -n '/^## \[Unreleased\]/,/^## \[0\.2\.0\]/p' "$rc/CHANGELOG.md" >"$tmp/got-unreleased"
  printf '## [Unreleased]\n\n## [0.2.0] - 2026-10-30\n' >"$tmp/want-unreleased"
  expect_same "the Unreleased section after the final cut" "$tmp/want-unreleased" "$tmp/got-unreleased"
  for link in "[unreleased]: $base/v0.2.0...HEAD" "[0.2.0]: $base/v0.1.0...v0.2.0"; do
    grep -qxF -- "$link" "$rc/CHANGELOG.md" || { echo "self-test: the final cut lacks the link '$link'" >&2; fail=1; }
  done
  left="$(find "$rc/changelog.d" -type f | sed 's|.*/||' | LC_ALL=C sort | tr '\n' ' ')"
  [[ "$left" == "README.md " ]] || { echo "self-test: the final cut left '$left' in changelog.d/" >&2; fail=1; }
  expect_ok "--covered 0.2.0 after the final cut" cmd_covered "$rc" 0.2.0
  expect_refused "a candidate after its own release" "would follow its own release" \
    cmd_assemble "$rc" 0.2.0-rc3 2026-10-31
  expect_refused "--covered for a version with no section" "has no [0.3.0] section" cmd_covered "$rc" 0.3.0

  # Each bad fragment alone in its own tree, with the reason it must be refused.
  # The seeded SPDX header below is test data, not this file's licence.
  # REUSE-IgnoreStart
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
  # REUSE-IgnoreEnd
  mkdir "$tmp/bad-dir"
  seed_tree "$tmp/bad-dir"
  mkdir "$tmp/bad-dir/changelog.d/5-slug.added.md"
  expect_refused "--check over a directory" "not a regular file" cmd_check "$tmp/bad-dir"

  # Changelogs the cut must refuse, each with its fragments well formed.
  local broken="$tmp/broken"
  seed_tree "$broken"
  printf -- '- x\n' >"$broken/changelog.d/5-slug.added.md"
  local suffix
  for suffix in beta1 alpha.1 rc0 rc01 RC1 rc rc1.1 dev; do
    expect_refused "the non-rc suffix -$suffix" "the only one accepted is a release candidate" \
      cmd_assemble "$broken" "0.2.0-$suffix" 2026-10-28
  done
  expect_refused "--covered over a non-rc suffix" "the only one accepted is a release candidate" \
    cmd_covered "$broken" 0.2.0-beta1
  expect_refused "a v-prefixed version" "no leading v" cmd_assemble "$broken" v0.2.0 2026-10-28
  expect_refused "a v-prefixed candidate" "no leading v" cmd_assemble "$broken" v0.2.0-rc1 2026-10-28
  sed -i.bak 's|^\[unreleased\]: .*$|[unreleased]: https://example.invalid/r/compare/v0.2.0-rc1...HEAD|' "$broken/CHANGELOG.md"
  expect_refused "a candidate link with no reference behind it" "no [0.2.0-rc1] link reference" \
    cmd_assemble "$broken" 0.2.0-rc2 2026-10-28
  seed_tree "$broken"
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
  echo "changelog: self-test OK (a preview, a cut, two release candidates and the final cut after them folded exactly; --covered passed and refused; $n seeded bad fragments, a directory, every non-rc suffix and every broken cut refused)."
}

case "${1:-}" in
  --check) [[ $# -eq 1 ]] || usage; cmd_check "$REPO" ;;
  --preview) [[ $# -eq 1 ]] || usage; cmd_preview "$REPO" ;;
  --check-preview) [[ $# -eq 1 ]] || usage; cmd_check_preview "$REPO" ;;
  --assemble) [[ $# -eq 3 ]] || usage; cmd_assemble "$REPO" "$2" "$3" ;;
  --covered) [[ $# -eq 2 ]] || usage; cmd_covered "$REPO" "$2" ;;
  --self-test) [[ $# -eq 1 ]] || usage; self_test ;;
  *) usage ;;
esac
