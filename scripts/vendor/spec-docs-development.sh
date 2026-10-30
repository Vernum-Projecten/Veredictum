#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Vendors a pinned snapshot of the openEHR specification DEVELOPMENT line
# into specs/openehr-development/: the text the `development` specification
# profile of `veredictum test` and `veredictum validate` reads, and the text a
# proposed upstream patch under upstream/<openEHR repository>/ is written
# against. It is never the oracle of a released claim; that is specs/openehr/,
# which this script never touches.
#
# Which repositories: every component the catalogue's citations name, found
# from the citations themselves (`grep -rhoE 'specs/openehr/[A-Z-]+/'
# catalogue`, plus specs/rest-oas/, which is ITS-REST's computable/OAS/), and
# every repository upstream/ holds a patch for. A repository either list
# gains is added to the table below.
#
# Layout: one directory per upstream repository, named as the repository is,
# keeping the repository's own relative paths unchanged, so a patch written
# against the repository applies inside its directory:
#
#   specs/openehr-development/specifications-RM/docs/...
#   specs/openehr-development/specifications-ITS-REST/specifications/...
#   specs/openehr-development/specifications-ITS-REST/computable/OAS/...
#
# Every repository is pinned BY COMMIT. The branch is recorded for provenance
# only and is never read at run time: `development` where the repository
# keeps one, and `master` where it keeps no other line (ITS-JSON, ITS-XML).
# Every text format a citation or a patch can name is taken (INCLUDE_EXT);
# figures and other binaries are not. Each upstream LICENSE rides along
# verbatim.
#
# Idempotent: wipes and re-vendors each repository directory. Re-run after
# moving a pin below; the script writes every PROVENANCE.md.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST="$REPO_ROOT/specs/openehr-development"
INCLUDE_EXT=(adoc md txt csv json yaml yml g4 xsd xml)

# component | upstream repo | branch | pinned commit
# Read with `git ls-remote https://github.com/openEHR/<repo>.git
# refs/heads/<branch>` on 2026-10-30.
COMPONENTS=(
  "AM|specifications-AM|development|6fc148cb902b524746f35088d72077bd508e2705"
  "BASE|specifications-BASE|development|3c3bf5539dd0aed283ed80d505164376f65bf940"
  "ITS-JSON|specifications-ITS-JSON|master|14387ce9191f228d768b6bade3308b79aaf87471"
  "ITS-REST|specifications-ITS-REST|development|9afd2dac8d52a080961e405c6d78b1ddba1eb336"
  "ITS-XML|specifications-ITS-XML|master|e6ff2a4e55c13cc79e15376a46c29f4c1e529f92"
  "QUERY|specifications-QUERY|development|3df25fbc38f9b778f0fe9e86c54c3fa64063520f"
  "RM|specifications-RM|development|3f631d205919fee1674b9f7892d12dd0d23c2e21"
)

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# Upstream tooling and dependency manifests are not spec text (the same
# exclusions as scripts/vendor/spec-docs.sh, for the same reasons).
MANIFESTS=(
  'requirements*.txt' 'Pipfile*' 'pyproject.toml' 'setup.py' 'setup.cfg'
  'composer.json' 'composer.lock'
  'package.json' 'package-lock.json' 'yarn.lock' 'pnpm-lock.yaml'
  'Gemfile*' '*.gemspec'
  'pom.xml' 'build.gradle*' 'go.mod' 'go.sum'
)
rsync_args=(--exclude='.git' --exclude='.claude' --exclude='.junie' --exclude='.github' --exclude='AGENTS.md' --exclude='tmp-*')
for manifest in "${MANIFESTS[@]}"; do
  rsync_args+=(--exclude="$manifest")
done
for ext in "${INCLUDE_EXT[@]}"; do
  rsync_args+=(--include="*.$ext")
done
rsync_args+=(--include='*/' --exclude='*')

license_of() { # $1: LICENSE file; prints its name or fails
  if grep -q "Attribution-ShareAlike 3.0" "$1"; then
    echo "CC-BY-SA 3.0 Unported"
  elif grep -q "Apache License" "$1"; then
    echo "Apache-2.0"
  else
    return 1
  fi
}

mkdir -p "$DEST"
for entry in "${COMPONENTS[@]}"; do
  IFS='|' read -r name repo branch sha <<<"$entry"
  echo "==> $name ($repo @ $branch, $sha)"
  src="$TMP/$repo"
  git init -q "$src"
  git -C "$src" remote add origin "https://github.com/openEHR/$repo.git"
  git -C "$src" fetch -q --depth 1 origin "$sha"
  git -C "$src" checkout -q FETCH_HEAD
  if [[ "$(git -C "$src" rev-parse HEAD)" != "$sha" ]]; then
    echo "ERROR: the fetch of $repo did not resolve to $sha" >&2
    exit 1
  fi
  if [[ ! -f "$src/LICENSE" ]]; then
    echo "ERROR: $repo has no LICENSE at $sha; adjudicate before vendoring" >&2
    exit 1
  fi
  if ! license="$(license_of "$src/LICENSE")"; then
    echo "ERROR: the $repo LICENSE at $sha is neither CC-BY-SA 3.0 nor Apache-2.0; adjudicate before vendoring" >&2
    exit 1
  fi

  out="$DEST/$repo"
  rm -rf "$out"
  mkdir -p "$out"
  rsync -a --prune-empty-dirs "${rsync_args[@]}" "$src/" "$out/"
  cp "$src/LICENSE" "$out/LICENSE"
  cat >"$out/PROVENANCE.md" <<EOF
# Vendored openEHR development snapshot: $name

- Source: https://github.com/openEHR/$repo
- Branch: $branch
- Commit: \`$sha\`
- License: $license. The upstream \`LICENSE\` is vendored verbatim alongside
  this file, from the same pinned commit.
- Vendored by: \`scripts/vendor/spec-docs-development.sh\`, at the
  repository's own relative paths (text formats only: ${INCLUDE_EXT[*]}).
  Figures and other binaries are excluded.

A snapshot of the openEHR development line of this repository: the text a run
under the \`development\` specification profile reads, and the text a patch
under \`upstream/$repo/\` is written against. It is no part of the released
oracle under \`specs/openehr/\`, and nothing judged against it enters a
released result. Do not hand-edit files under this directory; re-run the
script instead.
EOF
  echo "    $(find "$out" -type f | wc -l | tr -d ' ') files"
done

echo "Done. Vendored into $DEST"
