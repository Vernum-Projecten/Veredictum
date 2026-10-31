#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The pull-request half of the changelog: a user-visible change carries a
# fragment under `changelog.d/`, and nothing writes under the Unreleased
# heading of `CHANGELOG.md` any more (#10, #766).
#
# Two rules, checked over the change as measured from its merge base with the
# base branch. In CI that change is the pull request's committed range, head
# against base. Run locally with at most one ref, it is the WORKING TREE:
# everything committed on the branch plus what is staged, unstaged and
# untracked (ignored files aside), against the merge base of the base ref and
# HEAD (#665). The gate a contributor runs before committing therefore judges
# the change being made, and commits that reached `origin/main` through
# another worktree's fetch are never counted against it.
#
#   1. REFUSED, whatever the labels: a non-blank line added under the
#      `## [Unreleased]` heading of `CHANGELOG.md`. Entries are fragments
#      (`changelog.d/README.md`), so two open pull requests never edit the same
#      lines. The release cut is no exception to write: its lines land under
#      the new `## [X.Y.Z] - YYYY-MM-DD` heading, below the empty Unreleased
#      heading it re-adds, so the rule holds for it unchanged. A
#      release-candidate cut (`## [X.Y.Z-rcN] - YYYY-MM-DD`) leaves the
#      Unreleased section exactly as it was, so it holds for that cut too.
#   2. REQUIRED, unless waived: a change that touches a user-visible path adds
#      at least one file under `changelog.d/` other than its README. The
#      `--no-changelog-label` flag (the CI step passes it when the pull request
#      carries the `no-changelog` label) waives this rule and only this one. A
#      release cut, recognised by a `## [X.Y.Z] - ` or `## [X.Y.Z-rcN] - `
#      heading it adds to `CHANGELOG.md`, satisfies it: the cut folds the
#      fragments in, and its version bump touches `Cargo.toml`.
#
# The path set lives HERE rather than in the workflow, so "user-visible" is one
# declared list a reader can check against `CHANGELOG.md`'s own definition of
# the term: the CLI surface, the published artifact schemas, verdict
# semantics, the container image, or anything a party's published record
# depends on. Each entry below is one of those:
#
#   app/veredictum/src/   the runner and the CLI, including verdict semantics
#   app/veredictum-console/src/   the web console the image serves
#   catalogue/      the catalogue — a workflow, a rows file or an option
#                   changes what a run reports about somebody's server. No
#                   file under it is carved out: `id-map.yaml` is how an
#                   earlier record's case ids resolve to the ones a run
#                   reports now, and `keys/` is the signing material the
#                   catalogue's signature and SMART workflows present
#   perf/           the performance schedule — a workload, a host
#                   prescription or a payload changes what a measured run
#                   records
#   schemas/        the published JSON Schemas an integrator authors against
#   deploy/docker/  the container image, and the ignore file that decides what
#   .dockerignore   its build can even see
#   Cargo.toml      the package identity, the dependency set, and the lint tables
#   Cargo.lock      that decide what compiles
#   action.yml      the published GitHub Action a vendor's CI runs, and the one
#   scripts/action/run.sh   script it executes
#
# Deliberately NOT in the set, because a changelog entry for them would be noise
# rather than news: the crates' `tests/`, `specs/` (vendored, refreshed only by
# its own script), `scripts/` other than the action's script, `.github/`,
# `.claude/`, and prose.
#
# Usage:  changelog-entry.sh [<base-ref>] [--no-changelog-label]
#                         the working tree against its merge base with
#                         <base-ref> (default: origin/main)
#         changelog-entry.sh <base-ref> <head-ref> [--no-changelog-label]
#                         the committed range, as CI runs it
#         changelog-entry.sh --self-test          seeded changes in a scratch repository
#
# Exits 0 when both rules hold, 1 otherwise, naming the lines or paths that
# triggered it, and 2 on a usage error.
set -euo pipefail

SELF="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
readonly SELF

