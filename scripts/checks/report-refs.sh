#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Every `report:` number in an `x-veredictum-cite` names an upstream report
# (#632), in the workflow documents and in `capabilities.yaml` alike.
#
# A citation that rests on defective released text names the tracker issue the
# defect is reported in (docs/ARCHITECTURE.md §15.4). `validate` is offline and can
# only check that the number is positive, so this guard asks the tracker: each
# number must be an ISSUE in this repository, open or closed, carrying the
# `upstream-report` label. A pull request number, a missing issue or an
# unlabelled one is refused. A number that points nowhere is the decay the
# removed ambiguity register suffered (#617).
#
# The snapshot `reports.yaml` beside the catalogue's documents records each
# report's title and state for the views; this guard holds every entry to the
# tracker, so a retitled, confirmed or closed report fails it until the
# snapshot is refreshed. `validate` holds the entries to the citations.
#
# The tracker must answer: a failed lookup other than "not found" stops the
# guard rather than passing it.
#
# Usage: scripts/checks/report-refs.sh               # the catalogue
#        scripts/checks/report-refs.sh --self-test   # against a stub tracker
# Env:   REPORT_REFS_ROOT  the tree to scan (default: catalogue)
#        GITHUB_REPOSITORY the repository to ask (default: Vernum-Projecten/Veredictum)
set -euo pipefail
cd "$(dirname "$0")/../.."

numbers() { # $1: root; prints each distinct report number once, ascending
  if [[ ! -d "$1" ]]; then
    return 0
  fi
  grep -rhoE 'report:[[:space:]]*[0-9]+' "$1" --include='*.arazzo.yaml' --include='capabilities.yaml' |
    sed -E 's/report:[[:space:]]*//' | sort -n -u
}

check() { # $1: root, $2: repository; prints findings, returns 1 on any
  local root="$1" repo="$2" n answer status found=0 seen=0
  while IFS= read -r n; do
    [[ -n "$n" ]] || continue
    seen=$((seen + 1))
    if answer="$(gh api "repos/${repo}/issues/${n}" \
      --jq '"\(.pull_request != null) \([.labels[].name] | index("upstream-report") != null)"' 2>&1)"; then
      status=0
    else
      status=$?
    fi
    if [[ "$status" -ne 0 ]]; then
      if [[ "$answer" == *"Not Found"* || "$answer" == *"HTTP 404"* ]]; then
        echo "report-refs: report: ${n} names no issue in ${repo}" >&2
        found=1
        continue
      fi
      echo "report-refs: the tracker did not answer for #${n}: ${answer}" >&2
      exit 2
    fi
    case "$answer" in
      "true "*) echo "report-refs: report: ${n} is a pull request, not an upstream report" >&2; found=1 ;;
      "false false") echo "report-refs: report: ${n} does not carry the upstream-report label" >&2; found=1 ;;
      "false true") ;;
      *) echo "report-refs: unexpected tracker answer for #${n}: ${answer}" >&2; exit 2 ;;
    esac
  done < <(numbers "$root")
  echo "report-refs: ${seen} report number(s) checked in ${root}"
  return "$found"
}

snapshot() { # $1: root, $2: repository; prints findings, returns 1 on any
  local file="$1/reports.yaml" repo="$2" line n title state answer live confirmed rest want found=0
  local entry='^[[:space:]]+([0-9]+):[[:space:]]*\{[[:space:]]*title:[[:space:]]*"(.*)",[[:space:]]*state:[[:space:]]*([a-z]+)[[:space:]]*\}[[:space:]]*$'
  if [[ ! -f "$file" ]]; then
    return 0
  fi
  while IFS= read -r line; do
    if [[ ! "$line" =~ $entry ]]; then
      if [[ "$line" =~ ^[[:space:]]+[0-9]+: ]]; then
        echo "report-refs: cannot read the snapshot entry: ${line}" >&2
        found=1
      fi
      continue
    fi
    n="${BASH_REMATCH[1]}"
    title="${BASH_REMATCH[2]//\\\"/\"}"
    state="${BASH_REMATCH[3]}"
    if ! answer="$(gh api "repos/${repo}/issues/${n}" \
      --jq '"\(.state) \([.labels[].name] | index("upstream-confirmed") != null) \(.title)"' 2>&1)"; then
      echo "report-refs: the tracker did not answer for the snapshot entry #${n}: ${answer}" >&2
      exit 2
    fi
    live="${answer%% *}"
    rest="${answer#* }"
    confirmed="${rest%% *}"
    if [[ "$live" == "closed" ]]; then
      want=closed
    elif [[ "$confirmed" == "true" ]]; then
      want=confirmed
    else
      want=open
    fi
    if [[ "$state" != "$want" ]]; then
      echo "report-refs: reports.yaml records #${n} as ${state}, and the tracker says ${want}" >&2
      found=1
    fi
    if [[ "$title" != "${rest#* }" ]]; then
      echo "report-refs: reports.yaml records #${n} as \"${title}\", and the tracker titles it \"${rest#* }\"" >&2
      found=1
    fi
  done < "$file"
  return "$found"
}

