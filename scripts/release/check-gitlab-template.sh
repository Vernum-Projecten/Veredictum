#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The GitLab CI template (`ci/gitlab/veredictum.yml`, #920) runs the release
# image of the version it is tagged with, and reads only the inputs it
# declares.
#
# A vendor includes the template from a release tag, so the image it names is
# what that tag runs. This check reads the template with yq and refuses:
#
#   - an image tag (`VEREDICTUM_IMAGE`) other than `[package] version` of
#     app/veredictum, and, given a version, other than that version too;
#   - a header without `spec: inputs`, or a declared input set other than the
#     one below (the GitHub Action's inputs where they apply, and the
#     template's own), or more than GitLab's 20 inputs;
#   - an input without a description, an `options` list its default is not
#     in, or a boolean whose default is not true or false;
#   - an input whose name reads as a credential: credentials travel as masked
#     CI/CD variables by name, never as inputs;
#   - an `$[[ inputs.<name> ]]` naming an undeclared input, or a declared
#     input no job reads;
#   - a run job without `artifacts: reports: junit`, or a verdict job that
#     does not need the run job.
#
# The `gitlab_template` integration test validates the template, with its
# inputs substituted, against GitLab's own CI schema; this check is what a
# release reads without cargo.
#
# Usage:
#   scripts/release/check-gitlab-template.sh [version]   # the bare version, no leading v
#   scripts/release/check-gitlab-template.sh --pin <version>   # rewrite the image tag (the release cut)
#   scripts/release/check-gitlab-template.sh --self-test
#
# Exit 0 = the template holds, 1 = it does not, 2 = usage.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly TEMPLATE=ci/gitlab/veredictum.yml
readonly ENGINE_MANIFEST=app/veredictum/Cargo.toml
readonly IMAGE=ghcr.io/vernum-projecten/veredictum
readonly INPUTS="admin-auth allow-failure auth base-url documents fail-on-inconclusive ixit job-name keep-recording out-dir platform-url profile stage statement"
readonly MAX_INPUTS=20

engine_version() {
  awk -F'"' '/^\[package\]/{p=1} p && /^version = /{print $2; exit}' "$1"
}

check() { # $1: template, $2: engine manifest, $3: version or empty
  local file="$1" bad=0 engine tag declared used name kind default
  engine="$(engine_version "$2")"
  tag="$(yq -r 'select(document_index == 1) | .[] | select(.variables.VEREDICTUM_IMAGE != null) | .variables.VEREDICTUM_IMAGE' "$file" | head -1)"
  if [[ "$tag" != "${IMAGE}:${engine}" ]]; then
    echo "gitlab-template: $file runs '${tag}', not ${IMAGE}:${engine} (the engine version)" >&2
    bad=1
  fi
  if [[ -n "$3" && "$tag" != "${IMAGE}:$3" ]]; then
    echo "gitlab-template: $file runs '${tag}', not ${IMAGE}:$3 (the version being released)" >&2
    bad=1
  fi

  declared="$(yq -r 'select(document_index == 0) | .spec.inputs // {} | keys | .[]' "$file" | LC_ALL=C sort | tr '\n' ' ')"
  if [[ "${declared% }" != "$INPUTS" ]]; then
    echo "gitlab-template: $file declares inputs '${declared% }', not '$INPUTS'" >&2
    bad=1
  fi
  if (( $(wc -w <<< "$declared") > MAX_INPUTS )); then
    echo "gitlab-template: $file declares more than GitLab's $MAX_INPUTS inputs" >&2
    bad=1
  fi
  for name in $declared; do
    if [[ "$name" =~ (token|password|secret|user|key|credential) ]]; then
      echo "gitlab-template: input '$name' reads as a credential; credentials are CI/CD variables" >&2
      bad=1
    fi
    if [[ -z "$(yq -r "select(document_index == 0) | .spec.inputs.\"$name\".description // \"\"" "$file")" ]]; then
      echo "gitlab-template: input '$name' has no description" >&2
      bad=1
    fi
    kind="$(yq -r "select(document_index == 0) | .spec.inputs.\"$name\".type // \"string\"" "$file")"
    default="$(yq -r "select(document_index == 0) | .spec.inputs.\"$name\" | (if has(\"default\") then .default | tostring else \"<none>\" end)" "$file")"
    if [[ "$kind" == boolean && "$default" != true && "$default" != false ]]; then
      echo "gitlab-template: boolean input '$name' defaults to '$default'" >&2
      bad=1
    fi
    if [[ "$(yq -r "select(document_index == 0) | .spec.inputs.\"$name\" | has(\"options\")" "$file")" == true ]] \
      && ! yq -e "select(document_index == 0) | .spec.inputs.\"$name\" | .options | contains([\"$default\"])" "$file" >/dev/null; then
      echo "gitlab-template: input '$name' defaults to '$default', which its options do not list" >&2
      bad=1
    fi
  done

  used="$(grep -oE '\$\[\[ *inputs\.[A-Za-z0-9_-]+' "$file" | sed -E 's/.*inputs\.//' | LC_ALL=C sort -u | tr '\n' ' ')"
  for name in $used; do
    if [[ " $declared " != *" $name "* ]]; then
      echo "gitlab-template: \$[[ inputs.$name ]] names an undeclared input" >&2
      bad=1
    fi
  done
  for name in $declared; do
    if [[ " $used " != *" $name "* ]]; then
      echo "gitlab-template: input '$name' is declared and never read" >&2
      bad=1
    fi
  done

  if ! yq -e 'select(document_index == 1) | .["$[[ inputs.job-name ]]"].artifacts.reports.junit' "$file" >/dev/null 2>&1; then
    echo "gitlab-template: the run job publishes no artifacts: reports: junit" >&2
    bad=1
  fi
  if [[ "$(yq -r 'select(document_index == 1) | .["$[[ inputs.job-name ]]-verdict"].needs[0].job // ""' "$file")" != '$[[ inputs.job-name ]]' ]]; then
    echo "gitlab-template: the verdict job does not need the run job" >&2
    bad=1
  fi
  return "$bad"
}

