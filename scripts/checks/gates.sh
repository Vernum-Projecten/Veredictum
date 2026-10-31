#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Runs the documented gate battery, so "run the gates" cannot mean a subset
# somebody remembered (#466).
#
# The battery was prose in four places — the root CLAUDE.md, CONTRIBUTING.md,
# the SessionStart dump and two agent definitions — and nothing ran it. On
# 2026-08-31 five changes reached CI with a rustdoc failure the local run had
# not caught, every one of them because the person or agent ran the cargo
# commands they remembered and not the doc pass. Two of those were the session
# model's own. A list people approximate is not a gate.
#
# The rustdoc pass is the one that matters most here and the one most skipped,
# because it reads as a documentation formality and is not: the workspace doc
# build compiles the console FEATURELESS, so a doc link from an ungated item
# into an `ssr`-gated one cannot resolve, and build, test, clippy and fmt all
# pass regardless. CI runs TWO doc commands and both are below.
#
# Usage:
#   scripts/checks/gates.sh              # the guard tier and the rust tier
#   scripts/checks/gates.sh --all        # everything, including the slow lanes
#   scripts/checks/gates.sh --guards     # the guard tier alone (no cargo)
#   scripts/checks/gates.sh --console    # the console's own two targets
#   scripts/checks/gates.sh --list       # print the battery without running it
#   scripts/checks/gates.sh --self-test  # the tool audit and seeded skips, judged
#
# Every gate is named, run in order, and its outcome printed. The script exits
# non-zero if any gate failed, and reports EVERY failure rather than stopping at
# the first: a run that tells you about one problem when there are three costs
# three round trips.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

declare -a NAMES=() CMDS=() TIERS=() NEEDS=() VERSIONS=()
# gate <tier> <name> <command> [required-tools] [version-command]
#
# The required tools are one word or a space-separated list ("gh yq"), and
# each is checked. A gate naming a tool the machine does not have is SKIPPED
# and says so by name. A silent skip and a pass print the same thing, which is
# the defect class this script exists to close, so the summary lists every skip.
#
# A version command prints which build of the tool ran, before the gate runs.
# It exists for tools CI pins while a local run takes whatever is installed:
# zizmor 1.31 added an audit the 1.29 pin did not have, and the local gate went
# red on a main CI called green with nothing saying the two ran different
# auditors (#832).
gate() { TIERS+=("$1"); NAMES+=("$2"); CMDS+=("$3"); NEEDS+=("${4:-}"); VERSIONS+=("${5:-}"); }

gate guard "comment style"            "bash scripts/checks/comment-style.sh --all"
gate guard "comment style self-test"  "bash scripts/checks/comment-style.sh --self-test"
gate guard "TODO issue refs"          "bash scripts/checks/todo-issue-refs.sh"
gate guard "TODO refs self-test"      "bash scripts/checks/todo-issue-refs.sh --self-test"
gate guard "report numbers"           "bash scripts/checks/report-refs.sh" "gh yq"
gate guard "report numbers self-test" "bash scripts/checks/report-refs.sh --self-test" yq
gate guard "pending numbers"          "bash scripts/checks/pending-refs.sh" gh
gate guard "pending numbers self-test" "bash scripts/checks/pending-refs.sh --self-test"
gate guard "vendored trees"           "bash scripts/checks/vendored-trees.sh"
gate guard "vendored trees self-test" "bash scripts/checks/vendored-trees.sh --self-test"
gate guard "test modules declared"     "bash scripts/checks/test-modules-declared.sh"
gate guard "test modules self-test"   "bash scripts/checks/test-modules-declared.sh --self-test"
gate guard "release tags after smoke" "bash scripts/checks/release-tags-after-smoke.sh" yq
gate guard "tags after smoke self-test" "bash scripts/checks/release-tags-after-smoke.sh --self-test" yq

