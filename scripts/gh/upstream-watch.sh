#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# scripts/gh/upstream-watch.sh — one tracking issue per openEHR specification
# repository and signal, whenever upstream moves past what this repository
# pins (#975). Run daily by .github/workflows/upstream-watch.yml.
#
# THE PINS. The two COMPONENTS tables are the only source, read at run time
# and never copied here:
#   scripts/vendor/spec-docs-development.sh  component|repo|branch|commit
#   scripts/vendor/spec-docs.sh              component|repo|human ref|commit
# A row that is not four `|` fields ending in a 40-hex commit fails the run.
#
# THE TWO SIGNALS, per repository:
#   master moved: `git ls-remote` reads the head of `master`; a head that is
#     not the development pin is the signal. Repositories absent from the
#     development table (TERM, LANG, CNF today) have no development pin and
#     get no master signal.
#   new release: `git ls-remote --tags` reads the `Release-*` tags; a tag that
#     sorts after the released pin's tag (`sort -V`) is the signal. Only a
#     released row whose third field STARTS WITH `Release-` is tag-pinned, its
#     first word being the tag. The other rows (CNF, ITS-JSON, ITS-XML) are
#     frozen master SHAs, never released or tag-only for their XSDs, and get
#     no release signal.
# The GitHub compare API (read-only) lists the commits and files between the
# pin and the new head or tag. Nothing is ever written outside this
# repository's issues.
#
# ONE ISSUE PER REPOSITORY AND SIGNAL, found by title among the issues
# labelled `upstream-change`:
#   openEHR <repo>: master moved past the development snapshot
#   openEHR <repo>: <tag> released
# The body ends in a marker naming what it reports. The same head or tag
# again: nothing is written. A newer one: the title and body are rewritten and
# a comment names the commits the old body did not list. A pin that caught up
# (re-vendored, or the released table moved to the tag): the open issue is
# closed with a comment. An issue closed by hand at the very head or tag
# reported now is not reopened or filed again; the next move files anew.
# Labels `upstream-change`, `chore` and the component's `spec:` label (ITS-*
# map to `spec:ITS`; LANG has no label). The type Task goes through the REST
# issue update, the one field a workflow token may set; the Priority field
# needs an organisation read that token is refused, so pickup sets it with
# scripts/gh/fields.sh. No milestone: triage decides what the move means.
#
# Run colour: a probe that cannot answer (ls-remote empty, the compare API
# failing) fails the run. A finding files or updates an issue and exits 0.
#
# Policy: no openEHR spec governs this — our own design.
#
# Usage:
#   scripts/gh/upstream-watch.sh             # read upstream, write issues
#   scripts/gh/upstream-watch.sh --dry-run   # print what it would write; no write
#   scripts/gh/upstream-watch.sh --self-test # drive it against a stub git and gh
# Env: GH_REPO / GH_TOKEN as gh reads them. UPSTREAM_WATCH_ROOT,
# UPSTREAM_WATCH_DEV_TABLE and UPSTREAM_WATCH_REL_TABLE move the tree and the
# two tables (the self-test seeds them).

# shellcheck disable=SC2016 # every backtick in a single-quoted string is markdown code, and every $x in a jq filter a jq binding
set -euo pipefail

SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
DEFAULT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

die() {
  echo "upstream-watch: $*" >&2
  exit 1
}

usage() {
  sed -n '/^# Usage:/,/^# Env:/p' "$SELF" | sed '$d' | sed 's/^# \{0,1\}//' >&2
  exit 2
}

# read_table <file>: the table's rows, one `a|b|c|d` per line, validated.
read_table() {
  local file="$1" rows row name repo ref sha extra
  [[ -f "$file" ]] || die "no table at $file"
  rows="$(awk '/^COMPONENTS=\(/{on=1; next} on && /^\)/{exit} on' "$file" |
    grep -oE '"[^"]*"' | tr -d '"')" || true
  [[ -n "$rows" ]] || die "no COMPONENTS rows in $file"
  while IFS= read -r row; do
    IFS='|' read -r name repo ref sha extra <<<"$row"
    if [[ -z "$name" || -z "$repo" || -z "$ref" || -n "${extra:-}" ]] ||
      ! [[ "$sha" =~ ^[0-9a-f]{40}$ ]]; then
      die "a malformed COMPONENTS row in $file: $row"
    fi
  done <<<"$rows"
  printf '%s\n' "$rows"
}

