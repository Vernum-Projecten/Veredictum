- Changelog entries are fragments: a pull request with a user-visible change
  adds one file under `changelog.d/`, named `<issue>-<slug>.<section>.md`, and
  no longer edits `CHANGELOG.md`, so open pull requests stop conflicting over
  the Unreleased section. `scripts/release/changelog.sh` checks and previews
  the fragments, and at the release cut `--assemble X.Y.Z YYYY-MM-DD` folds
  them into the new section and deletes them. The pull-request guard requires
  a fragment for a user-visible change and refuses a change that adds a line
  under the Unreleased heading; the `no-changelog` label waives only the
  first. The release `plan` job refuses a tag while a fragment is left.