gate guard "upstream patches apply"   "bash scripts/checks/upstream-patches-apply.sh"
gate guard "upstream patches self-test" "bash scripts/checks/upstream-patches-apply.sh --self-test"
gate guard "JUnit schemas"            "bash scripts/checks/junit-schema.sh" xmllint
gate guard "JUnit schemas self-test"  "bash scripts/checks/junit-schema.sh --self-test" xmllint
gate guard "XML write bodies"         "bash scripts/checks/xml-write-bodies.sh" xmllint
gate guard "XML write bodies self-test" "bash scripts/checks/xml-write-bodies.sh --self-test" xmllint
gate guard "changelog structure"      "bash scripts/checks/changelog-structure.sh"
gate guard "changelog entry"          "bash scripts/checks/changelog-entry.sh"
gate guard "changelog entry self-test" "bash scripts/checks/changelog-entry.sh --self-test"
gate guard "changelog fragments"      "bash scripts/release/changelog.sh --check"
gate guard "changelog preview"        "bash scripts/release/changelog.sh --check-preview"
gate guard "fragments self-test"      "bash scripts/release/changelog.sh --self-test"
gate guard "hosted-instrument words"  "bash scripts/checks/hosted-instrument-language.sh"
gate guard "hosted words self-test"   "bash scripts/checks/hosted-instrument-language.sh --self-test"
gate guard "hosted deploy script"     "bash scripts/checks/hosted-deploy-script.sh"
gate guard "hosted deploy self-test"  "bash scripts/checks/hosted-deploy-script.sh --self-test"
gate guard "issue fields self-test"   "bash scripts/gh/fields.sh --self-test" jq
gate guard "upstream watch self-test" "bash scripts/gh/upstream-watch.sh --self-test" jq
gate guard "action script self-test"  "bash scripts/action/run.sh --self-test"
gate guard "action metadata"          "bash scripts/checks/action-metadata.sh" yq
gate guard "action metadata self-test" "bash scripts/checks/action-metadata.sh --self-test" yq
gate guard "CI jobs gate the merge"   "bash scripts/checks/ci-conclusion-complete.sh"
gate guard "gates cover CI"           "bash scripts/checks/gates-cover-ci.sh"
gate guard "gates self-test"          "bash scripts/checks/gates.sh --self-test"
gate guard "the console's engine pin" "bash scripts/release/check-console-pin.sh"
gate guard "engine pin self-test"     "bash scripts/release/check-console-pin.sh --self-test"
gate guard "GitLab CI template"       "bash scripts/release/check-gitlab-template.sh" yq
gate guard "GitLab template self-test" "bash scripts/release/check-gitlab-template.sh --self-test" yq
gate guard "citation version"         "bash scripts/checks/citation-version.sh"
gate guard "citation version self-test" "bash scripts/checks/citation-version.sh --self-test"
gate guard "zenodo metadata"          "bash scripts/render/zenodo-json.sh --check" jq
gate guard "image labels"             "bash scripts/checks/image-labels.sh"
gate guard "VEX advisories"           "bash scripts/checks/vex-advisories.sh"
gate guard "fuzz seeds"               "bash fuzz/seeds.sh"
# The three workflow and licensing checks CI runs through actions rather than
# through a `run:` line, so `gates-cover-ci.sh` cannot see them. Each names its
# tool and skips by name when it is absent.
gate guard "workflow security"        "zizmor --min-severity=low action.yml .github/workflows/ .github/actions/" zizmor \
  "printf '%s (ci.yml pins %s)' \"\$(zizmor --version)\" \"\$(grep -oE 'zizmor@[0-9.]+' .github/workflows/ci.yml)\""
# SHELLCHECK_OPTS mirrors the workflow's own exclusion, for the reason recorded
# there: every SC2016 here is a `git log --format=` or a `jq` filter, where
# single quotes are the idiom that makes the directive reach the tool unexpanded.
gate guard "workflow correctness"     "SHELLCHECK_OPTS='-e SC2016' actionlint" actionlint
gate guard "REUSE licensing"          "reuse lint" reuse

