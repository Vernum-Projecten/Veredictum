#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The release pipeline tags an image only after every image smoke passed (#896).
#
# `release.yml` pushes the image BY DIGEST and a later job applies the tags,
# `:latest` among them, and the hosted deploy rides that job. The v0.3.0
# release tagged and deployed a digest whose smoke had failed, because the tag
# job needed the image job and nothing on the smoke. A `needs` edge is the only
# ordering a workflow guarantees, so this check reads the edges: every job that
# calls the reusable tag workflow (`scan-and-tag.yml`) must need, directly or
# through another job, every job whose id names a smoke.
#
# A check that cannot decide must not pass: a workflow with no tag job or no
# smoke job is refused, since the names this check keys on have then moved.
#
# Usage:
#   scripts/checks/release-tags-after-smoke.sh [workflow]   # default: release.yml
#   scripts/checks/release-tags-after-smoke.sh --self-test  # seeded workflows
#
# Exits 0 when every tag job waits for every smoke, 1 naming each missing edge,
# and 2 on a usage error. Needs `yq` and `jq`, which ship on the GitHub Ubuntu
# runner images.
set -euo pipefail

SELF="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
readonly SELF

# closure <json> <job>: every job <job> needs, directly or transitively.
closure() {
  local json="$1" job="$2"
  jq -r --arg start "$job" '
    def needs($j): (.jobs[$j].needs // []) | if type == "string" then [.] else . end;
    . as $doc
    | {seen: [], todo: ($doc | needs($start))}
    | until(.todo | length == 0;
        .todo[0] as $n
        | if (.seen | index($n)) then .todo |= .[1:]
          else .seen += [$n] | .todo = (.todo[1:] + ($doc | needs($n))) end)
    | .seen[]' <<<"$json"
}

# guard <workflow>
guard() {
  local file="$1" json status=0 tag smoke
  [[ -f "$file" ]] || { echo "::error::release-tags-after-smoke: ${file} is missing" >&2; return 1; }
  json="$(yq -o=json '.' "$file")"
  local -a tags=() smokes=()
  while IFS= read -r tag; do [[ -n "$tag" ]] && tags+=("$tag"); done \
    < <(jq -r '.jobs | to_entries[] | select((.value.uses // "") | test("scan-and-tag\\.yml$")) | .key' <<<"$json")
  while IFS= read -r smoke; do [[ -n "$smoke" ]] && smokes+=("$smoke"); done \
    < <(jq -r '.jobs | keys[] | select(test("smoke"))' <<<"$json")
  if [[ ${#tags[@]} -eq 0 ]]; then
    echo "::error::${file} has no job calling scan-and-tag.yml, so this check cannot tell which job applies the tags" >&2
    return 1
  fi
  if [[ ${#smokes[@]} -eq 0 ]]; then
    echo "::error::${file} has no job whose id names a smoke, so this check cannot tell what the tags must wait for" >&2
    return 1
  fi
  for tag in "${tags[@]}"; do
    local needed
    needed="$(closure "$json" "$tag")"
    for smoke in "${smokes[@]}"; do
      if ! grep -qxF -- "$smoke" <<<"$needed"; then
        echo "::error::${file}: job '${tag}' applies the image tags without needing '${smoke}', so a failed smoke still tags the digest and deploys it" >&2
        status=1
      fi
    done
  done
  if [[ "$status" -eq 0 ]]; then
    echo "release-tags-after-smoke: ${#tags[@]} tag job(s) wait for ${#smokes[@]} smoke job(s) in ${file}. OK."
  fi
  return "$status"
}

self_test() {
  local tmp fail=0 n=0
  tmp="$(mktemp -d)"
  # shellcheck disable=SC2064 # expanded now on purpose: the path is fixed for this run
  trap "rm -rf '$tmp'" EXIT

  # case_ <label> <want exit> <jobs YAML>
  case_() {
    local label="$1" want="$2" got=0
    n=$((n + 1))
    printf 'name: Release\non: push\njobs:\n%s\n' "$3" >"$tmp/case$n.yml"
    guard "$tmp/case$n.yml" >"$tmp/out" 2>&1 || got=$?
    if [[ "$got" -ne "$want" ]]; then
      echo "self-test: '$label' exited $got, wanted $want:" >&2
      sed 's/^/  /' "$tmp/out" >&2
      fail=1
    fi
  }

  local image='  image:
    uses: ./.github/workflows/build-image.yml'
  local smoke='  image-smoke:
    needs: [image]
    runs-on: ubuntu-latest
    steps: [{run: "true"}]'

  case_ "the tag job needs the smoke" 0 "${image}
${smoke}
  scan-and-tag:
    needs: [image, image-smoke]
    uses: ./.github/workflows/scan-and-tag.yml"
  case_ "the tag job needs the smoke as a scalar" 0 "${image}
${smoke}
  scan-and-tag:
    needs: image-smoke
    uses: ./.github/workflows/scan-and-tag.yml"
  case_ "the tag job needs the smoke through another job" 0 "${image}
${smoke}
  gate:
    needs: image-smoke
    runs-on: ubuntu-latest
    steps: [{run: \"true\"}]
  scan-and-tag:
    needs: gate
    uses: ./.github/workflows/scan-and-tag.yml"
  case_ "the tag job needs only the image (the v0.3.0 defect)" 1 "${image}
${smoke}
  scan-and-tag:
    needs: image
    uses: ./.github/workflows/scan-and-tag.yml"
  case_ "the tag job needs one smoke leg of two" 1 "${image}
${smoke}
  image-smoke-arm64:
    needs: [image]
    runs-on: ubuntu-latest
    steps: [{run: \"true\"}]
  scan-and-tag:
    needs: [image, image-smoke]
    uses: ./.github/workflows/scan-and-tag.yml"
  case_ "no tag job at all" 1 "${image}
${smoke}"
  case_ "no smoke job at all" 1 "${image}
  scan-and-tag:
    needs: image
    uses: ./.github/workflows/scan-and-tag.yml"
  case_ "a needs cycle terminates" 1 "${image}
${smoke}
  a:
    needs: b
    runs-on: ubuntu-latest
    steps: [{run: \"true\"}]
  b:
    needs: a
    runs-on: ubuntu-latest
    steps: [{run: \"true\"}]
  scan-and-tag:
    needs: a
    uses: ./.github/workflows/scan-and-tag.yml"

  [[ "$fail" -eq 0 ]] || { echo "release-tags-after-smoke: self-test FAILED" >&2; return 1; }
  echo "release-tags-after-smoke: self-test OK ($n seeded workflows judged as expected)."
}

case "${1:-}" in
  --self-test)
    [[ $# -eq 1 ]] || { echo "release-tags-after-smoke: --self-test takes no arguments" >&2; exit 2; }
    self_test
    ;;
  "")
    guard "$(cd "$(dirname "$SELF")/../.." && pwd)/.github/workflows/release.yml"
    ;;
  -*)
    echo "release-tags-after-smoke: unknown option $1" >&2
    exit 2
    ;;
  *)
    guard "$1"
    ;;
esac