# spec_label <component>: the component's `spec:` label, or nothing for LANG.
spec_label() {
  case "$1" in
    AM | BASE | QUERY | RM | TERM | CNF) printf 'spec:%s' "$1" ;;
    ITS-REST | ITS-XML | ITS-JSON) printf 'spec:ITS' ;;
    LANG) ;;
    *) die "no spec: label is known for the component '$1'; add it to spec_label" ;;
  esac
}

# highlights <root> <released rows>: `repo<TAB>path<TAB>why` for every file a
# catalogue citation or an upstream/ patch names, sorted and unique.
highlights() {
  local root="$1" rel_rows="$2" hit comp rest repo patch
  {
    if [[ -d "$root/catalogue" ]]; then
      grep -rhoE 'specs/openehr(-development)?/[A-Za-z0-9._/+-]+' "$root/catalogue" 2>/dev/null |
        sort -u | while IFS= read -r hit; do
        case "$hit" in
          specs/openehr-development/*)
            rest="${hit#specs/openehr-development/}"
            printf '%s\t%s\tcited by the catalogue\n' "${rest%%/*}" "${rest#*/}"
            ;;
          specs/openehr/*/*)
            rest="${hit#specs/openehr/}"
            comp="${rest%%/*}"
            repo="$(printf '%s\n' "$rel_rows" | awk -F'|' -v c="$comp" '$1==c{print $2; exit}')"
            [[ -n "$repo" ]] && printf '%s\t%s\tcited by the catalogue\n' "$repo" "${rest#*/}"
            ;;
          *) ;;
        esac
      done
    fi
    if [[ -d "$root/upstream" ]]; then
      for patch in "$root"/upstream/*/*.patch; do
        [[ -f "$patch" ]] || continue
        repo="$(basename "$(dirname "$patch")")"
        sed -nE 's#^(\+\+\+ b|--- a)/(.*)$#\2#p' "$patch" | sed 's/[[:space:]]*$//' |
          while IFS= read -r rest; do
            printf '%s\t%s\tpatched under upstream/%s/\n' "$repo" "$rest" "$repo"
          done
      done
    fi
  } | LC_ALL=C sort -u
}

# render_body <compare.json> <repo> <signal line> <pin line> <head line>
# <compare url> <marker> <highlights file>: the issue body, on stdout.
render_body() {
  local compare="$1" repo="$2" signal="$3" pin="$4" head="$5" url="$6" marker="$7" hl="$8"
  jq -r --arg repo "$repo" --arg signal "$signal" --arg pin "$pin" --arg head "$head" \
    --arg url "$url" --arg marker "$marker" --rawfile hl "$hl" '
    def code: "`" + (tostring | gsub("`"; "'"'"'") | gsub("[\r\n]+"; " ")) + "`";
    def cell: gsub("\\|"; "\\|");
    ($hl | split("\n") | map(select(length > 0) | split("\t"))
      | map(select(.[0] == $repo)) | map({key: .[1], value: .[2]})
      | group_by(.key) | map({key: .[0].key, value: (map(.value) | join("; "))})
      | from_entries) as $why
    | (.commits // []) as $commits
    | ([.files[]?.filename] | unique) as $files
    | ($files | map(select($why[.] != null))) as $named
    | ($files | map(select($why[.] == null))) as $other
    | [
        "The openEHR repository [`openEHR/\($repo)`](https://github.com/openEHR/\($repo)) moved past what this repository pins. The scheduled upstream watch filed this issue; the book'"'"'s methodology page, section *Keeping the pinned text current*, says what to do with it.",
        "",
        "- Signal: \($signal)",
        "- Pin: \($pin)",
        "- Upstream: \($head)",
        "- Compare: \($url)",
        "- Priority: set at pickup with `scripts/gh/fields.sh`",
        "",
        "## Commits since the pin (\(.total_commits // ($commits | length)))",
        "",
        "| Commit | Date | Author | First line |",
        "|---|---|---|---|",
        ($commits | reverse | .[:100][] |
          "| [`\(.sha[:12])`](https://github.com/openEHR/\($repo)/commit/\(.sha)) | \(.commit.author.date // "" | .[:10]) | \((.author.login // .commit.author.name // "unknown") | code | cell) | \(.commit.message | split("\n")[0] | code | cell) |"),
        (if ($commits | length) > 100 then "", "\(($commits | length) - 100) older commits are not listed here; the compare link has them." else empty end),
        (if (.total_commits // 0) > ($commits | length) then "", "The compare API returned \($commits | length) of \(.total_commits) commits." else empty end),
        "",
        "## Files a catalogue citation or an `upstream/` patch names (\($named | length))",
        "",
        (if ($named | length) == 0 then "None." else ($named[] | "- `\(.)`: \($why[.])") end),
        "",
        "## Other files touched (\($other | length))",
        "",
        (if ($other | length) == 0 then "None." else ($other[:200][] | "- `\(.)`") end),
        (if ($other | length) > 200 then "", "\(($other | length) - 200) more are not listed here." else empty end),
        (if ($files | length) >= 300 then "", "The compare API lists at most 300 files, so this list may be incomplete." else empty end),
        "",
        "<!-- upstream-watch: \($marker) -->"
      ] | join("\n")' "$compare"
}

# new_commits <compare.json> <old body> <repo>: the commits the old body does
# not name, as markdown lines.
new_commits() {
  jq -r --arg old "$2" --arg repo "$3" '
    [(.commits // [])[] | select(.sha as $s | $old | contains($s) | not)]
    | if length == 0 then "No commit is new to the previous body." else
        (.[] | "- [`\(.sha[:12])`](https://github.com/openEHR/\($repo)/commit/\(.sha)) `\(.commit.message | split("\n")[0] | gsub("`"; "'"'"'"))`")
      end' "$1"
}

marker_of() { # the marker a body carries, or nothing
  printf '%s' "$1" | sed -nE 's/.*<!-- upstream-watch: (.*) -->.*/\1/p' | tail -n 1
}