# One regular expression, assembled from the list above so the list is the thing
# a reader edits.
readonly USER_VISIBLE_PATHS=(
  'app/veredictum/src/'
  'app/veredictum-console/src/'
  'catalogue/'
  'perf/'
  'schemas/'
  'deploy/docker/'
  '\.dockerignore$'
  'Cargo\.toml$'
  'Cargo\.lock$'
  'app/[^/]+/Cargo\.toml$'
  'action\.yml$'
  'scripts/action/run\.sh$'
)

# unreleased_lines: the non-blank lines of the Unreleased section of the
# changelog on stdin, as `<line number><TAB><text>`; exits 1 when the changelog
# has no Unreleased heading. It reads to the end rather than exiting early, so
# the `git show` feeding it never meets a closed pipe.
unreleased_lines() {
  awk '
    !start && /^## \[Unreleased\][[:space:]]*$/ { start = 1; next }
    start && /^## / { done = 1 }
    start && !done && /[^[:space:]]/ { print NR "\t" $0 }
    END { if (!start) exit 1 }
  '
}

# head_changelog <repo> <head>: CHANGELOG.md as the change leaves it, from the
# named commit, or from the working tree when <head> is empty.
head_changelog() {
  if [[ -n "$2" ]]; then
    git -C "$1" show "$2:CHANGELOG.md"
  else
    cat "$1/CHANGELOG.md"
  fi
}

# guard <repo> <base> <head> <waived: 0|1>
# An empty <head> judges the working tree of <repo> against the merge base of
# <base> and HEAD; a named <head> judges the committed range <base>...<head>.
guard() {
  local repo="$1" base="$2" head="$3" waived="$4" status=0
  local pattern changed touched merge_base under cut fragments untracked range

  pattern="$(printf '%s|' "${USER_VISIBLE_PATHS[@]}")"
  pattern="^(${pattern%|})"
  # The change is always measured from the merge base, so commits that reached
  # the base branch afterwards are not counted against it. A committed range
  # spells that with three dots. The working tree is diffed against the merge
  # base itself, which takes in staged and unstaged edits, and untracked files
  # are listed beside it because `git diff` never sees them.
  if [[ -n "$head" ]]; then
    merge_base="$(git -C "$repo" merge-base "$base" "$head")"
    range="$base...$head"
    untracked=""
  else
    merge_base="$(git -C "$repo" merge-base "$base" HEAD)"
    range="$merge_base"
    untracked="$(git -C "$repo" ls-files --others --exclude-standard)"
  fi
  changed="$(
    git -C "$repo" diff --name-only "$range"
    [[ -z "$untracked" ]] || printf '%s\n' "$untracked"
  )"

  # Rule 1. The Unreleased section runs from its heading to the next `## `
  # heading. A non-blank line in it at head that it did not hold at the merge
  # base (counted, so a duplicated line is an added one) is refused. The
  # sections are compared, never the diff hunks: a release-candidate cut keeps
  # the Unreleased entries and writes the same lines again under its own
  # heading, and a diff may place either copy on its `+` side.
  if printf '%s\n' "$changed" | grep -qx 'CHANGELOG.md'; then
    if ! head_changelog "$repo" "$head" | unreleased_lines >/dev/null; then
      echo "::error::CHANGELOG.md at ${head:-the working tree} has no '## [Unreleased]' heading; the release cut re-adds it, and nothing else may remove it." >&2
      return 1
    fi
    under="$(
      {
        git -C "$repo" show "$merge_base:CHANGELOG.md" 2>/dev/null | unreleased_lines || true
        echo '--head--'
        head_changelog "$repo" "$head" | unreleased_lines
      } | awk '
        $0 == "--head--" { head = 1; next }
        { split($0, field, "\t"); text = substr($0, length(field[1]) + 2) }
        !head { held[text]++; next }
        held[text] > 0 { held[text]--; next }
        { print "  CHANGELOG.md:" field[1] ": " text }
      '
    )"
    if [[ -n "$under" ]]; then
      echo "::error::This change adds lines under the Unreleased heading of CHANGELOG.md. Entries are fragments now: move the text to changelog.d/<issue>-<slug>.<section>.md (changelog.d/README.md). No label waives this." >&2
      printf '%s\n' "$under" >&2
      status=1
    fi
    cut="$(git -C "$repo" diff -U0 "$range" -- CHANGELOG.md |
      grep -cE '^\+## \[[0-9]+\.[0-9]+\.[0-9]+(-rc[1-9][0-9]*)?\] - ' || true)"
  else
    cut=0
  fi

  # Rule 2.
  touched="$(printf '%s\n' "$changed" | grep -E "$pattern" || true)"
  if [[ -z "$touched" ]]; then
    echo "changelog-entry: no user-visible surface touched — a fragment is not required."
    return "$status"
  fi
  echo "changelog-entry: user-visible paths in this change:"
  printf '%s\n' "$touched" | sed 's/^/  /'
  if [[ "$waived" -eq 1 ]]; then
    echo "::notice::the 'no-changelog' label is applied — the fragment requirement is waived for this pull request (the Unreleased refusal is not)"
    return "$status"
  fi
  if [[ "${cut:-0}" -gt 0 ]]; then
    echo "changelog-entry: a release cut — the fragments move into the new CHANGELOG.md section."
    return "$status"
  fi
  fragments="$(
    {
      git -C "$repo" diff --name-only --no-renames --diff-filter=A "$range" -- changelog.d/
      printf '%s\n' "$untracked" | grep '^changelog\.d/' || true
    } | grep -vx 'changelog.d/README.md' | grep . || true
  )"
  if [[ -n "$fragments" ]]; then
    echo "changelog-entry: fragment(s) added — OK:"
    printf '%s\n' "$fragments" | sed 's/^/  /'
    return "$status"
  fi
  echo "::error::This change touches a user-visible surface but adds no changelog fragment. Add changelog.d/<issue>-<slug>.<section>.md (the format is in changelog.d/README.md), or apply the 'no-changelog' label if the change is genuinely invisible." >&2
  return 1
}

