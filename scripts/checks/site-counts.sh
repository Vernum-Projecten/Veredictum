#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The website's catalogue counts cannot drift from the catalogue (#35).
#
# The landing page and the book state how many cases, Arazzo documents and
# capability folders the catalogue carries. Those literals are hand-typed, and
# nothing held them to the catalogue: a change that moves a count would
# silently leave the public site wrong. This guard reads the truth from the
# instrument itself — the `validate` summary line — and fails on any
# count-bearing phrase whose number disagrees.
#
# Adjudicated 2026-09-01 (#466): these figures stay hand-written, and this
# guard stays. Every one of them sits inside a sentence a human wrote — a
# landing-page claim, a book paragraph, an argument in `docs/ARCHITECTURE.md` — so
# substituting the number would mean templating the prose around it, which is
# the one thing #466 rules out. The number is derived from the catalogue on
# every run instead, and a disagreement fails the build.
#
# Usage: scripts/checks/site-counts.sh [<validate summary line>]
#   With no argument it runs `cargo run -- validate` itself (the CI test job
#   passes the line it already produced, so the binary runs once).
set -euo pipefail
cd "$(dirname "$0")/../.."

SURFACES=(website/landing/index.html website/book/src README.md docs/ARCHITECTURE.md)

# Every extraction below ends in `|| true`, and that is load-bearing: under
# `set -e` a `$( … | grep )` whose grep matches nothing exits the script on the
# spot, so the diagnostic underneath was unreachable and CI failed with no
# message at all (POSIX shell rule: the pipeline's status is its last command's,
# <https://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html>).
# The empty value is what the loop is written to catch.
summary="${1:-}"
if [[ -z "$summary" ]]; then
  summary="$(cargo run --quiet --locked -- validate --root catalogue --specs specs/openehr | grep -E '[0-9]+ workflow\(s\)' || true)"
fi
cases=$(grep -oE '[0-9]+ workflow\(s\)' <<<"$summary" | grep -oE '^[0-9]+' || true)
documents=$(grep -oE '[0-9]+ document\(s\)' <<<"$summary" | grep -oE '^[0-9]+' || true)
capabilities=$(grep -oE '[0-9]+ capability folder\(s\)' <<<"$summary" | grep -oE '^[0-9]+' || true)
for truth in cases documents capabilities; do
  [[ -n "${!truth}" ]] || { echo "::error::could not derive the ${truth} count" >&2; exit 1; }
done

failures=0
check() { # $1 = expected, $2 = extraction regex, $3 = label
  local expected="$1" regex="$2" label="$3" hit file match n
  while IFS= read -r hit; do
    [[ -n "$hit" ]] || continue
    file="${hit%%:*}"
    match="${hit#*:}"
    n=$(grep -oE '[0-9]+' <<<"$match" | head -1)
    if [[ "$n" != "$expected" ]]; then
      echo "::error::${file} states '${match}', the catalogue carries ${expected} ${label} — update the page (or the phrase joined a pattern it should not match)" >&2
      failures=$((failures + 1))
    fi
  done < <(grep -roE "$regex" "${SURFACES[@]}")
}

check "$cases" '[0-9]+ (spec-cited (test )?cases|test cases|workflow\(s\))' "cases"
check "$documents" '[0-9]+ (Arazzo documents|document\(s\))' "documents"
# The landing page's fact cards carry the number and its label in separate
# spans, so the prose patterns above never see them — proven by mutation.
check "$cases" '"n">[0-9]+</span><span class="l">spec-cited cases' "cases (fact card)"
check "$documents" '"n">[0-9]+</span><span class="l">Arazzo documents' "documents (fact card)"
check "$capabilities" '[0-9]+ capability folder(s|\(s\))' "capability folders"

# The install snippets name the published crate version by hand, in several
# copies; hold every copy to the workspace manifest so a release bump cannot
# leave a page installing the superseded version.
crate_version="$(grep -m1 '^version = ' app/veredictum/Cargo.toml | cut -d'"' -f2)"
while IFS= read -r hit; do
  [[ -n "$hit" ]] || continue
  file="${hit%%:*}"
  n="${hit##*--version }"
  n="${n%% *}"
  if [[ "$n" != "$crate_version" ]]; then
    echo "::error::${file} installs --version ${n}, the workspace is ${crate_version} — update the snippet" >&2
    failures=$((failures + 1))
  fi
done < <(grep -roE -- 'cargo install veredictum[^`<]*--version [0-9a-zA-Z.-]+' "${SURFACES[@]}")

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi
echo "site-counts: every count on the site matches the catalogue (${cases} cases, ${documents} documents, ${capabilities} capability folders) — OK."