# The state every action below shares.
DRY_RUN=0
WORK=""
ISSUES=""
HIGHLIGHTS=""

# act <verb> <description> <command…>: run a write, or say it in a dry run.
act() {
  local what="$1"
  shift
  if [[ "$DRY_RUN" = 1 ]]; then
    echo "  DRY-RUN would $what"
  else
    "$@" >/dev/null
    echo "  $what"
  fi
}

# issue_field <title-prefix> <title-suffix> <state> <field>: the field of the
# newest issue in that state whose title has that prefix and suffix.
issue_field() {
  jq -r --arg p "$1" --arg s "$2" --arg st "$3" --arg f "$4" '
    map(select((.title | startswith($p)) and (.title | endswith($s)) and (.state | ascii_upcase) == $st))
    | sort_by(.number) | last | if . == null then "" else .[$f] | tostring end' "$ISSUES"
}

# compare <repo> <base> <head> <out>: the compare API answer, or a red run.
compare() {
  gh api "repos/openEHR/$1/compare/$2...$3" >"$4" ||
    die "the compare API refused openEHR/$1 $2...$3"
  jq -e '.commits | type == "array"' "$4" >/dev/null ||
    die "the compare API answered openEHR/$1 $2...$3 without a commit list"
}

# signal <repo> <component> <title> <prefix> <suffix> <want> <signal line>
# <pin line> <head line> <base> <head> <marker>: file, update or close the one
# issue of this repository and signal. <want> is 1 when upstream is ahead.
signal() {
  local repo="$1" comp="$2" title="$3" prefix="$4" suffix="$5" want="$6"
  local sig="$7" pinl="$8" headl="$9" base="${10}" head="${11}" marker="${12}"
  local open closed_marker old_body old_marker body comment label labels url n
  open="$(issue_field "$prefix" "$suffix" OPEN number)"
  if [[ "$want" = 0 ]]; then
    if [[ -n "$open" ]]; then
      act "close #$open: $title" gh issue close "$open" --comment \
        "The pin caught up: $pinl now matches upstream, so this move is vendored. Closed by the scheduled upstream watch."
    fi
    return 0
  fi
  if [[ -n "$open" ]]; then
    old_body="$(issue_field "$prefix" "$suffix" OPEN body)"
    old_marker="$(marker_of "$old_body")"
    if [[ "$old_marker" = "$marker" ]]; then
      echo "  unchanged: #$open already reports $marker"
      return 0
    fi
  else
    closed_marker="$(marker_of "$(issue_field "$title" "" CLOSED body)")"
    if [[ "$closed_marker" = "$marker" ]]; then
      echo "  unchanged: an issue closed by hand already reported $marker"
      return 0
    fi
  fi
  compare "$repo" "$base" "$head" "$WORK/compare.json"
  body="$WORK/body.md"
  render_body "$WORK/compare.json" "$repo" "$sig" "$pinl" "$headl" \
    "https://github.com/openEHR/$repo/compare/$base...$head" "$marker" "$HIGHLIGHTS" >"$body"
  if [[ -n "$open" ]]; then
    comment="$WORK/comment.md"
    {
      printf 'Upstream moved again: %s (was `%s`). Commits the previous body did not list:\n\n' "$headl" "$old_marker"
      new_commits "$WORK/compare.json" "$old_body" "$repo"
    } >"$comment"
    act "rewrite #$open: $title" gh issue edit "$open" --title "$title" --body-file "$body"
    act "comment on #$open" gh issue comment "$open" --body-file "$comment"
    return 0
  fi
  label="$(spec_label "$comp")"
  labels=(--label upstream-change --label chore)
  [[ -n "$label" ]] && labels+=(--label "$label")
  if [[ "$DRY_RUN" = 1 ]]; then
    echo "  DRY-RUN would create: $title [upstream-change, chore${label:+, $label}] <Task>"
    echo "  DRY-RUN body: $(grep -c '^| \[' "$body" || true) commit rows, $(sed -n 's/^## Files a catalogue citation.*(\([0-9]*\))$/\1/p' "$body") named files, $(sed -n 's/^## Other files touched (\([0-9]*\))$/\1/p' "$body") other files"
    sed 's/^/  | /' "$body"
    return 0
  fi
  url="$(gh issue create --title "$title" --body-file "$body" "${labels[@]}")"
  n="${url##*/}"
  echo "  created #$n: $title"
  if ! gh api --method PATCH "repos/{owner}/{repo}/issues/$n" -f type=Task --silent; then
    echo "::warning::#$n was filed without its issue type (Task); set it with scripts/gh/fields.sh"
  fi
  echo "  priority: set at pickup (scripts/gh/fields.sh priority $n <level>); a workflow token may not read the organisation's fields"
}

