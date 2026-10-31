#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Every Arazzo catalogue root in a git tree (#983), for the guards that read
# what a catalogue carries.
#
# A catalogue root is a directory holding `capabilities.yaml` beside a `cases/`
# directory, the shape `veredictum validate --root` and `test --root` read.
# Only files git knows of count, tracked or untracked and not ignored, so build
# output, nested worktrees and vendored trees outside the index never enter.
# No openEHR spec governs this — our own design.
#
# Sourced: `catalogue_roots <tree>` prints each root under the tree, relative
# to it, one per line, in byte order.
# Run:     scripts/checks/catalogue-roots.sh [tree]   # default: the repository
set -euo pipefail

catalogue_roots() { # $1: a git work tree
  local tree="$1" file dir
  git -C "$tree" ls-files --cached --others --exclude-standard -- \
    'capabilities.yaml' '*/capabilities.yaml' |
    LC_ALL=C sort -u |
    while IFS= read -r file; do
      dir="$(dirname "$file")"
      if [[ -d "$tree/$dir/cases" ]]; then
        echo "$dir"
      fi
    done
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  catalogue_roots "${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
fi
