#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Every `report:` number in an `x-veredictum-cite` names an upstream report
# (#632).
#
# A citation that rests on defective released text names the tracker issue the
# defect is reported in (ARCHITECTURE.md §15.4). `validate` is offline and can
# only check that the number is positive, so this guard asks the tracker: each
# number must be an ISSUE in this repository, open or closed, carrying the
# `upstream-report` label. A pull request number, a missing issue or an
# unlabelled one is refused. A number that points nowhere is the decay the
# removed ambiguity register suffered (#617).
#
# The tracker must answer: a failed lookup other than "not found" stops the
# guard rather than passing it.
#
# Usage: scripts/checks/report-refs.sh               # the catalogue
#        scripts/checks/report-refs.sh --self-test   # against a stub tracker
# Env:   REPORT_REFS_ROOT  the tree to scan (default: artifacts/workflows)
#        GITHUB_REPOSITORY the repository to ask (default: Vernum-Projecten/Veredictum)
set -euo pipefail
cd "$(dirname "$0")/../.."

numbers() { # $1: root; prints each distinct report number once, ascending
  if [[ ! -d "$1" ]]; then
    return 0
  fi
  grep -rhoE 'report:[[:space:]]*[0-9]+' "$1" --include='*.arazzo.yaml' |
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

if [[ "${1:-}" == "--self-test" ]]; then
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  mkdir -p "$tmp/bin" "$tmp/good/ehr" "$tmp/bad/ehr"
  # The stub answers exactly as `gh api … --jq` does: 7 is a labelled report,
  # 8 an unlabelled issue, 9 a pull request, anything else not found.
  cat > "$tmp/bin/gh" <<'STUB'
#!/usr/bin/env bash
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
  if ! PATH="$tmp/bin:$PATH" check "$tmp/good" stub/repo >/dev/null 2>&1; then
    echo "self-test: a labelled upstream report was refused" >&2
    exit 1
  fi
  out="$(PATH="$tmp/bin:$PATH" check "$tmp/bad" stub/repo 2>&1 || true)"
  for want in "report: 8 does not carry" "report: 9 is a pull request" "report: 404 names no issue"; do
    if [[ "$out" != *"$want"* ]]; then
      echo "self-test: '${want}' was not reported" >&2
      exit 1
    fi
  done
  if [[ "$(numbers "$tmp/missing")" != "" ]]; then
    echo "self-test: a missing tree yielded report numbers" >&2
    exit 1
  fi
  echo "report-refs: self-test OK (a labelled report passes; unlabelled, pull request and missing are refused)."
  exit 0
fi

check "${REPORT_REFS_ROOT:-artifacts/workflows}" "${GITHUB_REPOSITORY:-Vernum-Projecten/Veredictum}"