# ls_remote <repo> <heads|tags> <pattern>: the answer, or a red run when it
# is empty.
ls_remote() {
  local repo="$1" kind="$2" pattern="$3" out
  out="$(git ls-remote "--$kind" "https://github.com/openEHR/$repo.git" "$pattern" 2>/dev/null)" || out=""
  [[ -n "$out" ]] || die "git ls-remote answered nothing for openEHR/$repo $pattern"
  printf '%s\n' "$out"
}

watch() {
  local root dev_table rel_table dev_rows rel_rows repos repo comp row ref sha
  local head tags newest tag tag_sha want pin_tag
  root="${UPSTREAM_WATCH_ROOT:-$DEFAULT_ROOT}"
  dev_table="${UPSTREAM_WATCH_DEV_TABLE:-$root/scripts/vendor/spec-docs-development.sh}"
  rel_table="${UPSTREAM_WATCH_REL_TABLE:-$root/scripts/vendor/spec-docs.sh}"
  command -v jq >/dev/null 2>&1 || die "jq is not installed"
  command -v gh >/dev/null 2>&1 || die "the GitHub CLI (gh) is not installed"
  dev_rows="$(read_table "$dev_table")"
  rel_rows="$(read_table "$rel_table")"

  WORK="$(mktemp -d)"
  trap 'rm -rf "$WORK"' EXIT
  ISSUES="$WORK/issues.json"
  HIGHLIGHTS="$WORK/highlights.tsv"
  highlights "$root" "$rel_rows" >"$HIGHLIGHTS"
  gh issue list --label upstream-change --state all --limit 500 \
    --json number,title,state,body >"$ISSUES" || die "could not list the upstream-change issues"

  repos="$( { printf '%s\n' "$dev_rows"; printf '%s\n' "$rel_rows"; } | cut -d'|' -f2 | LC_ALL=C sort -u)"
  while IFS= read -r repo; do
    echo "== openEHR/$repo"
    comp="$(printf '%s\n%s\n' "$rel_rows" "$dev_rows" | awk -F'|' -v r="$repo" '$2==r{print $1; exit}')"

    row="$(printf '%s\n' "$dev_rows" | awk -F'|' -v r="$repo" '$2==r' | head -n 1)"
    if [[ -z "$row" ]]; then
      echo "  master: no development pin, no master signal"
    else
      IFS='|' read -r _ _ ref sha <<<"$row"
      head="$(ls_remote "$repo" heads "refs/heads/$ref" | awk -v r="refs/heads/$ref" '$2==r{print $1}')"
      [[ "$head" =~ ^[0-9a-f]{40}$ ]] || die "no $ref head for openEHR/$repo"
      want=0
      [[ "$head" != "$sha" ]] && want=1
      echo "  master: pin ${sha:0:12}, head ${head:0:12}"
      signal "$repo" "$comp" "openEHR $repo: master moved past the development snapshot" \
        "openEHR $repo: master moved past the development snapshot" "" "$want" \
        "\`$ref\` moved past the development snapshot" \
        "\`$sha\` (scripts/vendor/spec-docs-development.sh)" "\`$head\`, the head of \`$ref\`" \
        "$sha" "$head" "$ref $head"
    fi

    row="$(printf '%s\n' "$rel_rows" | awk -F'|' -v r="$repo" '$2==r' | head -n 1)"
    if [[ -z "$row" ]]; then
      echo "  release: no released pin, no release signal"
      continue
    fi
    IFS='|' read -r _ _ ref sha <<<"$row"
    if [[ "$ref" != Release-* ]]; then
      echo "  release: pinned at a frozen SHA, not a Release- tag, no release signal"
      continue
    fi
    pin_tag="${ref%% *}"
    tags="$(ls_remote "$repo" tags 'refs/tags/Release-*')"
    newest="$(printf '%s\n' "$tags" | awk '{print $2}' | sed -e 's#^refs/tags/##' -e 's#\^{}$##' |
      grep -E '^Release-' | { cat; printf '%s\n' "$pin_tag"; } | sort -u | sort -V | tail -n 1)"
    want=0
    [[ "$newest" != "$pin_tag" ]] && want=1
    tag="$newest"
    tag_sha="$(printf '%s\n' "$tags" | awk -v p="refs/tags/$tag^{}" '$2==p{print $1}')"
    [[ -n "$tag_sha" ]] || tag_sha="$(printf '%s\n' "$tags" | awk -v p="refs/tags/$tag" '$2==p{print $1}')"
    echo "  release: pin $pin_tag, newest $newest"
    if [[ "$want" = 1 && ! "$tag_sha" =~ ^[0-9a-f]{40}$ ]]; then
      die "no commit for the tag $tag of openEHR/$repo"
    fi
    signal "$repo" "$comp" "openEHR $repo: $tag released" "openEHR $repo: Release-" " released" "$want" \
      "a release tag newer than the released pin" \
      "\`$pin_tag\` at \`$sha\` (scripts/vendor/spec-docs.sh)" "\`$tag\` at \`$tag_sha\`" \
      "$sha" "$tag_sha" "$tag $tag_sha"
  done <<<"$repos"
}

