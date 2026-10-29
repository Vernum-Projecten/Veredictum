<!-- SPDX-FileCopyrightText: Veredictum contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Changelog fragments

A pull request with a user-visible change records its changelog entry as one
file in this directory. It does not edit `CHANGELOG.md`. Each pull request
writes its own file, so two open pull requests never conflict over the same
lines, and the release cut moves every fragment into the new version's section.

User-visible has the meaning `CHANGELOG.md` gives it: the CLI surface, the
published artifact schemas, verdict semantics, the container image, or anything
a party's published conformance record depends on. The paths that count are
listed in `scripts/checks/changelog-entry.sh`.

## Writing a fragment

The file name is `<issue>-<slug>.<section>.md`:

- **`<issue>`:** the tracker issue the pull request closes, a positive integer
  without leading zeros.
- **`<slug>`:** a few lower-case words joined by hyphens, so two fragments for
  one issue can sit side by side.
- **`<section>`:** the Keep a Changelog 1.1.0 section, in lower case: `added`,
  `changed`, `deprecated`, `removed`, `fixed` or `security`.

For example, `766-changelog-fragments.changed.md`.

The file holds the entry as it will read in `CHANGELOG.md`: one or more list
items that start with `- `, with any continuation line indented by two spaces.
It has no heading, no blank line, and no SPDX header (`REUSE.toml` covers this
directory, since the text moves into `CHANGELOG.md` at the cut).

```markdown
- `veredictum validate` names the catalogue root it read in every finding,
  so a run over two roots says which one a finding came from.
```

You do not need to write the issue number. At the cut, each entry gains
` (#<issue>)` at the end of its first paragraph, unless the entry already
names that issue.

## What the checks enforce

`scripts/release/changelog.sh --check` refuses a fragment with a malformed
name, an unknown section, a blank line, or a line that is neither a list item
nor an indented continuation. `--preview` prints `CHANGELOG.md` with every
fragment folded into the Unreleased section, as the next cut would write it,
and `--check-preview` runs `scripts/checks/changelog-structure.sh` over that
preview. CI runs all three, and `scripts/checks/gates.sh` runs them locally.

`scripts/checks/changelog-entry.sh` is the pull-request guard. It asks for a new
fragment here when a change touches a user-visible path, and the `no-changelog`
label waives that requirement. It also refuses any change that adds a line under
the Unreleased heading of `CHANGELOG.md`, and no label waives that refusal: a
new entry is always a fragment.

## The release cut

The cut pull request runs:

```bash
scripts/release/changelog.sh --assemble X.Y.Z YYYY-MM-DD
```

This renames the Unreleased heading to `## [X.Y.Z] - YYYY-MM-DD` and adds a new
empty Unreleased heading above it. The entries already in that section stay
first, and the fragments follow them, section by section in Keep a Changelog
order and by issue number within a section. It updates the `[unreleased]` link
reference and adds one for `X.Y.Z`, then deletes the fragments. The cut's own
lines land under the new version heading, so the guard accepts it. The release
`plan` job refuses a release tag while any fragment is still here.

## A release candidate

A release may be preceded by release candidates, `X.Y.Z-rcN` with N starting
at 1. No other pre-release suffix is accepted: the script refuses `beta1`,
`alpha.1`, `rc0` and every other one. The candidate's cut pull request runs:

```bash
scripts/release/changelog.sh --assemble X.Y.Z-rcN YYYY-MM-DD
```

This writes the `## [X.Y.Z-rcN] - YYYY-MM-DD` section exactly as the release
cut would write `X.Y.Z`, and it keeps the fragments and any entry under the
Unreleased heading where they are. Each candidate's section and the final
release section therefore carry every change since the last release, and the
candidate's link reference compares from that release. Pull requests merged
after a candidate keep adding fragments here, and the next candidate or the
final cut folds them in.

At a candidate tag, the `plan` job runs
`scripts/release/changelog.sh --covered X.Y.Z-rcN`, which fails unless that
section carries every fragment still here and every entry under the Unreleased
heading. A fragment merged after the candidate's cut and before its tag
therefore fails the tag, and since a tag is never re-pointed, the recovery is
the next candidate.

This README is the only file here that is not a fragment.
