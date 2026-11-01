#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Every open `upstream-report` issue carries its proposed fix, or records the
# `master` commit that already fixes the defect (#1029).
#
# A report never exists without its patch under `upstream/<openEHR
# repository>/`, named after the report (`0353-*.patch` for #353), unless
# `master` already fixes the defect and the report records the fixing commit.
# Thirty reports drifted out of that rule with nothing to notice, so this guard
# asks the tracker for every OPEN report and refuses one that has neither a
# patch nor a fixing-commit line in its body. The line is exact, on its own:
#
#   Fixed on master: specifications-<COMPONENT>@<40-hex commit>
#
# A closed report is the standing outbound record and is not checked. The
# tracker must answer: a failed listing stops the guard rather than passing it.
# Without a token and without a `gh` login there is no tracker to ask, and the
# guard says SKIPPED instead of passing quietly.
#
# Usage: scripts/checks/upstream-report-patches.sh               # this repository
#        scripts/checks/upstream-report-patches.sh --self-test   # seeded, no network
# Env:   GITHUB_REPOSITORY the repository to ask (default: Vernum-Projecten/Veredictum)
set -euo pipefail
cd "$(dirname "$0")/../.."

LIMIT=1000
MARKER='^Fixed on master: specifications-[A-Za-z0-9-]+@[0-9a-f]{40}$'

if ! command -v jq >/dev/null 2>&1; then
  echo "upstream-report-patches: jq is not installed" >&2
  exit 2
fi