# The self-test answers every git and gh call from stubs on PATH, over seeded
# tables and a seeded tree, and judges what was written.
self_test() {
  command -v jq >/dev/null 2>&1 || die "jq is not installed"
  local work stub fx pin_a head_a newer_a pin_r tag_r
  work="$(mktemp -d)"
  stub="$work/bin"
  fx="$work/fx"
  mkdir -p "$stub" "$fx/lsremote" "$fx/compare" "$work/root/catalogue" "$work/root/upstream/specifications-AM"
  pin_a="1111111111111111111111111111111111111111"
  head_a="2222222222222222222222222222222222222222"
  newer_a="3333333333333333333333333333333333333333"
  pin_r="4444444444444444444444444444444444444444"
  tag_r="5555555555555555555555555555555555555555"

  cat >"$stub/git" <<'STUB'
#!/usr/bin/env bash
# The git a self-test run of scripts/gh/upstream-watch.sh speaks to.
set -euo pipefail
printf 'git %s\n' "$*" >> "$STUB_CALLS"
[[ "${1:-}" = ls-remote ]] || { echo "stub: unexpected git $*" >&2; exit 90; }
url=""; tags=0
for a in "$@"; do
  case "$a" in
    --tags) tags=1 ;;
    https://*) url="$a" ;;
    refs/*) [[ -n "$url" ]] || { echo "stub: a pattern before the URL" >&2; exit 91; } ;;
    *) ;;
  esac
done
repo="$(basename "$url" .git)"
f="$STUB_FX/lsremote/$repo"
[[ -f "$f" ]] || exit 0
if [[ "$tags" = 1 ]]; then grep 'refs/tags/' "$f" || true; else grep 'refs/heads/' "$f" || true; fi
STUB
  cat >"$stub/gh" <<'STUB'
#!/usr/bin/env bash
# The gh a self-test run of scripts/gh/upstream-watch.sh speaks to.
set -euo pipefail
printf 'gh %s\n' "$*" >> "$STUB_CALLS"
previous=""
for a in "$@"; do
  if [[ "$previous" = --body-file ]]; then cat "$a" >> "$STUB_BODIES"; fi
  previous="$a"
done
case "${1:-} ${2:-}" in
  "issue list") cat "$STUB_FX/issues.json" ;;
  "issue create") printf 'gh %s\n' "$*" >> "$STUB_WRITES"; echo "https://github.com/Example-Org/Example/issues/900" ;;
  "issue edit" | "issue comment" | "issue close") printf 'gh %s\n' "$*" >> "$STUB_WRITES" ;;
  "api --method") printf 'gh %s\n' "$*" >> "$STUB_WRITES" ;;
  "api repos/"*)
    f="$STUB_FX/compare/$(printf '%s' "$2" | tr '/.' '__')"
    [[ -f "$f" ]] || { echo "stub: no compare fixture for $2" >&2; exit 1; }
    cat "$f"
    ;;
  *) echo "stub: unexpected gh $*" >&2; exit 90 ;;
esac
STUB
  chmod 0755 "$stub/git" "$stub/gh"

  printf 'COMPONENTS=(\n  "AM|specifications-AM|master|%s"\n)\n' "$pin_a" >"$work/dev.sh"
  printf 'COMPONENTS=(\n  "AM|specifications-AM|Release-2.3.0|%s"\n  "LANG|specifications-LANG|Release-1.0.0 (a note)|%s"\n  "CNF|specifications-CNF|master (frozen SHA)|%s"\n)\n' \
    "$pin_r" "$pin_r" "$pin_r" >"$work/rel.sh"
  printf 'source: specs/openehr/AM/docs/cited.adoc\n' >"$work/root/catalogue/case.yaml"
  printf -- '--- a/docs/patched.adoc\n+++ b/docs/patched.adoc\n' >"$work/root/upstream/specifications-AM/0001-x.patch"

  # compare_fixture <base> <head> <sha…>: a compare answer touching the cited,
  # the patched and one other file.
  compare_fixture() {
    local base="$1" head="$2" s rows=""
    shift 2
    for s in "$@"; do
      rows+="${rows:+,}{\"sha\":\"$s\",\"author\":{\"login\":\"someone\"},\"commit\":{\"author\":{\"date\":\"2026-10-30T00:00:00Z\",\"name\":\"S\"},\"message\":\"Fix #12 for @someone\\n\\nbody\"}}"
    done
    printf '{"status":"ahead","total_commits":%s,"commits":[%s],"files":[{"filename":"docs/cited.adoc"},{"filename":"docs/patched.adoc"},{"filename":"docs/other.adoc"}]}' \
      "$#" "$rows" >"$fx/compare/repos_openEHR_specifications-AM_compare_${base}___${head}"
  }
  compare_fixture "$pin_a" "$head_a" "$head_a"
  compare_fixture "$pin_a" "$newer_a" "$head_a" "$newer_a"
  compare_fixture "$pin_r" "$tag_r" "$tag_r"

  # lsremote <master head> <tag lines…>: the seeded answer for AM.
  lsremote() {
    local head="$1" line
    shift
    printf '%s\trefs/heads/master\n' "$head" >"$fx/lsremote/specifications-AM"
    for line in "$@"; do printf '%s\n' "$line" >>"$fx/lsremote/specifications-AM"; done
    printf '%s\trefs/heads/master\n%s\trefs/tags/Release-1.0.0\n' "$pin_r" "$pin_r" >"$fx/lsremote/specifications-LANG"
    printf '%s\trefs/heads/master\n' "$pin_r" >"$fx/lsremote/specifications-CNF"
  }
  # issues <json>: the seeded issue list.
  issues() { printf '%s' "$1" >"$fx/issues.json"; }
  # run <name> <code> <args…>: this program over the stubs.
  run() {
    local name="$1" code="$2" rc=0
    shift 2
    : >"$work/calls"; : >"$work/writes"; : >"$work/bodies"
    PATH="$stub:$PATH" STUB_CALLS="$work/calls" STUB_WRITES="$work/writes" STUB_BODIES="$work/bodies" \
      STUB_FX="$fx" UPSTREAM_WATCH_ROOT="$work/root" \
      UPSTREAM_WATCH_DEV_TABLE="$work/dev.sh" UPSTREAM_WATCH_REL_TABLE="$work/rel.sh" \
      bash "$SELF" "$@" >"$work/out" 2>"$work/err" || rc=$?
    if [[ "$rc" -ne "$code" ]]; then
      echo "upstream-watch: self-test failed: $name exited $rc, wanted $code." >&2
      cat "$work/out" "$work/err" "$work/calls" >&2
      exit 1
    fi
  }
  fail() {
    echo "upstream-watch: self-test failed: $1" >&2
    cat "$work/out" "$work/err" "$work/writes" >&2
    exit 1
  }
  has() { grep -qF -- "$3" "$2" || fail "$1: '$3' is not in $(basename "$2")"; }
  hasnt() { if grep -qF -- "$3" "$2"; then fail "$1: '$3' is in $(basename "$2")"; fi; }
  nowrite() { [[ ! -s "$work/writes" ]] || fail "$1 wrote to an issue"; }

  local tag_old="$pin_r	refs/tags/Release-2.3.0"
  local tag_new="$tag_r	refs/tags/Release-2.4.0"
  local open_master="openEHR specifications-AM: master moved past the development snapshot"

  # Nothing moved, and nothing is open: nothing is written.
  lsremote "$pin_a" "$tag_old"
  issues '[]'
  run "nothing moved" 0
  nowrite "nothing moved"
  has "nothing moved" "$work/out" "no master signal"
  has "nothing moved" "$work/out" "pinned at a frozen SHA"

  # The pins come from the two tables: the development pin reaches the compare
  # call, the released tag decides the release signal.
  lsremote "$head_a" "$tag_old"
  run "a new head" 0
  has "the development pin" "$work/calls" "compare/$pin_a...$head_a"
  has "a new head" "$work/writes" "issue create --title $open_master"
  has "a new head" "$work/writes" "--label upstream-change --label chore --label spec:AM"
  has "a new head" "$work/writes" "-f type=Task"
  has "the cited file" "$work/bodies" '- `docs/cited.adoc`: cited by the catalogue'
  has "the patched file" "$work/bodies" '- `docs/patched.adoc`: patched under upstream/specifications-AM/'
  has "the other file" "$work/bodies" '- `docs/other.adoc`'
  has "a mention kept inert" "$work/bodies" '`Fix #12 for @someone`'
  has "the marker" "$work/bodies" "<!-- upstream-watch: master $head_a -->"

  # The same head with its issue open: nothing is written.
  issues "[{\"number\":501,\"title\":\"$open_master\",\"state\":\"OPEN\",\"body\":\"x [$head_a] <!-- upstream-watch: master $head_a -->\"}]"
  run "the same head" 0
  nowrite "the same head"
  hasnt "the same head" "$work/calls" "compare/"

  # The same head, its issue closed by hand: nothing is filed again.
  issues "[{\"number\":501,\"title\":\"$open_master\",\"state\":\"CLOSED\",\"body\":\"<!-- upstream-watch: master $head_a -->\"}]"
  run "closed by hand" 0
  nowrite "closed by hand"

  # A newer head: the body is rewritten and the comment names the new commit.
  issues "[{\"number\":501,\"title\":\"$open_master\",\"state\":\"OPEN\",\"body\":\"x [$head_a] <!-- upstream-watch: master $head_a -->\"}]"
  lsremote "$newer_a" "$tag_old"
  run "a newer head" 0
  has "a newer head" "$work/writes" "issue edit 501 --title $open_master --body-file"
  has "a newer head" "$work/writes" "issue comment 501 --body-file"
  hasnt "a newer head" "$work/writes" "issue create"
  has "a newer head" "$work/bodies" "commit/$newer_a"
  has "a newer head" "$work/bodies" "Upstream moved again"

  # The same move in a dry run: nothing is written, and it says what it would do.
  run "a dry run" 0 --dry-run
  nowrite "a dry run"
  has "a dry run" "$work/out" "DRY-RUN would rewrite #501"

  # The pin caught up: the open issue is closed.
  lsremote "$pin_a" "$tag_old"
  run "the pin caught up" 0
  has "the pin caught up" "$work/writes" "issue close 501"

  # A newer release tag: a release issue, and LANG gets no spec: label.
  issues '[]'
  lsremote "$pin_a" "$tag_old" "$tag_new"
  run "a new release" 0
  has "a new release" "$work/writes" "issue create --title openEHR specifications-AM: Release-2.4.0 released"
  has "the released pin" "$work/calls" "compare/$pin_r...$tag_r"
  hasnt "LANG" "$work/writes" "spec:LANG"

  # That release tag reported, then the released pin moved to it: closed.
  issues '[{"number":502,"title":"openEHR specifications-AM: Release-2.4.0 released","state":"OPEN","body":"<!-- upstream-watch: Release-2.4.0 '"$tag_r"' -->"}]'
  run "the same release" 0
  nowrite "the same release"
  sed "s/Release-2.3.0/Release-2.4.0/" "$work/rel.sh" >"$work/rel2.sh" && mv "$work/rel2.sh" "$work/rel.sh"
  run "the released pin caught up" 0
  has "the released pin caught up" "$work/writes" "issue close 502"

  # A probe that answers nothing is a red run.
  : >"$fx/lsremote/specifications-AM"
  run "an empty ls-remote" 1
  has "an empty ls-remote" "$work/err" "answered nothing"

  # A malformed table row is a red run.
  printf 'COMPONENTS=(\n  "AM|specifications-AM|master|not-a-sha"\n)\n' >"$work/dev.sh"
  run "a malformed row" 1
  has "a malformed row" "$work/err" "malformed COMPONENTS row"

  # The real tables parse, and each holds a tag-pinned row and a commit pin.
  local dev rel
  dev="$(read_table "$DEFAULT_ROOT/scripts/vendor/spec-docs-development.sh")"
  rel="$(read_table "$DEFAULT_ROOT/scripts/vendor/spec-docs.sh")"
  printf '%s\n' "$dev" | grep -q '|master|' || fail "the development table has no master row"
  printf '%s\n' "$rel" | grep -q '|Release-' || fail "the released table has no Release- row"
  printf '%s\n%s\n' "$dev" "$rel" | cut -d'|' -f1 | sort -u | while IFS= read -r c; do
    spec_label "$c" >/dev/null
  done

  run "an unknown option" 2 --bogus
  rm -rf "$work"
  echo "upstream-watch: self-test OK."
}

case "$#:${1:-}" in
  0:) watch ;;
  1:--dry-run) DRY_RUN=1; watch ;;
  1:--self-test) self_test ;;
  *) usage ;;
esac
