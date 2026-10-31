# Repository guard scripts

Each script here enforces one rule from `.claude/rules/`. A rule without a
failing check is a wish, so a new rule lands together with its script.

Every script takes the same three modes, so the same code serves the per-edit
hook and the CI job:

```
scripts/checks/<name>.sh --all                 # the whole tree
scripts/checks/<name>.sh --diff <base> [head]  # changed files only
scripts/checks/<name>.sh --files <f>...        # named files (the edit hook)
```

Exit 0 is clean, 1 is violations listed as `file:line: message`, 2 is a usage
error.

| Script | Rule it enforces | Wired into |
|---|---|---|
| `comment-style.sh` | `.claude/rules/comments.md` — block comments, `TODO(#NNNN):` form, marker vocabulary, NOTE and comment-run budgets | `.claude/hooks/rust_fmt_clippy.sh` per edit; the `guards` job in `.github/workflows/ci.yml` |
| `changelog-structure.sh` | Keep a Changelog 1.1.0 — no duplicated `### <Type>` inside one release section, no header outside the canonical type set | the `guards` job in `.github/workflows/ci.yml` |
| `changelog-entry.sh` | a user-visible change adds a fragment under `changelog.d/` (the `no-changelog` label waives it), and no change adds a line under the Unreleased heading of `CHANGELOG.md` | the `guards` job in `.github/workflows/ci.yml` on pull requests, with `--self-test` beside it |
| `ci-conclusion-complete.sh` | branch protection routes through one `conclusion` check, so no CI job may run without appearing in its `needs` | the `workflow-audit` job in `.github/workflows/ci.yml` |
| `citation-version.sh` | `CITATION.cff` `version` and the generated `.zenodo.json` `version` are the workspace engine's `[package]` version, release candidates included | the `guards` job in `.github/workflows/ci.yml`, with `--self-test` beside it |
| `hosted-deploy-script.sh` | the hosted box's `deploy.sh` is shellcheck-clean, extracted from the `write_files` block it lives in | the `guards` job in `.github/workflows/ci.yml`, with `--self-test` beside it |
| `vendored-trees.sh` | every vendored tree (a directory holding a `PROVENANCE.md`, and every directory under `specs/` and `specs/openehr/`) names an existing `scripts/vendor/` script that writes it, and carries the upstream `LICENSE` that script fetches | the `guards` job in `.github/workflows/ci.yml`, with `--self-test` beside it |
| `junit-schema.sh` | every pinned JUnit view holds to the published JUnit XML schemas: the document to the Jenkins xUnit plugin's `junit-10.xsd`, each `<testsuite>` to Apache Maven Surefire's test report schema, both vendored by `scripts/vendor/junit-schemas.sh` and checked with `xmllint` | the `guards` job in `.github/workflows/ci.yml`, with `--self-test` beside it |
| `xml-write-bodies.sh` | every canonical-XML request body of a document an `*-xml-write-accepted` arm names validates against the vendored ITS-XML schemas (`specs/its-xml-schemas/`), chosen by the root's namespace and checked with `xmllint`, and every such document sends one | the `guards` job in `.github/workflows/ci.yml`, with `--self-test` beside it |
| `test-modules-declared.sh` | every `.rs` file of a one-binary integration-test tree (`tests/<dir>/main.rs`) is declared by its parent module, so no topic file is silently never compiled | the `guards` job in `.github/workflows/ci.yml`, with `--self-test` beside it |
| `release-tags-after-smoke.sh` | every release job that applies the image tags needs, directly or transitively, every image smoke job, so a digest that does not run is never tagged or deployed | the `guards` job in `.github/workflows/ci.yml`, with `--self-test` beside it |
| `upstream-patches-apply.sh` | every proposed fix under `upstream/<repository>/` applies with `git apply --check` to the pinned `master` snapshot of that repository under `specs/openehr-development/<repository>/`, and no patch names a repository without a snapshot | the `guards` job in `.github/workflows/ci.yml`, with `--self-test` beside it |

Eight of these take a single file or no argument rather than the three modes
above: `changelog-structure.sh` reads one changelog,
`ci-conclusion-complete.sh` reads one workflow,
`citation-version.sh` reads the three files that carry the version,
`hosted-deploy-script.sh` reads one cloud-init document,
`release-tags-after-smoke.sh` reads one workflow, `test-modules-declared.sh`
walks the integration-test trees, `upstream-patches-apply.sh` reads `upstream/`
and the snapshots, and `vendored-trees.sh` reads the vendored trees and
`scripts/vendor/`. The mode contract
applies to guards that scan a file set.

Guards still to port with the code: the spec-citation resolver, the
default-value style check, the typed-status check, and the SPDX header check.
They arrive with the migration
([FerroEHR#2789](https://github.com/rubentalstra/FerroEHR/issues/2789)), each
with the rule text that justifies it.
