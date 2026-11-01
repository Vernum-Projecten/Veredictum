#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Every `report:` number in an `x-veredictum-cite` names an upstream report
# (#632), in the workflow documents and in `capabilities.yaml` alike, in every
# Arazzo catalogue root of the tree (`catalogue-roots.sh`, #983).
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
# Usage: scripts/checks/report-refs.sh               # every catalogue root
#        scripts/checks/report-refs.sh --self-test   # against a stub tracker
# Env:   REPORT_REFS_TREE  the git tree whose catalogue roots are scanned
#                          (default: the repository)
#        GITHUB_REPOSITORY the repository to ask (default: Vernum-Projecten/Veredictum)
set -euo pipefail
cd "$(dirname "$0")/../.."
# shellcheck source=scripts/checks/catalogue-roots.sh
source scripts/checks/catalogue-roots.sh

if ! command -v yq >/dev/null 2>&1; then
  echo "report-refs: yq is not installed" >&2
  exit 2
fi

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
  local file="$1/reports.yaml" repo="$2" entries n title state answer live confirmed rest want found=0
  if [[ ! -f "$file" ]]; then
    return 0
  fi
  # yq reads the flow and the block form alike; one line per entry, tab-split.
  if ! entries="$(yq e '.reports // {} | to_entries[] | (.key | tostring) + "\t" + (.value.state // "" | tostring) + "\t" + (.value.title // "" | tostring)' "$file" 2>&1)"; then
    echo "report-refs: cannot read the snapshot ${file}: ${entries}" >&2
    return 1
  fi
  while IFS=$'\t' read -r n state title; do
    [[ -n "$n$state$title" ]] || continue
    if [[ ! "$n" =~ ^[0-9]+$ || ! "$state" =~ ^[a-z]+$ || -z "$title" ]]; then
      echo "report-refs: cannot read the snapshot entry: ${n}" >&2
      found=1
      continue
    fi
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
  done <<< "$entries"
  return "$found"
}

scan() { # $1: a git tree, $2: repository; checks every catalogue root in it
  local tree="$1" repo="$2" root roots status=0
  roots="$(catalogue_roots "$tree")"
  if [[ -z "$roots" ]]; then
    echo "report-refs: no catalogue root (capabilities.yaml beside cases/) in ${tree}" >&2
    return 1
  fi
  while IFS= read -r root; do
    check "$tree/$root" "$repo" || status=1
    snapshot "$tree/$root" "$repo" || status=1
  done <<< "$roots"
  return "$status"
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
  printf 'reports:\n  7:\n    title: A report that\n      is seven\n    state: confirmed\n' > "$tmp/good/reports.yaml"
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
  # A second root, three levels down, carries a stale number the first does
  # not; a capabilities.yaml with no cases/ beside it is no root.
  tree="$tmp/tree"
  mkdir -p "$tree/catalogue/cases/ehr" "$tree/fixtures/console/catalogue/cases/ehr" "$tree/notes"
  git -C "$tree" init -q
  printf 'options: {}\n' > "$tree/catalogue/capabilities.yaml"
  printf 'x-veredictum-cite:\n  report: 7\n' > "$tree/catalogue/cases/ehr/a.arazzo.yaml"
  printf 'options: {}\n' > "$tree/fixtures/console/catalogue/capabilities.yaml"
  printf 'x-veredictum-cite:\n  report: 406\n' > "$tree/fixtures/console/catalogue/cases/ehr/a.arazzo.yaml"
  printf 'x-veredictum-cite:\n  report: 407\n' > "$tree/notes/capabilities.yaml"
  if [[ "$(catalogue_roots "$tree" | tr '\n' ' ')" != "catalogue fixtures/console/catalogue " ]]; then
    echo "self-test: the catalogue roots are not the two directories holding cases/" >&2
    exit 1
  fi
  out="$(PATH="$tmp/bin:$PATH" scan "$tree" stub/repo 2>&1 || true)"
  if [[ "$out" != *"report: 406 names no issue"* || "$out" == *"report: 407"* ]]; then
    echo "self-test: the second root's stale number was not refused alone: ${out}" >&2
    exit 1
  fi
  mkdir -p "$tmp/empty"
  git -C "$tmp/empty" init -q
  if PATH="$tmp/bin:$PATH" scan "$tmp/empty" stub/repo >/dev/null 2>&1; then
    echo "self-test: a tree with no catalogue root passed" >&2
    exit 1
  fi
  echo "report-refs: self-test OK (a labelled report passes; unlabelled, pull request and missing are refused, in documents and capabilities and in every catalogue root; a snapshot entry is held to the tracker's title and state)."
  exit 0
fi

scan "${REPORT_REFS_TREE:-.}" "${GITHUB_REPOSITORY:-Vernum-Projecten/Veredictum}"
