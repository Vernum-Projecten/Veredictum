# CLAUDE.md

Veredictum is the independent conformance instrument for openEHR clinical data
repositories: a machine-readable catalogue of spec-cited test cases, executed
against any running CDR over its own wire, judged by pure functions over the
recorded exchanges. One tool covers functional conformance and measured
performance: a declared clinical workload driven open-loop to the knee,
compared A/A and A/B on one host. The
released openEHR specifications are the
only authority it accepts, and every expectation in the catalogue cites the
section it comes from. The instrument is independent on purpose: a CDR must
never be able to grade its own homework, and that property only holds if the
workflow here cannot leak, shortcut, or bend an expectation. **This file is
that workflow.** The tracker is GitHub Issues in this repository; the record is
the closed issues, the PR descriptions, `CHANGELOG.md`, and git history. There
is no design-doc layer: decisions live in this file, in `.claude/rules/*.md`,
and in the code.

## Layout

This repository holds the whole instrument: the runner, the catalogue, the
performance schedule and the vendored spec oracle. It commits no
conformance declaration about any product: ISO/IEC 9646-7 assigns an ICS
proforma's support columns to the supplier of the implementation, so this
repository authors the proforma (the capability folders under
`catalogue/cases/` and the options of `catalogue/capabilities.yaml`) and
a declaration travels with the submission it describes (#465).

A change to instrument behaviour lands here, and every release ships from here. The crate is on crates.io, and a `v*` tag publishes signed binaries
and the container image through the pipeline the Releasing section describes.

| Path | Contents |
|---|---|
| `app/veredictum/src/**` | the runner machinery: the Arazzo reader, gates, evaluator and HAR recording, the performance instrument (workloads, the open-loop generator, the run, the `perf-result` record, `perf-judge` and `perf-compare`) |
| `app/veredictum/src/bin/veredictum.rs` | the instrument binary; every instrument is a subcommand of it |
| `app/veredictum/tests/**` | the integration suite: catalogue gates, seeded-defect rejection, schema drift, the verification pack, the perf driver |
| `app/veredictum-console/**` | the web console: the Leptos frontend the container image serves (#6, design #52); its rules are `.claude/rules/leptos-ui.md` |
| `catalogue/**` | the conformance catalogue: Arazzo workflows per capability folder under `cases/`, `capabilities.yaml` options, rows files, the test signing material under `keys/`, `id-map.yaml` (docs/ARCHITECTURE.md §15) |
| `perf/**` | the performance material: the workload documents under `workloads/`, the payloads, and the host prescriptions under `hosts/` (docs/ARCHITECTURE.md §8.14) |
| `fixtures/**` | test material: the named ICS fixture, a declaration and its ixit |
| `schemas/**` | the published JSON Schemas for every artifact family (emitted, drift-guarded) |
| `verification-pack/**` | the recorded HAR exchanges proving the evaluator re-judges them into their adjudicated results |
| `specs/openehr/**` | the vendored released openEHR spec text: the oracle |
| `specs/its-xml-schemas/**`, `specs/its-json-schemas/**`, `specs/rest-oas/**` | the machine-readable bundles; each is a root a citation may name beside `specs/openehr/`, and an ITS-JSON schema grounds an expectation only as the second ordered supplement (hard rule 1) |
| `upstream/<openEHR repository>/**` | proposed fixes to a released specification, one patch per `upstream-report` issue, written against the pinned `master` snapshot under `specs/openehr-development/`, checked by `scripts/checks/upstream-patches-apply.sh`, and never applied to `specs/` |
| `deploy/docker/**` | the container image: its `Dockerfile`, the quickstart `docker-compose.yml`, the browser-journey SUT pairs, and the hadolint configuration |
| `deploy/hosted/**` | the hosted instrument: the posture the image carries and the box's cloud-init |
| `docs/**` | the design and governance record: `ARCHITECTURE.md`, `ASSURANCE_CASE.md`, `GOVERNANCE.md`, `MAINTAINERS.md`, `AI_STATEMENT.md` |
| `scripts/vendor/**` | one script per vendored tree — the only way any of them is refreshed |
| `scripts/checks/**` | the repository guard scripts |
| `.claude/**` | rules, hooks, agents, memory |

The oracle is `specs/openehr/`, and it is in this repository. Read the governing
section there first-hand. Never answer a spec question from memory, from a
vendor's documentation, or from what a server did.

## The instrument's hard rules

### 1. The released openEHR specifications are the only authority

The vendored spec text is the oracle, and **it is never a suspect.** Before
authoring or changing any expectation, read the governing section first-hand
and quote the sentence that assigns the value.

- **Oracle set:** the RELEASED components. RM, BASE, AM, QUERY (AQL), TERM,
  ITS-XML, and the ITS-REST **docs text**, which binds every operation to the
  wire. The SM was never released and is no part of the oracle (#604).
- **Two ordered supplements:** the released OpenAPI documents are part of the
  release's own specification artifacts, so they ground an expectation **where
  the docs text is silent**. They lose to the docs text on any conflict, and an
  OAS-grounded expectation is always cited as the OAS, by file and element.
  The ITS-JSON schemas (`specs/its-json-schemas/`, DEVELOPMENT in their
  manifest) are the second, by owner ruling on #842: they ground an expectation
  only where every released component is silent on the canonical JSON form,
  lose to any released text, and are always cited as ITS-JSON by file and JSON
  Pointer. Review-enforced, like the OAS order: no gate can judge silence.
- **Never authority:** the CNF Platform Conformance Test Schedule (openEHR CNF
  never released a stable version, so it says which behaviours to cover, not
  what the correct answer is), the upstream Robot suites and their data sets
  (stalled), any CDR's behaviour, and any prior harness. Where one of these
  conflicts with a released component, the released component wins.
- **Maturity inside the release** (owner ruling 2026-10-30, #872): each
  document's status is read from the vendored manifests, never declared. A
  document marked DEVELOPMENT inside a release is released text and gates
  as BASE. A case resting on TRIAL text keeps its verdict and is read with
  the OPTIONS, outside every BASE tally. A RETIRED document is no part of
  the release, and `validate` refuses a citation into it.
- **Two specification profiles** (owner rulings 2026-10-30, #875). A run
  picks one. `development`, the default run profile, is the latest text: a
  pinned snapshot of the `master` of each openEHR repository, the line
  that carries the newest text, under `specs/openehr-development/`, vendored by
  `scripts/vendor/spec-docs-development.sh` with a `PROVENANCE.md` naming
  the commit, and the text every patch under `upstream/` is written against.
  `released` is selectable and frozen: `specs/openehr/`, where nothing is
  ever patched. The released profile is the oracle for released claims, and
  the development profile for development runs. A development run computes
  real verdicts against the development text, and its record, every view and
  every export name the profile and the snapshot. Released results alone back
  a released claim or a certificate: a development result is never mixed
  with, compared as, or counted toward a released result, a certificate or
  the released BASE/OPTIONS tally. A case whose cited section differs between
  the profiles cites the text of the profile it runs under: the citation's
  `development` member quotes the snapshot's wording, once a first-hand
  reading confirms the requirement is unchanged in substance. A case whose
  cited text is absent from a profile is skipped with that reason.
  `validate` gates the catalogue against the release, with zero findings the
  only passing result.
- Where both the docs text and the OAS are silent, no case asserts the
  behaviour: the silence is an `upstream-report` issue here, with its patch
  under `upstream/`, and a citation that rests on defective text names that
  issue in `report:`. Behaviour the release leaves to the implementation is an
  option in `catalogue/capabilities.yaml`, which a statement answers. A private
  resolution is never acceptable, and neither is a bent expectation.

### 2. The three-way attribution law (the core discipline)

When a run goes red, the failure is attributed **before anything is changed**,
to exactly one of three suspects, by comparing spec-required against
catalogue-expected against SUT-observed. The full law is
`.claude/rules/cnf-triage.md`; the delegation target is the `cnf-triage` agent.

| Suspect | What it means | Fix path |
|---|---|---|
| **The SUT** | the server under test violates the spec | the attribution recorded where it was found, with the reproduced exchange and the spec citation. Never a change here, and never a report filed outside this repository (`.claude/rules/cnf-triage.md`) |
| **The runner machinery** | the server behaved correctly and `app/veredictum/src/**` misdrove the case or misjudged the response | fix the runner module. Those rows were inconclusive, never SUT failures |
| **The catalogue** | the hand-authored expectation is wrong against the spec | edit the artifact, with a new spec-cited source for the corrected expectation |

Two reflexes are banned, and the second one is the failure mode this product
exists to prevent:

1. **"The catalogue must be wrong, our SUT is right."** This is the reflex a
   vendor brings. The instrument answers it by construction: an expectation
   traces to a spec citation, so it is refuted by a better reading of the spec
   and by nothing else.
2. **"The SUT must be wrong, the instrument is right."** Veredictum is the
   thing being trusted, so it is a suspect on every red row, first-class,
   before the server. An instrument that presumes itself correct is worth
   nothing to the people who are supposed to rely on its verdicts. The first
   live triage in FerroEHR attributed 7 of 7 diagnosed defects to the runner,
   zero to the server under test.

Never read a SUT response to decide what an expectation should be. The response
is evidence in the comparison. The spec is the reference.

### 3. Coverage is a mandate, and it ratchets up only

A green run over a thin catalogue proves nothing. The catalogue must exercise
every behaviour the spec defines: every operation, every status-code branch,
every required or conditional header, every negotiation variant, every
precondition and error family, every RM and AQL behaviour. Each gets its own
small isolated case, so a red row names one defect.

- A spec-defined behaviour with no case is a gap to close, or an honest
  boundary recorded as an upstream report or a capability option. Silence is
  never coverage.
- Cases are added, never removed to go green. Narrowing coverage needs an
  adjudicated, spec-cited reason.
- **Every refusal the spec requires is an asserted negative test.** When a
  server is spec-right to reject something, the invalid shape is preserved as
  its own corpus entry plus a refusal case, so a lenient server fails it. A
  fixture fix that deletes the invalid shape silently narrows the claim.

### 4. Never weaken, skip, or delete a test

Not to make a build pass, and not to route around a bug a test exposes. If a
test fails and the fix is unclear, leave it failing and record a
`// TODO(#NNNN):` naming its issue. Details in `.claude/rules/testing.md`.

### 5. Comments follow RFC 505 and RFC 1574, with hard budgets

Line comments only; block comments are banned. Pending work is
`// TODO(#NNNN): <what is missing>`, always with its tracker issue. `// NOTE:`
is a settled decision as a citation plus one sentence, at most 3 lines.
A plain `//` run is at most 8 lines. Adjudication essays and change narration
belong on the PR or the issue. No other marker vocabulary exists. Full guide:
`.claude/rules/comments.md`, enforced by `scripts/checks/comment-style.sh`
through the edit hook and by the `guards` job in CI, over every hand-written
`.rs` file in the tree.

### 6. Reliability rules pair with a failing check

No `unsafe`, ever. No `unwrap`/`expect`/`panic!` outside tests. Errors are
typed at every boundary that branches. Determinism where output is compared.
The generic register, with the lint or check that enforces each rule, is
`.claude/rules/reliability.md`. A rule with no failing check is a wish, so when
you add one, add its enforcement in the same change.

### 7. Branches use the conventional types

`<type>/<kebab-case-slug>` with `type` one of `feat`, `fix`, `chore`, `docs`,
`refactor`, `perf`, `test`, `ci`, `build`, `release`. Pick the type by the
dominant change. Never force-push `main`. When one issue's fix builds on
another's unmerged branch, GitHub's native stacked pull requests are the
candidate mechanism (cascading rebase, bottom-up merges, per-layer CI);
`.claude/rules/stacked-prs.md` is the full reference and the adoption gate —
one issue per PR and every gate stay unchanged either way.

### 8. Never add AI or Claude attribution to a commit or a PR

This is absolute and has no exceptions. No `Co-Authored-By` trailer of any
kind, no "Generated with Claude Code", no robot emoji, no similar line in a
commit message, body, trailer, PR title, PR description, PR comment, issue, or
code comment. Commit and PR text describe the change and nothing else. Do not
pass a flag or template that injects attribution. The `no_attribution_guard.sh`
hook blocks the command before it runs.

### 9. Every commit is verified

Local commits are GPG-signed (`commit.gpgsign=true`). Never disable it and
never commit with a stripped identity.

**Learned 2026-08-26: the GitHub contents API creates UNSIGNED commits.** A
commit written through `PUT /repos/{owner}/{repo}/contents/{path}` lands
unverified, so it is not an acceptable way to write to this repository. Two
paths are: a local signed commit, or a workflow that builds the commit through
the Git Data API (blob, then tree, then commit, then ref update), which GitHub
signs as `github-actions[bot]`. Any workflow that writes a commit uses the Git
Data API pattern, never `git commit` and push, and never the contents API.

### 10. Keep the changelog

`CHANGELOG.md` follows Keep a Changelog 1.1.0. Every PR with user-visible
changes adds a fragment under `changelog.d/` in the same PR, and never edits
`CHANGELOG.md`: `changelog.d/<issue>-<kebab-slug>.<section>.md`, holding the
entry as it will read (`changelog.d/README.md` has the format). Each PR writes
its own file, so open PRs stop conflicting over the Unreleased section (#766).
User-visible here means the CLI surface, the catalogue's published schemas,
verdict semantics, the container image, or anything a party's published record
depends on. Releases are cut from the changelog:
`scripts/release/changelog.sh --assemble X.Y.Z YYYY-MM-DD` renames the
Unreleased heading to the version with its date, folds the fragments in,
re-adds an empty Unreleased heading, updates the link references and deletes
the fragments; then tag. A release candidate's cut,
`--assemble X.Y.Z-rcN YYYY-MM-DD`, writes the same section and keeps the
fragments, so the final release section carries every change too. The script
rewrites the HEADING, never the first textual match — the v0.0.1-alpha.1 cut
rewrote a mention inside a paragraph and left the release with no section. Milestones are releases, and a release
is cut when its milestone reaches zero open issues.
`scripts/checks/changelog-structure.sh` and `changelog.sh --check`,
`--check-preview` run in CI on every change, and
`scripts/checks/changelog-entry.sh` (#10, #766) REQUIRES a fragment when a
user-visible surface changes, waived only by the `no-changelog` label, and
REFUSES any line added under the Unreleased heading, which no label waives. The
full cut procedure is the Releasing section below, and the pipeline reads this
file: a missing or empty section for the tagged version, or a fragment left in
`changelog.d/`, fails `plan` before anything is published.

### 11. Cite only durable references

In code, schema, and doc comments, justify behaviour by citing the vendored
openEHR spec text (file plus section) or official external documentation: the
Rust book and reference, the docs.rs page of a pinned crate, the PostgreSQL
docs where a SUT's storage is relevant. Never cite an internal markdown file,
because internal documents move or die. Where the specs are silent, write the
explicit flag "no openEHR spec governs this — our own design", never a pointer
to a plan file.

### 12. Prose has no AI tells

Every piece of text a human reads as prose is held to
`.claude/rules/writing-style.md`: no "not X, but Y" framing, no decorative
triads, no buzzword vocabulary, an em dash budget, no vague transitions. This
covers the README, the docs site, issue and PR bodies, release notes, and
announcement drafts. It does not loosen any technical rule.

## Issue workflow

The tracker is GitHub Issues in this repository. The open issue list is the
worklist, and issue state is edited only through `gh`. Never track work in chat
alone.

1. Orient with `gh issue list --state open`. Read the contract with
   `gh issue view <n> --comments`. Skip an issue whose blockers are still open.
2. **The issue body is the contract.** It opens with a plain summary, no
   heading: what, why, the owner rulings, the spec citations. Then an
   `## Acceptance criteria` checklist, and an optional `## Tasks` task list.
3. Do the work. Read the governing spec sections first for anything
   spec-facing. Build compiling, tested increments.
4. Record progress on the issue: tick verified criteria with
   `gh issue edit <n>`, post decisions as comments with `gh issue comment <n>`.
5. Commit on a conventional-type branch with a descriptive subject. The PR body
   declares `Closes #<n>` so the merge closes the issue. One `Closes` keyword
   per issue: `Closes #1, #2` closes only #1.
6. New work found en route gets its own issue, linked as a sub-issue of the
   issue it decomposes or as a real `blocked-by` dependency. A prose "see also"
   is not a link, and an edge lives in its native panel, never in the body.

**Relationships are native metadata, set through one command.** `gh` has no
subcommand for sub-issues or dependencies, and the write endpoints take an
issue's database id rather than its number, so every edge is set with
`scripts/gh/rel.sh` (`parent`, `unparent`, `blocked-by`, `unblock`, `blocking`,
`unblocking`, `tree`, `id`) and never with a hand-typed `gh api`. Sub-issues
express decomposition, `blocked-by` expresses real in-repository sequencing, and
neither is ever restated in an issue body — the panel is canonical and a body
copy rots on the first change. Full policy: `.claude/rules/issue-relationships.md`.

**Type, priority and effort are issue fields, never labels** (owner ruling
2026-10-23, #588). The Vernum-Projecten organisation defines them: the native
issue type (`Bug`, `Feature`, `Task`), the `Priority` issue field (`Urgent`,
`High`, `Medium`, `Low`) and the `Effort` issue field (`High`, `Medium`,
`Low`). `scripts/gh/fields.sh` is the one sanctioned write path (`type`,
`priority`, `effort`, `show`), and `scripts/gh/fields.sh new <type> <priority>
<effort> <gh issue create args…>` files an issue that carries all three from
the start. Every open issue has a type and a priority. Priority decides pickup
order, oldest first within a priority; Effort is an estimate and never reorders
anything.

**Labels.** A `Bug` or a `Feature` carries no work-kind label. A `Task` carries
exactly one, mapped to the conventional-commit types: `documentation`, `chore`,
`refactor`, `perf`, `ci` or `test`. The component
an issue adjudicates against carries a `spec:` label (`spec:RM`, `spec:BASE`,
`spec:AM`, `spec:QUERY`, `spec:TERM`, `spec:ITS`, `spec:CNF`).
`question` routes a question, `on-hold` marks work parked by owner decision, and
`no-changelog` is the escape hatch the changelog guard reads — a guard label that
does not exist fails silently at apply time, so the label exists before the guard
that reads it. An outbound
report of a defect, contradiction, or silence in a released openEHR
specification is an issue labeled `upstream-report`, and that issue **is** the
report: a plain summary, then `## What the released spec says` with citations,
`## What this implementation does`, `## Resolution sought upstream`,
`## What the patch changes`, `## Points for the SEC to decide`, and it never
exists without its patch under `upstream/<openEHR repository>/`, written
against the pinned `master` snapshot, unless `master` already fixes the defect
and the report records the fixing commit. A citation
that rests on the defect names it in `report:`. A report is created unverified, gains
`upstream-confirmed` once re-verified first-hand, and closes as the standing
outbound record once its divergence is adjudicated here. A refuted one closes
as refuted, every `report:` naming it is removed, and the affected case becomes
gating.

**Milestones are releases.** A milestone is a delivery promise, so an issue
waiting on something upstream carries `blocked-upstream` and no milestone. A
release is cut when its milestone reaches zero open issues; the next milestone
always exists, so triage always has a target. **Veredictum stays on 0.x until
the openEHR SEC approves it, and 1.0.0 marks that approval** (owner ruling
2026-10-23); 0.x already means not yet stable, so no release carries a
pre-release suffix. **A release may be preceded by release candidates,
`X.Y.Z-rcN` with N from 1** (owner ruling 2026-10-29), so the image, the GitHub
Action and the pipeline run for real before the release; the first is
`0.2.0-rc1`. No other suffix exists: no alpha, no beta. A candidate belongs to
its release's milestone and is cut whenever the owner wants one, before that
milestone reaches zero. `v0.2.0` is the rewrite onto Arazzo (#590); `v0.2.1` holds
what comes up from it and must ship right after; `v0.3.0` holds the follow-ups
outside the rewrite.

**Skills.** `/phase-status` orients, `/next-task` turns an issue into a plan,
`/phase-done` closes one. Each is trimmed to the machinery that exists here and
names what it deliberately does not check, because a verification step pointing
at absent machinery reports green.

## Merging

`main` merges through a **merge queue** (the `main` ruleset, 2026-10-23). A
pull request enters it with `gh pr merge <n> --auto` or "Merge when
ready" (no `--squash`: the queue sets the method, and `gh` refuses one);
GitHub rebases it onto the current `main` with the entries ahead of
it, runs CI on that `merge_group`, and squash-merges it when the required
`conclusion` check passes. Nothing merges with a red `conclusion` through the
queue, and no pull request has to be updated by hand after another merges.

## Build and test

Every gate below runs on every pull request, every push to `main` and every
merge-queue entry, behind one required `conclusion` check
(`.github/workflows/ci.yml`; `scripts/checks/ci-conclusion-complete.sh` refuses
a job that runs without gating the merge).

**One script runs the whole battery**, and it is the battery:

```bash
scripts/checks/gates.sh            # the guard tier and the rust tier
scripts/checks/gates.sh --all      # everything, the console and the slow lanes
scripts/checks/gates.sh --guards   # the guard tier alone (no cargo)
scripts/checks/gates.sh --console  # the console's own two targets
scripts/checks/gates.sh --list     # what it runs, without running it
```

That script exists because the list used to be prose in four places and nothing
ran it. On 2026-08-31 five changes reached CI with a rustdoc failure the local
run had not caught, every one because whoever ran "the gates" ran the cargo
commands they remembered. Two were the session model's own. Run the script; do
not reconstruct the list, and do not copy it here — `--list` prints it, and
`scripts/checks/gates-cover-ci.sh` fails the build when `ci.yml` runs a command
the script does not. The first version of the script was itself a subset: it
ran 7 of the 22 script invocations in `ci.yml`, and two of the four failures on
a pull request that same day were commands in the gap.

A gate whose tool is missing locally prints `SKIPPED: <tool> is not installed`
and is counted in the summary. A skip that looks like a pass is the failure
this whole mechanism exists to prevent.

Three things it encodes that are easy to get wrong on your own:

- **`--workspace` is not decoration.** Without it a root-run command reads
  `default-members`, which is `app/veredictum` alone, so a green board says
  nothing whatever about the console (#405).
- **`--workspace` is still not the console's gate.** It compiles the console
  with no features, which is not how it ships. The console's two real targets
  (`ssr` natively, `hydrate` on wasm32) are their own tier.
- **CI runs TWO rustdoc passes**, and the second one — the console with
  `--features ssr` — is the one nobody remembers. The workspace pass compiles the
  console featureless, so a doc link from an ungated item into an `ssr`-gated one
  cannot resolve there, and build, test, clippy and fmt all pass regardless.


`/ui-gates` adds `leptosfmt` and the browser journeys.

The libFuzzer harnesses in `fuzz/` are their own nightly workspace, never a
root member. CI compiles them on every pull request touching `app/` or `fuzz/`
and campaigns weekly; a crash is fixed in the reader and pinned by a test, never
worked around in the harness (`.claude/rules/fuzzing.md`).

```bash
fuzz/seeds.sh <target>
cargo +nightly fuzz run <target> fuzz/corpus/<target> fuzz/seeds/<target> \
  -- -max_total_time=120 -timeout=25
```

**`validate` is the gate the catalogue lives or dies by**, and it is clean
before any SUT is composed:

```bash
cargo run -- validate --root catalogue --specs specs/openehr
```

It is every machine check over the catalogue (docs/ARCHITECTURE.md §15.9): the
Arazzo document shape, citation resolution against the vendored specs, the
condition profile, coverage of the released operations, per-capability
floors — and **zero findings is the only passing result.** `--statement <file>` adds the static conformance
review of one SUPPLIED declaration, which is the only way a claim enters:
nothing is swept from the tree. `veredictum --help` is the authority on how to
invoke everything else, and `veredictum <subcommand> --help` on one subcommand;
never improvise a flag. No document here restates that surface:
`website/book/src/commands.md` is GENERATED from the same help output by
`scripts/render/commands-md.sh`, with the hand-written notes per command living
beside it in `website/book/commands/`, and
`scripts/checks/cli-surface.sh` refuses a committed page the generator does not
reproduce.

## Releasing

A release is a `v*` tag, and `.github/workflows/release.yml` is the only thing
that publishes anything. The tag is the trigger; every ordering inside the
pipeline is a `needs` edge, so no leg guesses whether an earlier one finished.

**Cut the release in one pull request**, on a `release/vX.Y.Z` branch:

1. `CHANGELOG.md`: run
   `scripts/release/changelog.sh --assemble X.Y.Z YYYY-MM-DD` and commit what
   it writes. It renames the Unreleased heading to `## [X.Y.Z] - YYYY-MM-DD`,
   folds every `changelog.d/` fragment in after the entries already there,
   re-adds an empty Unreleased heading above it, updates the `[unreleased]`
   link reference and adds the `[X.Y.Z]` one, and deletes the fragments. It
   rewrites the HEADING, never the first textual match — the v0.0.1-alpha.1 cut
   rewrote a mention inside the intro paragraph and shipped a release with no
   section at all. `--preview` shows the result first without writing anything.
2. `Cargo.toml` `[package] version`, and `Cargo.lock` with it (`cargo check`).
   A content PR MAY pre-bump the version when its change is what the next
   release exists to ship (the way #36 moved alpha.3 → alpha.4 for the lib
   API); the cut then verifies the version rather than moving it. Whichever
   PR moves it, the version in the tree at tag time is what publishes, and the
   console's `veredictum = "=X"` moves in the same commit. The console's
   `ENGINE_PIN` follows on its own: `app/veredictum-console/build.rs` reads
   that dependency line and substitutes the value. The committed example
   perf-result embeds this version, so whichever PR moves the version also
   runs `cargo run --example make_example_perf_result` and commits the
   result — the PR-level path filter does not catch the drift, and the push
   run to main does (#521). The fuzz workspace's `fuzz/Cargo.lock` records
   the engine version through its path dependency and moves in the same
   commit: `cargo update -p veredictum --manifest-path fuzz/Cargo.toml`
   (add `--offline` when the local registry cache already holds the fuzz
   dependencies). `scripts/release/check-console-pin.sh` refuses a fuzz lock
   on any other version, on every pull request and at tag time (#611).
3. `CITATION.cff`: `version` and `date-released`.
4. `bash scripts/render/zenodo-json.sh` — `.zenodo.json` is GENERATED, because
   Zenodo ignores `CITATION.cff` completely whenever a `.zenodo.json` exists
   (<https://help.zenodo.org/docs/github/describe-software/zenodo-json/>). The
   deposit's FILES freeze at publication, so a drifted copy is archived under a
   DOI nobody can correct.
5. Put it in the merge queue (`gh pr merge <n> --auto`), then
   **verify the merge actually landed** —
   `gh pr view <n> --json state --jq .state` must read `MERGED`. `gh pr merge`
   can print advice and exit zero without merging, and a tag on an unmerged
   branch publishes the wrong tree.
6. `git tag -s vX.Y.Z -m "…"` on the verified merge commit, and push it. The
   `release tags` ruleset requires a signature and refuses deletion, so a tag is
   never re-pointed: the recovery for a bad cut is the next version.
7. Once `finalize` has published the GitHub release, the owner edits it and
   ticks "Publish this Action to the GitHub Marketplace" (primary category
   Testing, second Continuous integration). The pipeline cannot do this: GitHub
   has no API for it, and the form needs the owning account's two-factor
   authentication. The `announce` job reads the public listing and, when it
   does not show the tag yet, writes the release form's direct edit link into
   the run summary. The form reads `action.yml` from the tagged commit, so a
   description over the limit cannot be fixed on that tag (#802, #829);
   `scripts/checks/action-metadata.sh` holds it on every pull request.

**A release candidate is cut the same way**, on a `release/vX.Y.Z-rcN` branch,
with `X.Y.Z-rcN` wherever the steps above say `X.Y.Z`: in the manifests and the
console pin, the compose tag, `CITATION.cff`, and the tag `vX.Y.Z-rcN`. N starts
at 1, and no other pre-release suffix is accepted anywhere: `changelog.sh`
refuses one and `plan` refuses its tag. Step 1 differs in one way.
`scripts/release/changelog.sh --assemble X.Y.Z-rcN YYYY-MM-DD` writes the
`## [X.Y.Z-rcN] - YYYY-MM-DD` section exactly as the release cut would and
KEEPS the fragments and any entry under the Unreleased heading, so each
candidate's section and the final `X.Y.Z` section carry every change since the
last release, and its link reference compares from that release. A merge after
a candidate adds its fragment as usual, and the next candidate or the final
cut folds it in. At a candidate tag, `plan` runs `changelog.sh --covered
X.Y.Z-rcN` in place of the no-fragment check. A candidate moves no release
pointer: no `:latest`, no hosted deploy, and the GitHub release is marked as a
pre-release. It still publishes its crate, its image tag and its binaries.

**The console's engine pin is ONE value: the workspace engine version** (#179).
The console names the engine by an exact crates.io version in its own manifest,
which is what keeps a console release reproducible from published artifacts,
and the workspace root's `[patch.crates-io]` redirects that name to
`app/veredictum` for every build made inside this repository. So the pin may
name the version being cut before crates.io carries it, every pull request in
the window still compiles the console on both targets, and the engine every
gate spawns is the engine the console links.
`scripts/release/check-console-pin.sh` holds the manifest pin,
`app/veredictum`'s version, the operator compose tag and the tag being
published to that one value, and it also refuses a
`Cargo.lock` carrying a registry-sourced `veredictum` — cargo drops an
unmatchable patch and resolves the registry copy with no warning at all, which
is the silent fallback the check exists to catch. It runs on every pull request
(`ci.yml`, guards) and again at tag time inside `plan`.

Two orderings lost. **Publishing the crate first**, ahead of `image-binary`,
puts the immutable crates.io upload and its reviewer pause on the critical path
before the image is built or scanned, and it fixes nothing for pull requests:
the cut PR that pre-bumps the pin names a version crates.io does not carry yet,
so that PR cannot go green. **Pre-bump plus a patch dropped in the publishing
lane** needs the crate live before the image is built, so it inherits the same
irreversibility. The cost of the patch, stated plainly: an in-repository build
no longer proves the console's dependency line resolves against the registry.
What it does prove is that the console links the engine this tree publishes,
which is the property the drift window was destroying.

**What the tag then does**, in order: `plan` re-verifies every fact above
against the tagged commit and publishes nothing; the GitHub release is created
as a DRAFT with the changelog section as its notes and the bare version as its
title; the per-architecture binaries build in the reusable SLSA L3 lane and
attach their tarball, checksum, CycloneDX SBOM and Sigstore bundles; `finalize`
refuses to publish until every expected asset is attached, then flips the draft
— last, because publication freezes the asset set. In parallel the image binary
compiles natively per architecture, the multi-architecture image pushes BY
DIGEST, a smoke run drives that digest, Trivy scans both variants, and only then
do the tags apply, `:latest` among them for a non-pre-release. `publish-crate`
runs last of all and pauses for the `crates-io` environment's reviewer, so that
approval blocks nothing else. `announce` asserts every leg landed and prints the
exact recovery per leg.

**Two facts that are easy to get wrong.** A pre-release is a `v*` tag with a `-`
suffix and moves no release pointer; the rule has one definition,
`.github/actions/release-kind`, and the only such suffix is `-rcN`. And crates.io Trusted Publishing matches the
OIDC `workflow_ref` claim, which names the CALLING workflow for a job inside a
reusable workflow — so the publisher configuration must name `release.yml`
itself, beside the entry for the out-of-band `publish-crates.yml` lane, and the
publish logic is shared as `scripts/release/publish-crate.sh` rather than as a
reusable workflow.

## Model orchestration

The session model orchestrates and does the judgement-heavy work itself: the
attribution calls, the spec adjudications, the catalogue design. It fans
bounded, file-heavy implementation out to subagents through the `Agent` tool,
at most two implementation workers at a time. Delegate by the nature of the
work, never reflexively, and hand every spec-facing worker the exact spec paths
it must read. The defined agents are in `.claude/agents/`:

- `cnf-triage` — read-only attribution of red rows. Every red run goes here
  before any code is touched.
- `spec-researcher` — answers a spec question from the vendored text with exact
  citations, keeping heavy `.adoc` reading out of the main context.
- `implementer` — bounded implementation on a tight spec.

Subagents obey every rule in this file. Tell them not to spawn further
subagents.

## References

- `docs/ARCHITECTURE.md` — **the architecture record**, and the design authority for
  the machinery: the party documents and the verdict computation (§8), the
  Arazzo catalogue with its extensions, condition profile and gates (§15), and
  §8.14's
  population-anchored performance-class model with its journey decomposition.
  Read it before changing how the instrument judges or measures anything. It
  moved here from the FerroEHR mono-repo at the split and is still being worked
  on: adapt names and paths when the tree moves, never reword the content
- `.claude/rules/cnf-triage.md` — the attribution law and the upstream-report lifecycle
- `.claude/rules/testing.md` — test discipline and the coverage mandate
- `.claude/rules/fuzzing.md` — the harness contract, the crash law, and the
  crash-to-regression-test procedure
- `.claude/rules/comments.md` — RFC 505 / RFC 1574 with budgets
- `.claude/rules/reliability.md` — the safety rules and their enforcement
- `.claude/rules/writing-style.md` — prose style
- `.claude/rules/ai-code-review.md` — machine review is a second opinion, never
  authority; the SonarQube Cloud setup facts
- `.claude/rules/issue-relationships.md` — the four native issue edges, the one
  sanctioned write path, and the no-duplication law
- `.claude/rules/stacked-prs.md` — GitHub stacked pull requests: the `gh stack`
  command surface, how the semantics meet this repository's rules, and the
  adoption gate
- `README.md` — the product identity and the origin of the name
- [FerroEHR#2789](https://github.com/rubentalstra/FerroEHR/issues/2789) — where
  the instrument was built before it moved to this repository