pin() { # $1: version
  [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-rc[1-9][0-9]*)?$ ]] || { echo "usage: --pin X.Y.Z[-rcN]" >&2; exit 2; }
  sed -E -i.bak "s|^(    VEREDICTUM_IMAGE: ${IMAGE}:).*$|\\1$1|" "$TEMPLATE"
  rm -f "${TEMPLATE}.bak"
  grep -q "VEREDICTUM_IMAGE: ${IMAGE}:$1\$" "$TEMPLATE" || { echo "gitlab-template: the pin did not apply" >&2; exit 1; }
  echo "gitlab-template: $TEMPLATE runs ${IMAGE}:$1"
}

self_test() {
  local dir version fails=0
  dir="$(mktemp -d)"
  trap 'rm -rf "$dir"' RETURN
  version="$(engine_version "$ENGINE_MANIFEST")"
  printf '[package]\nname = "veredictum"\nversion = "%s"\n' "$version" > "$dir/Cargo.toml"

  # seed <name> <sed expression>: the committed template with one defect.
  seed() {
    sed -E "$2" "$TEMPLATE" > "$dir/$1.yml"
  }
  expect_refused() { # $1: label, $2: file, $3: version
    if check "$dir/$2.yml" "$dir/Cargo.toml" "$3" 2>/dev/null; then
      echo "self-test FAILED: $1 passed" >&2
      fails=$((fails + 1))
    else
      echo "self-test ok: $1 is refused"
    fi
  }
  cp "$TEMPLATE" "$dir/ok.yml"
  if check "$dir/ok.yml" "$dir/Cargo.toml" "$version"; then
    echo "self-test ok: the committed template holds"
  else
    echo "self-test FAILED: the committed template was refused" >&2
    fails=$((fails + 1))
  fi
  expect_refused "a release version the image does not carry" ok "${version}9"
  seed other-tag "s|(VEREDICTUM_IMAGE: ${IMAGE}:).*|\\10.0.0|"
  expect_refused "an image tag other than the engine version" other-tag ""
  seed undeclared 's/\$\[\[ inputs\.profile \]\]/$[[ inputs.profiel ]]/'
  expect_refused "an interpolation naming an undeclared input" undeclared ""
  seed unread 's/"\$\[\[ inputs\.platform-url \]\]"/""/'
  expect_refused "a declared input no job reads" unread ""
  seed credential 's/^    ixit:$/    cdr-token:/'
  expect_refused "an input that reads as a credential" credential ""
  seed bad-default 's/^      default: development$/      default: draft/'
  expect_refused "a default its options do not list" bad-default ""
  seed no-junit '/^    reports:$/,/^      junit:/d'
  expect_refused "a run job without a JUnit report" no-junit ""
  awk '!done && /^      default: false$/ {print "      default: maybe"; done=1; next} {print}' \
    "$TEMPLATE" > "$dir/bad-boolean.yml"
  expect_refused "a boolean input with a non-boolean default" bad-boolean ""
  awk '!/^      description: Path to the party.s IXIT/' "$TEMPLATE" > "$dir/no-description.yml"
  expect_refused "an input without a description" no-description ""

  if [[ "$fails" -gt 0 ]]; then
    echo "check-gitlab-template self-test: ${fails} case(s) failed" >&2
    return 1
  fi
  echo "check-gitlab-template self-test: every case passed"
}

case "${1:-}" in
  --self-test) self_test ;;
  --pin)
    [[ $# -eq 2 ]] || { echo "usage: $0 --pin <version>" >&2; exit 2; }
    pin "$2"
    ;;
  -*) echo "usage: $0 [version] | --pin <version> | --self-test" >&2; exit 2 ;;
  *)
    [[ $# -le 1 ]] || { echo "usage: $0 [version]" >&2; exit 2; }
    if check "$TEMPLATE" "$ENGINE_MANIFEST" "${1:-}"; then
      echo "gitlab-template: $TEMPLATE holds"
    else
      exit 1
    fi
    ;;
esac