if [[ "${1:-}" == "--self-test" ]]; then
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  mkdir -p "$tmp/bin" "$tmp/good/ehr" "$tmp/bad/ehr"
  # The stub answers exactly as `gh api … --jq` does: 7 is a labelled report,
  # 8 an unlabelled issue, 9 a pull request, anything else not found.
  cat > "$tmp/bin/gh" <<'STUB'
#!/usr/bin/env bash
if [[ "$4" == *upstream-confirmed* ]]; then
  case "$2" in
    */issues/7) echo "open true A report that is seven" ;;
    *) echo "gh: Not Found (HTTP 404)" >&2; exit 1 ;;
  esac
  exit 0
fi
case "$2" in
  */issues/7) echo "false true" ;;
  */issues/8) echo "false false" ;;
  */issues/9) echo "true false" ;;
  *) echo "gh: Not Found (HTTP 404)" >&2; exit 1 ;;
esac
STUB
  chmod +x "$tmp/bin/gh"
  printf 'x-veredictum-cite:\n  report: 7\n' > "$tmp/good/ehr/a.arazzo.yaml"
  printf 'x-veredictum-cite: { source: s, report: 8 }\n' > "$tmp/bad/ehr/a.arazzo.yaml"
  printf 'x-veredictum-cite:\n  report: 9\n---\nreport: 404\n' > "$tmp/bad/ehr/b.arazzo.yaml"
  printf 'options:\n  o:\n    x-veredictum-cite: { source: s, report: 405 }\n' > "$tmp/bad/capabilities.yaml"
  if ! PATH="$tmp/bin:$PATH" check "$tmp/good" stub/repo >/dev/null 2>&1; then
    echo "self-test: a labelled upstream report was refused" >&2
    exit 1
  fi
  out="$(PATH="$tmp/bin:$PATH" check "$tmp/bad" stub/repo 2>&1 || true)"
  for want in "report: 8 does not carry" "report: 9 is a pull request" "report: 404 names no issue" "report: 405 names no issue"; do
    if [[ "$out" != *"$want"* ]]; then
      echo "self-test: '${want}' was not reported" >&2
      exit 1
    fi
  done
  printf 'reports:\n  7: { title: "A report that is seven", state: confirmed }\n' > "$tmp/good/reports.yaml"
  printf 'reports:\n  7: { title: "Seven", state: open }\n  8: {title: broken}\n' > "$tmp/bad/reports.yaml"
  if ! PATH="$tmp/bin:$PATH" snapshot "$tmp/good" stub/repo >/dev/null 2>&1; then
    echo "self-test: a snapshot entry matching the tracker was refused" >&2
    exit 1
  fi
  out="$(PATH="$tmp/bin:$PATH" snapshot "$tmp/bad" stub/repo 2>&1 || true)"
  for want in "#7 as open, and the tracker says confirmed" "titles it \"A report that is seven\"" "cannot read the snapshot entry"; do
    if [[ "$out" != *"$want"* ]]; then
      echo "self-test: '${want}' was not reported" >&2
      exit 1
    fi
  done
  if [[ "$(numbers "$tmp/missing")" != "" ]]; then
    echo "self-test: a missing tree yielded report numbers" >&2
    exit 1
  fi
  echo "report-refs: self-test OK (a labelled report passes; unlabelled, pull request and missing are refused, in documents and capabilities; a snapshot entry is held to the tracker's title and state)."
  exit 0
fi

status=0
check "${REPORT_REFS_ROOT:-catalogue}" "${GITHUB_REPOSITORY:-Vernum-Projecten/Veredictum}" || status=1
snapshot "${REPORT_REFS_ROOT:-catalogue}" "${GITHUB_REPOSITORY:-Vernum-Projecten/Veredictum}" || status=1
exit "$status"
