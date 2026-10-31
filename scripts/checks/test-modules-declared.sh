#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Every file of a one-binary integration-test tree is a declared module (#907).
#
# An integration suite here is one binary, `tests/it/main.rs`, with one `mod`
# line per topic file (https://doc.rust-lang.org/cargo/reference/cargo-targets.html
# compiles each top-level `tests/*.rs` as its own crate, so the topics live one
# directory down). Cargo compiles only the modules a crate root declares
# (https://doc.rust-lang.org/reference/items/modules.html), so a topic file
# without its `mod` line is never built, its tests never run, and every gate
# stays green. This check walks every `tests/<dir>/main.rs` under the tree and
# refuses a `.rs` file, or a directory holding one, that its parent module does
# not declare. A commented-out `mod` line is not a declaration.
#
# Usage:
#   scripts/checks/test-modules-declared.sh              # this repository
#   scripts/checks/test-modules-declared.sh --self-test  # seeded trees
#
# Exits 0 when every file is declared, 1 naming each one that is not, and 2 on
# a usage error.
set -euo pipefail

SELF="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
readonly SELF

# declares <module file> <name>: the file carries `mod <name>;`, optionally
# `pub` or `pub(...)`, on a line of its own (an attribute line may precede it).
declares() {
  grep -qE "^[[:space:]]*(pub(\([a-z]+\))?[[:space:]]+)?mod[[:space:]]+$2[[:space:]]*;" "$1"
}

# walk <module file> <module directory>: every child of the module directory
# is declared by the module file, and every child directory is walked in turn.
walk() {
  local file="$1" dir="$2" entry name status=0
  for entry in "$dir"/*; do
    [[ -e "$entry" ]] || continue
    name="$(basename "$entry")"
    if [[ -f "$entry" && "$name" == *.rs ]]; then
      [[ "$entry" == "$file" || "$name" == main.rs || "$name" == mod.rs ]] && continue
      if ! declares "$file" "${name%.rs}"; then
        echo "::error file=${entry}::${entry} is never compiled: ${file} has no \`mod ${name%.rs};\` line" >&2
        status=1
      fi
    elif [[ -d "$entry" ]]; then
      [[ -n "$(find "$entry" -name '*.rs' -print -quit)" ]] || continue
      if [[ -f "$dir/${name}.rs" ]]; then
        walk "$dir/${name}.rs" "$entry" || status=1
      elif [[ -f "$entry/mod.rs" ]]; then
        if declares "$file" "$name"; then
          walk "$entry/mod.rs" "$entry" || status=1
        else
          echo "::error file=${entry}/mod.rs::${entry} is never compiled: ${file} has no \`mod ${name};\` line" >&2
          status=1
        fi
      else
        echo "::error file=${entry}::${entry} holds Rust files no module file declares: neither ${dir}/${name}.rs nor ${entry}/mod.rs exists" >&2
        status=1
      fi
    fi
  done
  return "$status"
}

# guard <root>: every one-binary tree under <root>, excluding build output and
# the agent worktrees under `.claude/`, which are other checkouts.
guard() {
  local root="$1" main status=0 trees=0
  while IFS= read -r main; do
    trees=$((trees + 1))
    walk "$main" "$(dirname "$main")" || status=1
  done < <(find "$root" \( -name target -o -name .git -o -name node_modules -o -path "$root/.claude" \) -prune \
    -o -path '*/tests/*/main.rs' -type f -print | LC_ALL=C sort)
  if [[ "$trees" -eq 0 ]]; then
    echo "::error::test-modules-declared: no tests/<dir>/main.rs under ${root}, so there is nothing to check and the check cannot pass" >&2
    return 1
  fi
  if [[ "$status" -eq 0 ]]; then
    echo "test-modules-declared: every file of ${trees} integration-test tree(s) is a declared module. OK."
  fi
  return "$status"
}

self_test() {
  local tmp fail=0 n=0
  tmp="$(mktemp -d)"
  # shellcheck disable=SC2064 # expanded now on purpose: the path is fixed for this run
  trap "rm -rf '$tmp'" EXIT

  # case_ <label> <want exit> <file:content>...: a crate whose tests/it holds
  # the named files, each path relative to tests/it.
  case_() {
    local label="$1" want="$2" got=0 spec path
    shift 2
    n=$((n + 1))
    mkdir -p "$tmp/case$n/app/x/tests/it"
    for spec in "$@"; do
      path="$tmp/case$n/app/x/tests/it/${spec%%:*}"
      mkdir -p "$(dirname "$path")"
      printf '%b' "${spec#*:}" >"$path"
    done
    guard "$tmp/case$n" >"$tmp/out" 2>&1 || got=$?
    if [[ "$got" -ne "$want" ]]; then
      echo "self-test: '$label' exited $got, wanted $want:" >&2
      sed 's/^/  /' "$tmp/out" >&2
      fail=1
    fi
  }

  case_ "every topic declared" 0 'main.rs:mod alpha;\nmod beta;\n' 'alpha.rs:' 'beta.rs:'
  case_ "a cfg-gated and a pub declaration" 0 \
    'main.rs:#[cfg(feature = "ssr")]\nmod alpha;\npub(crate) mod beta;\n' 'alpha.rs:' 'beta.rs:'
  case_ "a topic file with no mod line (the #907 defect)" 1 'main.rs:mod alpha;\n' 'alpha.rs:' 'beta.rs:'
  case_ "a commented-out mod line" 1 'main.rs:mod alpha;\n// mod beta;\n' 'alpha.rs:' 'beta.rs:'
  case_ "a prefix of the name is not its declaration" 1 'main.rs:mod alpha;\n' 'alpha.rs:' 'alpha_two.rs:'
  case_ "a directory module through mod.rs" 0 'main.rs:mod group;\n' 'group/mod.rs:mod inner;\n' 'group/inner.rs:'
  case_ "a directory module through its sibling file" 0 'main.rs:mod group;\n' 'group.rs:mod inner;\n' 'group/inner.rs:'
  case_ "a nested file its directory module does not declare" 1 \
    'main.rs:mod group;\n' 'group/mod.rs:mod inner;\n' 'group/inner.rs:' 'group/stray.rs:'
  case_ "an undeclared directory module" 1 'main.rs:mod alpha;\n' 'alpha.rs:' 'group/mod.rs:'
  case_ "a directory of Rust files no module file names" 1 'main.rs:mod alpha;\n' 'alpha.rs:' 'loose/inner.rs:'
  case_ "a directory with no Rust file is not a module" 0 'main.rs:mod alpha;\n' 'alpha.rs:' 'fixtures/data.json:{}'

  n=$((n + 1))
  mkdir -p "$tmp/case$n/app/x/src"
  if guard "$tmp/case$n" >"$tmp/out" 2>&1; then
    echo "self-test: a tree with no integration-test binary passed" >&2
    fail=1
  fi

  [[ "$fail" -eq 0 ]] || { echo "test-modules-declared: self-test FAILED" >&2; return 1; }
  echo "test-modules-declared: self-test OK ($n seeded trees judged as expected)."
}

case "${1:-}" in
  --self-test)
    [[ $# -eq 1 ]] || { echo "test-modules-declared: --self-test takes no arguments" >&2; exit 2; }
    self_test
    ;;
  "")
    guard "$(cd "$(dirname "$SELF")/../.." && pwd)"
    ;;
  *)
    echo "test-modules-declared: unknown argument $1" >&2
    exit 2
    ;;
esac