self_test() {
  local tmp fail=0 n=0
  tmp="$(mktemp -d)"
  # shellcheck disable=SC2064 # expanded now on purpose: the path is fixed for this run
  trap "rm -rf '$tmp'" EXIT
  local repo="$tmp/repo"
  git init -q -b main "$repo"
  commit() { # <message>; the scratch repository is throwaway and never signed
    git -C "$repo" add -A
    git -C "$repo" -c commit.gpgsign=false -c user.name=self-test \
      -c user.email=self-test@example.invalid commit -q -m "$1"
  }
  mkdir -p "$repo/changelog.d" "$repo/app/veredictum/src" "$repo/catalogue/cases/ehr"
  printf 'workflows: []\n' >"$repo/catalogue/cases/ehr/ehr.arazzo.yaml"
  printf 'cases: {}\n' >"$repo/catalogue/id-map.yaml"
  printf 'The format.\n' >"$repo/changelog.d/README.md"
  printf -- '- An older fragment.\n' >"$repo/changelog.d/1-old.added.md"
  printf 'fn main() {}\n' >"$repo/app/veredictum/src/main.rs"
  printf '[package]\nversion = "0.1.0"\n' >"$repo/Cargo.toml"
  printf 'readme\n' >"$repo/README.md"
  cat >"$repo/CHANGELOG.md" <<'EOF'
# Changelog

Intro prose.

## [Unreleased]

### Added

- An entry already here (#2).

## [0.1.0] - 2026-01-01

### Added

- The first release.

[unreleased]: https://example.invalid/r/compare/v0.1.0...HEAD
[0.1.0]: https://example.invalid/r/compare/v0.0.9...v0.1.0
EOF
  commit base

  # case <label> <want exit> <waived 0|1> <shell that mutates the tree>
  case_() {
    local label="$1" want="$2" waived="$3" got=0
    n=$((n + 1))
    git -C "$repo" checkout -q -b "case$n" main
    (cd "$repo" && eval "$4")
    commit "case $n"
    guard "$repo" main "case$n" "$waived" >"$tmp/out" 2>&1 || got=$?
    if [[ "$got" -ne "$want" ]]; then
      echo "self-test: '$label' exited $got, wanted $want:" >&2
      sed 's/^/  /' "$tmp/out" >&2
      fail=1
    fi
    git -C "$repo" checkout -q main
  }
  local src='echo "// x" >> app/veredictum/src/main.rs'
  local frag='printf -- "- New.\n" > changelog.d/9-new.fixed.md'
  local under='awk '\''{ print } /^- An entry already here/ { print ""; print "- A hand-written entry." }'\'' CHANGELOG.md > c && mv c CHANGELOG.md'

  case_ "a user-visible change with no fragment" 1 0 "$src"
  case_ "a user-visible change with a fragment" 0 0 "$src && $frag"
  case_ "a fragment, plus a line under Unreleased" 1 0 "$src && $frag && $under"
  case_ "a user-visible change waived by the label" 0 1 "$src"
  case_ "a line under Unreleased, with the label" 1 1 "$under"
  case_ "a line under Unreleased, no code" 1 0 "$under"
  case_ "a user-visible change that edits only the changelog.d/ README" 1 0 "$src && echo more >> changelog.d/README.md"
  case_ "prose only" 0 0 "echo more >> README.md"
  case_ "a catalogue-only change with no fragment" 1 0 \
    "echo '# x' >> catalogue/cases/ehr/ehr.arazzo.yaml"
  case_ "a catalogue-only change with a fragment" 0 0 \
    "echo '# x' >> catalogue/cases/ehr/ehr.arazzo.yaml && $frag"
  case_ "an id-map-only change with no fragment" 1 0 "echo '# x' >> catalogue/id-map.yaml"
  case_ "an edit inside an older release section" 0 0 \
    "sed -i.bak 's/^- The first release\.\$/- The first release, reworded./' CHANGELOG.md && rm CHANGELOG.md.bak"
  case_ "an edit to the intro above Unreleased" 0 0 \
    "sed -i.bak 's/^Intro prose\.\$/Intro prose, reworded./' CHANGELOG.md && rm CHANGELOG.md.bak"
  case_ "an entry deleted from under Unreleased" 0 0 \
    "sed -i.bak '/^- An entry already here/d' CHANGELOG.md && rm CHANGELOG.md.bak"
  case_ "the Unreleased heading removed" 1 0 \
    "sed -i.bak 's/^## \\[Unreleased\\]\$/## Next/' CHANGELOG.md && rm CHANGELOG.md.bak"
  case_ "a release cut" 0 0 "$(cat <<'CUT'
printf '[package]\nversion = "0.2.0"\n' > Cargo.toml
rm changelog.d/1-old.added.md
awk '
  /^## \[Unreleased\]$/ { print; print ""; print "## [0.2.0] - 2026-10-28"; next }
  /^- An entry already here/ { print; print ""; print "- An older fragment. (#1)"; next }
  /^\[unreleased\]:/ { print "[unreleased]: https://example.invalid/r/compare/v0.2.0...HEAD"; print "[0.2.0]: https://example.invalid/r/compare/v0.1.0...v0.2.0"; next }
  { print }
' CHANGELOG.md > c && mv c CHANGELOG.md
CUT
)"

  # A release-candidate cut keeps the Unreleased entries and the fragments, and
  # writes the same entries again under its own heading.
  local candidate
  candidate="$(cat <<'CUT'
printf '[package]\nversion = "0.2.0-rc1"\n' > Cargo.toml
awk '
  /^## \[0\.1\.0\]/ { print "## [0.2.0-rc1] - 2026-10-28"; print ""; print "### Added"; print ""; print "- An entry already here (#2)."; print "- An older fragment. (#1)"; print "" }
  /^\[unreleased\]:/ { print "[unreleased]: https://example.invalid/r/compare/v0.2.0-rc1...HEAD"; print "[0.2.0-rc1]: https://example.invalid/r/compare/v0.1.0...v0.2.0-rc1"; next }
  { print }
' CHANGELOG.md > c && mv c CHANGELOG.md
CUT
)"
  case_ "a release-candidate cut" 0 0 "$candidate"
  case_ "a release-candidate cut, plus a line under Unreleased" 1 0 "$candidate && $under"
  case_ "an Unreleased entry duplicated" 1 0 \
    "awk '{ print } /^- An entry already here/ { print }' CHANGELOG.md > c && mv c CHANGELOG.md"
  case_ "a heading that is not a cut" 1 0 "$src && $(cat <<'NOTCUT'