gate rust  "build"                    "cargo build --workspace --all-targets"
# After `build`, never in the guard tier: the page is compared with the help
# of `target/debug/veredictum`, so it must be this tree's binary.
gate rust  "the command reference"    "bash scripts/checks/cli-surface.sh"
gate rust  "the book's badge gallery" "bash scripts/checks/book-badges.sh"
gate rust  "tests"                    "cargo nextest run --workspace" cargo-nextest
gate rust  "clippy"                   "cargo clippy --workspace --all-targets -- -D warnings"
gate rust  "fmt"                      "cargo fmt --all --check"
# BOTH doc passes, exactly as ci.yml runs them. The second one is why this
# script exists.
gate rust  "rustdoc (workspace)"      "RUSTFLAGS='-D warnings' RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps --document-private-items"
gate rust  "rustdoc (console ssr)"    "RUSTFLAGS='-D warnings' RUSTDOCFLAGS='-D warnings' cargo doc --locked -p veredictum-console --no-deps --document-private-items --features ssr"
gate rust  "validate the Arazzo catalogue" "cargo run --quiet -- validate --root catalogue --specs specs/openehr --profile released"
gate rust  "the development snapshot's absences" "cargo run --quiet -- validate --root catalogue --specs specs/openehr --profile development"
gate rust  "the site's counts"        "bash scripts/checks/site-counts.sh"
gate rust  "the example results"      "bash scripts/checks/example-results-drift.sh"

gate console "clippy (ssr)"           "cargo clippy -p veredictum-console --all-targets --features ssr -- -D warnings"
gate console "clippy (wasm32 hydrate)" "cargo clippy -p veredictum-console --lib --target wasm32-unknown-unknown --no-default-features --features hydrate -- -D warnings"
gate console "tests (ssr)"            "cargo nextest run -p veredictum-console --features ssr" cargo-nextest
gate console "leptosfmt"              "leptosfmt --check app/veredictum-console/src" leptosfmt

gate slow  "cargo deny"               "cargo deny check" cargo-deny
gate slow  "MSRV"                     "cargo hack check --rust-version --all-targets --workspace" cargo-hack
gate slow  "unused dependencies"      "cargo machete" cargo-machete
# The journeys and the fuzz harnesses each need something the other gates do
# not: a container runtime for the digest-pinned browser, and the nightly
# toolchain cargo-fuzz requires. Both are CI jobs of their own, so both are
# here rather than remembered.
gate slow  "console journeys"         "bash scripts/ui-e2e.sh" docker
# The GitHub Action's CI job drives the action through the runner, which no
# local command can; what runs here is the two images that job builds.
gate slow  "action stand-in images"   "bash scripts/action/stand-in-image.sh" docker
gate slow  "SMART lane"               "bash scripts/smart-lane.sh" docker
gate slow  "fuzz harnesses build"     "cargo +nightly fuzz build --target \$(rustc -vV | sed -n 's|^host: ||p')" cargo-fuzz


# gate_tool <command>: the external tool a gate's command runs, or nothing.
#
# Leading NAME=value assignments are skipped. A `bash` script checks for its
# own tools, and the cargo subcommands below ship with every toolchain, so
# both need nothing. Any other cargo subcommand is the binary `cargo-<name>`
# on PATH, which is how cargo finds one
# (<https://doc.rust-lang.org/cargo/reference/external-tools.html#custom-subcommands>),
# and anything else is the command's own first word.
gate_tool() {
  local cmd="$1" assignment="^[A-Z_]+=('[^']*'|\"[^\"]*\"|[^ ]*) +(.*)$" sub
  local -a words
  while [[ "$cmd" =~ $assignment ]]; do cmd="${BASH_REMATCH[2]}"; done
  read -r -a words <<<"$cmd"
  case "${words[0]:-}" in
    "" | bash) return 0 ;;
    cargo)
      sub="${words[1]:-}"
      [[ "$sub" == +* ]] && sub="${words[2]:-}"
      case "$sub" in
        build | check | clippy | doc | fmt | run | test) return 0 ;;
        *) printf 'cargo-%s\n' "$sub" ;;
      esac
      ;;
    *) printf '%s\n' "${words[0]}" ;;
  esac
}