check() { # $1: root holding upstream/, $2: the open reports as JSON; prints findings, returns 1 on any
  local root="$1" json="$2" n marker padded count found=0 seen=0
  if ! count="$(jq 'length' <<<"$json" 2>&1)"; then
    echo "upstream-report-patches: the tracker's answer is not a JSON list: ${count}" >&2
    return 1
  fi
  if ((count >= LIMIT)); then
    echo "upstream-report-patches: ${count} open reports reach the listing limit of ${LIMIT}; raise it" >&2
    return 1
  fi
  while IFS=$'\t' read -r n marker; do
    [[ -n "$n" ]] || continue
    seen=$((seen + 1))
    printf -v padded '%04d' "$n"
    if compgen -G "$root/upstream/*/${padded}-*.patch" >/dev/null; then
      continue
    fi
    if [[ -n "$marker" ]]; then
      continue
    fi
    echo "upstream-report-patches: #${n} has no upstream/<repository>/${padded}-*.patch and records no fixing commit (a line 'Fixed on master: specifications-<COMPONENT>@<40-hex commit>')" >&2
    found=1
  done < <(jq -r --arg re "$MARKER" '.[] | [.number,
      ([(.body // "") | split("\n")[] | rtrimstr("\r") | select(test($re))] | first // "")] | @tsv' <<<"$json" |
    sort -n)
  echo "upstream-report-patches: ${seen} open upstream report(s) checked against ${root}/upstream/"
  return "$found"
}

run() { # $1: root, $2: repository; asks the tracker, then checks
  local root="$1" repo="$2" json
  if [[ -z "${GH_TOKEN:-}" && -z "${GITHUB_TOKEN:-}" ]] && ! gh auth status >/dev/null 2>&1; then
    echo "SKIPPED: gh is not authenticated"
    return 0
  fi
  if ! json="$(gh issue list --repo "$repo" --label upstream-report --state open \
    --limit "$LIMIT" --json number,body 2>&1)"; then
    echo "upstream-report-patches: the tracker did not answer: ${json}" >&2
    exit 2
  fi
  check "$root" "$json"
}

if [[ "${1:-}" == "--self-test" ]]; then
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  sha="0123456789abcdef0123456789abcdef01234567"
  mkdir -p "$tmp/bin" "$tmp/tree/upstream/specifications-X"
  : > "$tmp/tree/upstream/specifications-X/0007-has-a-patch.patch"
  : > "$tmp/tree/upstream/specifications-X/00090-five-digits.patch"
  # 7 has a patch; 8, 12 and 13 record a fixing commit (12 with CRLF line
  # endings); 9 has neither, and 00090 is not its patch; 10 has a short commit,
  # 11 the marker inside a sentence, 14 no body at all.
  jq -n --arg sha "$sha" '[
    {number: 7, body: "A report."},
    {number: 8, body: ("Summary.\n\nFixed on master: specifications-QUERY@" + $sha + "\n")},
    {number: 9, body: "No patch, no commit."},
    {number: 10, body: "Fixed on master: specifications-QUERY@0123abc"},
    {number: 11, body: ("It was Fixed on master: specifications-RM@" + $sha)},
    {number: 12, body: ("Summary.\r\nFixed on master: specifications-RM@" + $sha + "\r\nMore.")},
    {number: 13, body: ("Fixed on master: specifications-ITS-REST@" + $sha)},
    {number: 14, body: null}
  ]' > "$tmp/reports.json"
  # The stub answers `gh auth status` from STUB_AUTH and `gh issue list` with
  # the seeded reports, after checking it was asked for the open reports only.
  cat > "$tmp/bin/gh" <<'STUB'
#!/usr/bin/env bash
if [[ "$1 $2" == "auth status" ]]; then
  [[ "${STUB_AUTH:-yes}" == yes ]]
  exit
fi
if [[ "$1 $2" == "issue list" && "$*" == *"--label upstream-report"* && "$*" == *"--state open"* ]]; then
  cat "$STUB_REPORTS"
  exit 0
fi
echo "stub gh: unexpected call: $*" >&2
exit 1
STUB
  chmod +x "$tmp/bin/gh"
  out="$(STUB_REPORTS="$tmp/reports.json" PATH="$tmp/bin:$PATH" run "$tmp/tree" stub/repo 2>&1 || true)"
  for want in "#9 has no" "#10 has no" "#11 has no" "#14 has no" "8 open upstream report(s) checked"; do
    if [[ "$out" != *"$want"* ]]; then
      echo "self-test: '${want}' was not reported: ${out}" >&2
      exit 1
    fi
  done
  for unwanted in "#7 has" "#8 has" "#12 has" "#13 has"; do
    if [[ "$out" == *"$unwanted"* ]]; then
      echo "self-test: '${unwanted}' was refused, and it has a patch or a fixing commit: ${out}" >&2
      exit 1
    fi
  done
  jq '[.[] | select(.number != 9 and .number != 10 and .number != 11 and .number != 14)]' \
    "$tmp/reports.json" > "$tmp/good.json"
  if ! STUB_REPORTS="$tmp/good.json" PATH="$tmp/bin:$PATH" run "$tmp/tree" stub/repo >/dev/null 2>&1; then
    echo "self-test: reports that each have a patch or a fixing commit were refused" >&2
    exit 1
  fi
  out="$(env -u GH_TOKEN -u GITHUB_TOKEN STUB_AUTH=no STUB_REPORTS="$tmp/reports.json" \
    PATH="$tmp/bin:$PATH" bash "$0" 2>&1)" || {
    echo "self-test: an unauthenticated run did not skip: ${out}" >&2
    exit 1
  }
  if [[ "$out" != "SKIPPED: gh is not authenticated" ]]; then
    echo "self-test: an unauthenticated run did not say SKIPPED: ${out}" >&2
    exit 1
  fi
  out="$(GH_TOKEN=seeded STUB_AUTH=no STUB_REPORTS="$tmp/reports.json" \
    PATH="$tmp/bin:$PATH" run "$tmp/tree" stub/repo 2>&1 || true)"
  if [[ "$out" == *SKIPPED* || "$out" != *"#9 has no"* ]]; then
    echo "self-test: a run with a token skipped instead of asking the tracker: ${out}" >&2
    exit 1
  fi
  jq -n --argjson n "$LIMIT" '[range($n) | {number: (. + 1), body: ""}]' > "$tmp/many.json"
  out="$(STUB_REPORTS="$tmp/many.json" PATH="$tmp/bin:$PATH" run "$tmp/tree" stub/repo 2>&1 || true)"
  if [[ "$out" != *"reach the listing limit"* ]]; then
    echo "self-test: a listing at the limit was not refused: ${out}" >&2
    exit 1
  fi
  echo "upstream-report-patches: self-test OK (a patch or an exact fixing-commit line passes; neither, a short commit, a marker inside a sentence and an empty body are refused; no token and no login reads SKIPPED; a token always asks; a listing at its limit is refused)."
  exit 0
fi

run . "${GITHUB_REPOSITORY:-Vernum-Projecten/Veredictum}"