awk '/^## \[0\.1\.0\]/ { print "## [0.2.0-beta1] - 2026-10-28"; print ""; print "### Added"; print ""; print "- A beta."; print "" } { print }' CHANGELOG.md > c && mv c CHANGELOG.md
NOTCUT
)"

  # The working tree, judged before a commit (#665). Each case starts a branch
  # from main, leaves its change uncommitted, and is wiped afterwards. A
  # non-empty <move-base> shell is committed on a branch past main first, and
  # that branch is the base the guard reads: it has moved past the branch
  # point the way another worktree's fetch moves origin/main.
  # wt_case_ <label> <want exit> <move-base shell, or ""> <shell for the tree>
  wt_case_() {
    local label="$1" want="$2" got=0 base=main
    n=$((n + 1))
    if [[ -n "$3" ]]; then
      git -C "$repo" checkout -q -b "moved$n" main
      (cd "$repo" && eval "$3")
      commit "base moved $n"
      base="moved$n"
    fi
    git -C "$repo" checkout -q -b "wt$n" main
    (cd "$repo" && eval "$4")
    guard "$repo" "$base" "" 0 >"$tmp/out" 2>&1 || got=$?
    if [[ "$got" -ne "$want" ]]; then
      echo "self-test: '$label' exited $got, wanted $want:" >&2
      sed 's/^/  /' "$tmp/out" >&2
      fail=1
    fi
    git -C "$repo" reset -q --hard
    git -C "$repo" clean -qfd
    git -C "$repo" checkout -q main
  }
  local stage='git add -A'
  local moved_src='echo "// upstream" >> app/veredictum/src/main.rs'
  local moved_frag='printf -- "- Upstream.\n" > changelog.d/8-upstream.fixed.md'

  wt_case_ "an uncommitted change with no fragment" 1 "" "$src"
  wt_case_ "an uncommitted change with an untracked fragment" 0 "" "$src && $frag"
  wt_case_ "a staged change with a staged fragment" 0 "" "$src && $frag && $stage"
  wt_case_ "an uncommitted line under Unreleased" 1 "" "$frag && $under"
  wt_case_ "uncommitted prose only" 0 "" "echo more >> README.md"
  wt_case_ "prose only, after origin/main moved a user-visible path" 0 "$moved_src" \
    "echo more >> README.md"
  wt_case_ "a change with no fragment, after origin/main gained one" 1 \
    "$moved_src && $moved_frag" "$src"
  wt_case_ "a change with a fragment, after origin/main moved" 0 "$moved_src" "$src && $frag"

  [[ "$fail" -eq 0 ]] || { echo "changelog-entry: self-test FAILED" >&2; return 1; }
  echo "changelog-entry: self-test OK ($n seeded changes judged as expected)."
}

if [[ "${1:-}" == "--self-test" ]]; then
  [[ $# -eq 1 ]] || { echo "changelog-entry: --self-test takes no arguments" >&2; exit 2; }
  self_test
  exit $?
fi

waived=0
refs=()
for arg in "$@"; do
  case "$arg" in
    --no-changelog-label) waived=1 ;;
    -*) echo "changelog-entry: unknown option $arg" >&2; exit 2 ;;
    *) refs+=("$arg") ;;
  esac
done
[[ ${#refs[@]} -le 2 ]] || { echo "changelog-entry: at most two refs, <base> and <head>" >&2; exit 2; }
# No head ref is the local run: the working tree, before or after a commit.
# The workflow passes both refs and judges the pull request's committed range.
guard "$(cd "$(dirname "$SELF")/../.." && pwd)" "${refs[0]:-origin/main}" "${refs[1]:-}" "$waived"