# audit_tools: every gate whose command runs an external tool declares that
# tool, so a machine without it reads SKIPPED and never FAILED (#606).
audit_tools() {
  local i want status=0
  for i in "${!NAMES[@]}"; do
    want="$(gate_tool "${CMDS[$i]}")"
    if [[ -n "$want" && " ${NEEDS[$i]} " != *" ${want} "* ]]; then
      echo "gates: '${NAMES[$i]}' runs ${want} but declares '${NEEDS[$i]}'; name ${want} as its required tool so a machine without it reads SKIPPED" >&2
      status=1
    fi
  done
  return "$status"
}

# run_gates <tiers>: runs every gate of the named tiers and prints the summary.
run_gates() {
  local want="$1" i need log
  local -a failed=() skipped=() needs=() missing=()
  log="$(mktemp)"
  for i in "${!NAMES[@]}"; do
    case " $want " in *" ${TIERS[$i]} "*) ;; *) continue ;; esac
    printf '\033[1m==>\033[0m %s\n' "${NAMES[$i]}"
    read -r -a needs <<<"${NEEDS[$i]}"
    missing=()
    for need in ${needs[@]+"${needs[@]}"}; do
      if ! command -v "$need" >/dev/null 2>&1; then
        printf '    SKIPPED: %s is not installed\n' "$need"
        missing+=("$need")
      fi
    done
    if [[ ${#missing[@]} -gt 0 ]]; then
      skipped+=("${NAMES[$i]} (no ${missing[*]})")
      continue
    fi
    if [[ -n "${VERSIONS[$i]}" ]]; then
      printf '    ran: %s\n' "$(eval "${VERSIONS[$i]}" 2>&1)"
    fi
    if eval "${CMDS[$i]}" >"$log" 2>&1; then
      printf '    ok\n'
    else
      printf '    FAILED\n'
      tail -12 "$log" | sed 's/^/    /'
      failed+=("${NAMES[$i]}")
    fi
  done
  rm -f "$log"

  if [[ ${#skipped[@]} -gt 0 ]]; then
    printf '\n\033[1mgates: %d skipped:\033[0m %s\n' "${#skipped[@]}" "$(printf '%s; ' "${skipped[@]}")"
  fi
  if [[ ${#failed[@]} -gt 0 ]]; then
    printf '\n\033[1mgates: %d failed:\033[0m %s\n' "${#failed[@]}" "$(printf '%s; ' "${failed[@]}")" >&2
    return 1
  fi
  if [[ ${#skipped[@]} -gt 0 ]]; then
    printf '\ngates: every gate RUN in [%s] passed; %d skipped above\n' "$want" "${#skipped[@]}"
    return 0
  fi
  printf '\ngates: every gate in [%s] passed\n' "$want"
}

# indent <text>: the text with every line indented, for a self-test report.
indent() {
  local line
  while IFS= read -r line; do printf '  %s\n' "$line"; done <<<"$1"
}

# self_test: the audit holds this battery, refuses seeded undeclared tools,
# and a missing tool reads SKIPPED while a real failure reads FAILED.
# shellcheck disable=SC2030 # the seeded batteries replace the arrays inside their own subshells on purpose
self_test() {
  local fail=0 out status
  audit_tools || fail=1

  # expect_audit <want exit> <command> [tool]
  expect_audit() {
    local got=0
    ( NAMES=("seeded") CMDS=("$2") NEEDS=("${3:-}"); audit_tools ) 2>/dev/null || got=$?
    if [[ "$got" -ne "$1" ]]; then
      echo "self-test: the audit of '$2' declaring '${3:-}' exited $got, wanted $1" >&2
      fail=1
    fi
  }
  expect_audit 1 "cargo machete"
  expect_audit 0 "cargo machete" cargo-machete
  expect_audit 1 "cargo deny check" deny
  expect_audit 1 "cargo +nightly fuzz build"
  expect_audit 0 "cargo +nightly fuzz build" cargo-fuzz
  expect_audit 0 "cargo build --workspace"
  expect_audit 0 "RUSTDOCFLAGS='-D warnings' cargo doc --workspace"
  expect_audit 1 "SHELLCHECK_OPTS='-e SC2016' actionlint"
  expect_audit 0 "SHELLCHECK_OPTS='-e SC2016' actionlint" actionlint
  expect_audit 0 "bash scripts/checks/junit-schema.sh" xmllint
  expect_audit 0 "cargo machete" "jq cargo-machete"
  expect_audit 1 "cargo machete" "jq cargo-deny"

  status=0
  out="$(
    TIERS=() NAMES=() CMDS=() NEEDS=() VERSIONS=()
    gate seeded "a missing tool" "veredictum-no-such-tool" veredictum-no-such-tool
    gate seeded "a pass" "true"
    gate seeded "a failure" "false"
    run_gates seeded 2>&1
  )" || status=$?
  if [[ "$status" -ne 1 ]] \
    || ! grep -q 'SKIPPED: veredictum-no-such-tool is not installed' <<<"$out" \
    || ! grep -q 'gates: 1 skipped:.*a missing tool (no veredictum-no-such-tool)' <<<"$out" \
    || ! grep -q 'gates: 1 failed:.* a failure;' <<<"$out" \
    || grep -q 'failed:.*a missing tool' <<<"$out"; then
    echo "self-test: a seeded skip, pass and failure were not reported as such (exit $status):" >&2
    indent "$out" >&2
    fail=1
  fi

  status=0
  out="$(
    TIERS=() NAMES=() CMDS=() NEEDS=() VERSIONS=()
    gate seeded "a missing tool" "veredictum-no-such-tool" veredictum-no-such-tool
    gate seeded "a pass" "true"
    run_gates seeded 2>&1
  )" || status=$?
  if [[ "$status" -ne 0 ]] || ! grep -q 'every gate RUN in \[seeded\] passed; 1 skipped' <<<"$out"; then
    echo "self-test: a skip beside a pass did not read as a counted skip (exit $status):" >&2
    indent "$out" >&2
    fail=1
  fi

  status=0
  out="$(
    TIERS=() NAMES=() CMDS=() NEEDS=() VERSIONS=()
    gate seeded "one of two tools" "true" "bash veredictum-no-such-tool"
    gate seeded "both tools" "true" "bash env"
    run_gates seeded 2>&1
  )" || status=$?
  if [[ "$status" -ne 0 ]] \
    || ! grep -q 'SKIPPED: veredictum-no-such-tool is not installed' <<<"$out" \
    || grep -q 'SKIPPED: bash ' <<<"$out" \
    || ! grep -q 'gates: 1 skipped:.*one of two tools (no veredictum-no-such-tool)' <<<"$out"; then
    echo "self-test: a gate missing one of its two tools did not read as a skip naming it (exit $status):" >&2
    indent "$out" >&2
    fail=1
  fi

  [[ "$fail" -eq 0 ]] || { echo "gates: self-test FAILED" >&2; return 1; }
  echo "gates: self-test OK (the battery declares every tool it runs; seeded audits, skip and failure judged as expected)."
}

WANT="guard rust"
# shellcheck disable=SC2031 # the self-test's subshells never touch the battery read here
case "${1:-}" in
  --all)     WANT="guard rust console slow" ;;
  --guards)  WANT="guard" ;;
  --console) WANT="console" ;;
  --list)
    for i in "${!NAMES[@]}"; do printf '%-8s %-24s %s\n' "${TIERS[$i]}" "${NAMES[$i]}" "${CMDS[$i]}"; done
    exit 0 ;;
  # One command per line, for the audit that holds this list to ci.yml.
  --commands)
    for i in "${!NAMES[@]}"; do printf '%s\n' "${CMDS[$i]}"; done
    exit 0 ;;
  --self-test)
    self_test
    exit ;;
  "") ;;
  *) echo "gates: unknown option ${1}; --all, --guards, --console, --list, --self-test" >&2; exit 2 ;;
esac

audit_tools || exit 1
run_gates "$WANT"
