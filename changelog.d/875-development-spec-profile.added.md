- A run picks a specification profile. `development`, the default for `test`,
  the GitHub Action's new `profile` input and the console, is the newest text:
  a pinned snapshot of the `master` of each openEHR specification repository,
  the line that carries the newest text, under `specs/openehr-development/`,
  one directory per repository at its own paths, vendored by `scripts/vendor/spec-docs-development.sh` with a
  `PROVENANCE.md` naming the repository, branch and commit. `released` is
  selectable and frozen: the vendored release under `specs/openehr/`. A case
  whose cited file, section or sentence the snapshot lacks is skipped as
  `cited-text-absent`, with a `detail` naming the profile and what is missing.
- `results.json` records the profile under `specification`, with every
  snapshot commit for a development run, and `schemas/arazzo-results.schema.json`
  describes it. The terminal view, `show`, the Markdown summary, JUnit, CTRF and
  the console's results screen name the profile and the snapshot.
- Released results alone back a released claim or a certificate. Certificate
  issuance refuses a development record with a typed error, so a certificate
  needs a `--profile released` run, and a development result is never compared
  with or counted toward a released result.
- `validate --profile development` reports, beside every gate against the
  release, each case citation the snapshot lacks as information and the counts
  per component. `validate` still defaults to the released profile, where zero
  findings is the only passing result.
- The container image carries `specs/openehr-development/`.
- Under the development profile a quote is matched against the cited text as
  the page renders it: `link:`, `xref:` and labelled `<<id,label>>` macros read
  as their label and a straight double quote as a backtick, and a class table
  the release names by its class is found under its package-qualified name.
  The released profile's matching is unchanged.
