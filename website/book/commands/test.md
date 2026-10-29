`test` drives the Arazzo catalogue under `catalogue/`. `veredictum run` is the
same subcommand under its other name.

**What runs.** Every document under `cases/<capability>/`, in path order. The
shared setups under `cases/_setup/` are not cases: a setup runs only where a
case names it in `dependsOn`. `--document <ID>` narrows the run to named
documents, which is how you re-drive one family while working on a fix; a
narrowed `results.json` is a working tool, never the record you submit.

**What runs last.** A workflow whose rows file says `server: exclusive` changes
state it did not create, such as the bulk delete of every EHR, and runs only
when the IXIT declares `environment.exclusive_server: true`. It runs in a final
phase, after every other workflow of every document, so nothing it deletes is
taken from a workflow still to come. Within each phase the documents keep path
order and the workflows keep document order, and `results.json` lists the rows
in the order they ran. Nothing set up is carried from one workflow to the
next: each workflow runs its own `dependsOn` setups, so no setup result
outlives an exclusive workflow. A re-judgement follows the same order. No
openEHR specification governs this order; it is this instrument's own design.
A workflow that declares no inputs carries the marker in a rows file with no
`rows`, and the workflow then runs once. A conformance-only IXIT declares
`exclusive_server` alone: the other `environment` members describe the
hardware a measured run was taken on, and only `perf`, `stress` and
`aql-probe` read them. `test` reads no `instances` either, since `--base-url`
names the server, so `{ "environment": { "exclusive_server": true } }` is a
whole IXIT for it. The IXIT is checked against the published IXIT schema
before anything runs, and one that fails it, such as a misspelled member,
stops the run with exit status 2 naming the file and the violation.

**What the party declares.** A statement names the capabilities it claims in
`claims.capabilities`, the field the published statement schema defines. For
this catalogue a capability is a folder under `cases/` (`ehr`, `demographic`,
`admin`, …), and a statement claims at least one. Every workflow of a document
in a folder the statement does not claim is skipped as `not-claimed`, citing
the schema's claims clause; no openEHR specification governs the claim
mechanism, which is this instrument's own design. A setup a claimed document
depends on still runs, wherever it lives. The statement's `options` list picks
one arm of each option in `capabilities.yaml`, and the documents of every
other arm are skipped as `option-not-selected`. A name that is not a folder, a
token that names no arm, or two arms of one option stops the run. Without a
statement, every capability runs and no arm is selected.

```json
{
  "claims": { "capabilities": ["ehr", "ehr_status", "composition", "query"] },
  "options": ["ehr-xml-unsupported", "authentication-required"]
}
```

The IXIT answers the `declared` inputs a rows file reads. A workflow with an
administrative step needs the administrative principal: without
`--admin-auth`, it is skipped as `not-declared`. The `version-out-of-range`
and `format-not-claimed` skips are not computed yet (#777, #778).

**Credentials.** Each principal's secrets are read from the environment, from
the variable itself or from a file named by the variable with `_FILE`
appended, which is how container secrets are mounted. Setting both is
refused. A variable set to the empty string counts as unset, because a CI
system expands a secret it does not hold to an empty value, so the run stops
on the missing credential instead of presenting an empty one. Neither credential is recorded: the HAR carries `[not recorded]` in
place of every `Authorization` value, and records only whether the
administrative principal was supplied, so a re-judgement skips the same
workflows.

**Re-judging.** `--rejudge <RECORDING>` answers every request from the
recording and reuses every recorded input, so `results.json` comes out
byte-identical. Pass the root, statement, IXIT and `--document` list the
recorded run was given; a replay that does not line up with the recording is
refused rather than judged.

**In the container image.** The image's entry point hands any argument other
than `healthcheck` to the engine, and `VEREDICTUM_CATALOGUE` points `--root`
at the catalogue the image ships:

```bash
docker run --rm -v "$PWD/out:/work/out" \
    -e VEREDICTUM_SUT_TOKEN \
    ghcr.io/vernum-projecten/veredictum:<tag> \
    test --base-url https://cdr.example/openehr/v1 --auth bearer --out /work/out
```

The image runs as uid 65532, so the mounted directory must be writable by it.

**Other Arazzo tools.** The documents are plain Arazzo 1.1.0, so any Arazzo
tool reads them and a workflow runner can smoke-run them. The verdict is
`test`'s alone: the citations, the judges, the statement-driven selection and
the attribution of every red row live in the `x-veredictum-*` extensions and
in this pipeline, which a generic runner ignores.
