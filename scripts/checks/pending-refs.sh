#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Every `pending:` number in an `x-veredictum-control` names an OPEN issue in
# this repository (#662), the sibling of report-refs.sh, in every Arazzo
# catalogue root of the tree (`catalogue-roots.sh`, #983).
#
# A refusal workflow with no valid twin in the catalogue yet names the issue
# that tracks authoring one, `x-veredictum-control: { pending: <issue> }`, and
# is never judged (docs/ARCHITECTURE.md §15.4). `validate` is offline and can
# only check that the number is positive, so this guard asks the tracker: each
# number must be an open ISSUE here. A pull request number, a missing issue or
# a closed one is refused, because a closed issue means the twin was authored
# and the refusal should name it as its control instead.
#
# The tracker must answer: a failed lookup other than "not found" stops the
# guard rather than passing it.
#
# Usage: scripts/checks/pending-refs.sh               # every catalogue root
#        scripts/checks/pending-refs.sh --self-test   # against a stub tracker
# Env:   PENDING_REFS_TREE the git tree whose catalogue roots are scanned
#                          (default: the repository)
#        GITHUB_REPOSITORY the repository to ask (default: Vernum-Projecten/Veredictum)
set -euo pipefail
cd "$(dirname "$0")/../.."
# shellcheck source=scripts/checks/catalogue-roots.sh
source scripts/checks/catalogue-roots.sh

numbers() { # $1: root; prints each distinct pending number once, ascending
  if [[ ! -d "$1" ]]; then
    return 0
  fi
  grep -rhoE 'x-veredictum-control:[[:space:]]*\{[[:space:]]*pending:[[:space:]]*[0-9]+' "$1" --include='*.arazzo.yaml' |
    sed -E 's/.*pending:[[:space:]]*//' | sort -n -u
}

check() { # $1: root, $2: repository; prints findings, returns 1 on any
  local root="$1" repo="$2" n answer status found=0 seen=0
  while IFS= read -r n; do
    [[ -n "$n" ]] || continue
    seen=$((seen + 1))
    if answer="$(gh api "repos/${repo}/issues/${n}" \
      --jq '"\(.pull_request != null) \(.state)"' 2>&1)"; then
      status=0
    else
      status=$?
    fi
    if [[ "$status" -ne 0 ]]; then
      if [[ "$answer" == *"Not Found"* || "$answer" == *"HTTP 404"* ]]; then
        echo "pending-refs: pending: ${n} names no issue in ${repo}" >&2
        found=1
        continue
      fi
      echo "pending-refs: the tracker did not answer for #${n}: ${answer}" >&2
      exit 2
    fi
    case "$answer" in
      "true "*) echo "pending-refs: pending: ${n} is a pull request, not an issue" >&2; found=1 ;;
      "false closed") echo "pending-refs: pending: ${n} is closed; name the authored twin as the control" >&2; found=1 ;;
      "false open") ;;
      *) echo "pending-refs: unexpected tracker answer for #${n}: ${answer}" >&2; exit 2 ;;
    esac
  done < <(numbers "$root")
  echo "pending-refs: ${seen} pending number(s) checked in ${root}"
  return "$found"
}

scan() { # $1: a git tree, $2: repository; checks every catalogue root in it
  local tree="$1" repo="$2" root roots status=0
  roots="$(catalogue_roots "$tree")"
  if [[ -z "$roots" ]]; then
    echo "pending-refs: no catalogue root (capabilities.yaml beside cases/) in ${tree}" >&2
    return 1
  fi
  while IFS= read -r root; do
    check "$tree/$root" "$repo" || status=1
  done <<< "$roots"
  return "$status"
}

if [[ "${1:-}" == "--self-test" ]]; then
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  mkdir -p "$tmp/bin" "$tmp/good/ehr" "$tmp/bad/ehr"
  # The stub answers exactly as `gh api … --jq` does: 7 is an open issue, 8 a
  # closed one, 9 a pull request, anything else not found.
  cat > "$tmp/bin/gh" <<'STUB'
#!/usr/bin/env bash
case "$2" in
  */issues/7) echo "false open" ;;
  */issues/8) echo "false closed" ;;
  */issues/9) echo "true open" ;;
  *) echo "gh: Not Found (HTTP 404)" >&2; exit 1 ;;
esac
STUB
  chmod +x "$tmp/bin/gh"
  printf '  - workflowId: w\n    x-veredictum-control: { pending: 7 }\n' > "$tmp/good/ehr/a.arazzo.yaml"
  printf '    x-veredictum-control: { pending: 8 }\n' > "$tmp/bad/ehr/a.arazzo.yaml"
  printf '    x-veredictum-control: {pending: 9}\n    x-veredictum-control: { pending: 404 }\n' > "$tmp/bad/ehr/b.arazzo.yaml"
  if ! PATH="$tmp/bin:$PATH" check "$tmp/good" stub/repo >/dev/null 2>&1; then
    echo "self-test: an open issue was refused" >&2
    exit 1
  fi
  out="$(PATH="$tmp/bin:$PATH" check "$tmp/bad" stub/repo 2>&1 || true)"
  for want in "pending: 8 is closed" "pending: 9 is a pull request" "pending: 404 names no issue"; do
    if [[ "$out" != *"$want"* ]]; then
      echo "self-test: '${want}' was not reported" >&2
      exit 1
    fi
  done
  if [[ "$(numbers "$tmp/missing")" != "" ]]; then
    echo "self-test: a missing tree yielded pending numbers" >&2
    exit 1
  fi
  # A second root, three levels down, carries a closed number the first does
  # not; a capabilities.yaml with no cases/ beside it is no root.
  tree="$tmp/tree"
  mkdir -p "$tree/catalogue/cases/ehr" "$tree/fixtures/console/catalogue/cases/ehr" "$tree/notes/ehr"
  git -C "$tree" init -q
  printf 'options: {}\n' > "$tree/catalogue/capabilities.yaml"
  printf '    x-veredictum-control: { pending: 7 }\n' > "$tree/catalogue/cases/ehr/a.arazzo.yaml"
  printf 'options: {}\n' > "$tree/fixtures/console/catalogue/capabilities.yaml"
  printf '    x-veredictum-control: { pending: 8 }\n' > "$tree/fixtures/console/catalogue/cases/ehr/a.arazzo.yaml"
  printf 'options: {}\n' > "$tree/notes/capabilities.yaml"
  printf '    x-veredictum-control: { pending: 9 }\n' > "$tree/notes/ehr/a.arazzo.yaml"
  if [[ "$(catalogue_roots "$tree" | tr '\n' ' ')" != "catalogue fixtures/console/catalogue " ]]; then
    echo "self-test: the catalogue roots are not the two directories holding cases/" >&2
    exit 1
  fi
  out="$(PATH="$tmp/bin:$PATH" scan "$tree" stub/repo 2>&1 || true)"
  if [[ "$out" != *"pending: 8 is closed"* || "$out" == *"pending: 9"* ]]; then
    echo "self-test: the second root's closed number was not refused alone: ${out}" >&2
    exit 1
  fi
  mkdir -p "$tmp/empty"
  git -C "$tmp/empty" init -q
  if PATH="$tmp/bin:$PATH" scan "$tmp/empty" stub/repo >/dev/null 2>&1; then
    echo "self-test: a tree with no catalogue root passed" >&2
    exit 1
  fi
  echo "pending-refs: self-test OK (an open issue passes; closed, pull request and missing are refused, in every catalogue root)."
  exit 0
fi

scan "${PENDING_REFS_TREE:-.}" "${GITHUB_REPOSITORY:-Vernum-Projecten/Veredictum}"
