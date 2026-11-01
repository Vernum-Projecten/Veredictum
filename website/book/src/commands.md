# Command reference

<!-- toc -->

Every code block below is `veredictum <command> --help` from the binary itself:
this page is rendered by `scripts/render/commands-md.sh` out of clap's own
output, so a flag cannot land in the binary and go missing here. The sections
follow the order `veredictum --help` lists them in, and the prose under each
one is written by hand and lives in `website/book/commands/`.

Two commands make the conformance record: `validate` checks the catalogue,
and `test` drives it or re-judges its recording. `export`
renders a results document as JUnit XML, CTRF and a Markdown summary for a CI
system. `perf` measures a workload against a live server, `perf-compare`
compares performance records or runs two servers interleaved, `perf-judge`
re-derives a performance record, `verify` checks a conformance certificate
offline, and the rest render or explore.

## emit-schemas

```text
Write the published JSON-Schema set (byte-deterministic)

Usage: veredictum emit-schemas --out <OUT>

Options:
      --out <OUT>  Output directory (created if missing)
  -h, --help       Print help
```

Byte-deterministic. The schemas in the repository's `schemas/` directory are
this command's output, drift-tested against it, which is how the published
format and the code that reads it stay one thing. Author against these if you
are writing your own catalogue or your own harness.

## test

```text
Drive the Arazzo catalogue against a server under test, or re-judge the recording of an earlier run, and write `results.json` and the HAR recording.

The whole catalogue is gated first, as `validate` gates it, and nothing runs while the gate has a finding. Every document under `cases/` runs, in path order, except the shared setups under `cases/_setup/`, which run only where a case names them in `dependsOn`. A workflow whose rows file says `server: exclusive` runs in a final phase, after every other workflow, and a re-judgement replays in the same order. Every row is printed, and a failed or errored row with its four-part explanation. The run ends with the view `show` prints, coloured only when standard output is a terminal and `NO_COLOR` is unset or empty.

The results record, under `server`, what the server stated through ITS-REST System OPTIONS: the run's own exchange when a workflow sent one, otherwise one read after the catalogue ran, which the recording keeps, so a re-judgement never asks the server. A member the server did not state stays absent. When the statement declares `spec_versions.its_rest` and the server states a different `restapi_specs_version`, the results carry a `review` note and the run prints it; the note changes no row and no exit status.

A credential is read from the environment, never from an argument: from the variables `--auth` and `--admin-auth` name, or from the file named by the same variable with `_FILE` appended. Neither credential is recorded. The recording holds the server's response bodies verbatim, so it belongs where the operator keeps the run.

Exit status follows whose problem each red row's record names. `1` when any row counts against the server: it failed (a warning row excepted) or errored with the server named, whatever else the run holds. `3` when no row counts against the server and at least one is inconclusive: it failed or errored with the instrument named, because the instrument could not judge it. `0` when every row passed or was skipped, or failed only as a warning (a SHOULD, which never counts against a capability). `2` when the run could not start or finish: a usage error, a gate finding, an unreadable file, a missing credential, or a recording that does not replay. The line before the exit names both counts. `--rejudge` exits the same way.

Usage: veredictum test [OPTIONS]

Options:
      --root <ROOT>
          The catalogue root, holding `capabilities.yaml` and `cases/`. Without `--specs`, its parent holds the vendored `specs/` tree its citations and OAS sources resolve against. The container image sets it to the catalogue it ships
          
          [env: VEREDICTUM_CATALOGUE]
          [default: catalogue]

      --base-url <BASE_URL>
          The server's base URL, up to and including the openEHR REST base (for example `https://cdr.example/openehr/v1`)

      --platform-url <PLATFORM_URL>
          The SMART Platform base URL (for example `https://platform.example`), which SMART on openEHR serves its discovery document relative to and which can differ from the openEHR REST base. Without it, every workflow with a step sent to it is skipped as `not-declared`

      --auth <AUTH>
          How the CDR wants the client to log in: `none`, `basic` or `bearer`.
          
          `none`: the CDR needs no login.
          
          `basic`: a username and password, from `VEREDICTUM_CDR_USER` and `VEREDICTUM_CDR_PASSWORD`.
          
          `bearer`: a token, from `VEREDICTUM_CDR_TOKEN`.
          
          [default: none]

      --admin-auth <ADMIN_AUTH>
          How the CDR wants the administrative principal to log in: `none`, `basic` or `bearer`. With `none`, every workflow with an administrative step is skipped as `not-declared`.
          
          `basic`: a username and password, from `VEREDICTUM_CDR_ADMIN_USER` and `VEREDICTUM_CDR_ADMIN_PASSWORD`.
          
          `bearer`: a token, from `VEREDICTUM_CDR_ADMIN_TOKEN`.
          
          [default: none]

      --statement <STATEMENT>
          The party's statement (JSON), which decides what runs.
          
          Its `claims.capabilities` names the capability folders under `cases/` it claims, at least one; every workflow of a document in another folder is skipped as `not-claimed`, while a setup a claimed document depends on still runs. Its `options` list selects one arm of each option in `capabilities.yaml`, and the documents of every other arm are skipped as `option-not-selected`. A document whose `applies` range in `capabilities.yaml` excludes a version its `spec_versions` declares is skipped as `version-out-of-range`, and one needing a wire format its its-rest `tech_profiles` entry does not list as `format-not-claimed`. A name that is not a folder or an arm stops the run. Without a statement, every capability runs, in every version and format, and no arm is selected.

      --ixit <IXIT>
          The party's IXIT (JSON), which the `declared` inputs of a rows file read. A row whose declared value is missing is skipped as `not-declared`.
          
          Its `scoped_tokens` member declares the tokens a step that names a SMART scope set presents: `issued` maps a scope set to the variable (`VEREDICTUM_SCOPED_TOKEN_<NAME>`, or the file its `_FILE` names) holding a token issued for it, and `mint` declares a test issuer the deployment trusts, whose RSA key is read from `VEREDICTUM_SCOPED_TOKEN_KEY`. A row whose scope set neither covers is skipped as `not-declared`; a declared variable that is not set stops the run before any request.

      --document <ID>
          Run only this document, by its id (`ehr` for `cases/ehr/ehr.arazzo.yaml`). Repeat the flag once per document; without it, every document runs

      --out <OUT>
          Where `results.json` and `recording.har` are written. Required for a live run; a re-judgement writes only `results.json`, and only when this is given

      --rejudge <RECORDING>
          Re-judge this HAR recording instead of driving a server: every request is answered from the recording, so the results are the bytes the recorded run wrote. Pass the root, statement, IXIT and documents the recorded run was given

      --profile <PROFILE>
          The specification text the case citations are read against.
          
          `development` (the default): the pinned `master` of each openEHR specification repository under `specs/openehr-development/`, the line that carries the newest text. A case citing text the snapshot lacks is skipped as `cited-text-absent`, naming what is missing. The results name the profile and every snapshot commit, and a development result never builds a certificate and is never compared with or counted toward a released result.
          
          `released`: the frozen release under `specs/openehr/`. Only a released result backs a released claim or a certificate.
          
          The catalogue is gated against the release under either profile. A re-judgement takes the profile the recorded run was given.

          Possible values:
          - development: The pinned `master` of each openEHR repository: the newest text
          - released:    The frozen release: the oracle of every released claim
          
          [default: development]

      --specs <SPECS>
          The vendored openEHR spec tree; citations resolve against the directory two levels above it, and the development snapshot sits at `specs/openehr-development/` beside it. Without it, the tree is `specs/openehr` in the parent of `--root`

  -h, --help
          Print help (see a summary with '-h')
```

`test` drives the Arazzo catalogue under `catalogue/`. It replaces the removed
`run` subcommand, which no longer exists under any name.

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
hardware a measured run was taken on, and only `perf` and `perf-compare`
read them. `test` reads no `instances` either, since `--base-url`
names the server, so `{ "environment": { "exclusive_server": true } }` is a
whole IXIT for it. The IXIT is checked against the published IXIT schema
before anything runs, and one that fails it, such as a misspelled member,
stops the run with exit status 2 naming the file and the violation.

**Which text.** `--profile development`, the default, reads every citation
against the pinned `master` of each openEHR specification repository, the
line that carries the newest text, under
`specs/openehr-development/`; `--profile released` reads the frozen release
under `specs/openehr/`. A case whose cited file, section or sentence the
snapshot lacks is skipped as `cited-text-absent`, naming the profile and what
is missing. The results record the profile under `specification`, with the
repository, branch and commit of every snapshot directory, and the terminal
view, `show`, every export and the console name them. A development result
never builds a certificate and is never compared with or counted toward a
released result, so a certificate needs a `--profile released` run. The
catalogue is gated against the release under either profile. Pass a
re-judgement the profile the recorded run was given.

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
  "options": ["ehr-xml-unsupported", "authentication-required"],
  "spec_versions": { "its_rest": "1.1.0" },
  "tech_profiles": [{ "its": "its-rest", "formats": ["canonical-json", "wt-flat"] }]
}
```

**Versions and formats.** `capabilities.yaml` states what a document needs
from the statement under `documents:`. An `applies` range bounds the release
versions of one component (`rm`, `base`, `am`, `aql`, `its_rest` or `term`,
the keys of the statement's `spec_versions`) with an inclusive `min`, `max`
or both, each `MAJOR.MINOR.PATCH`, and cites the released sentence that sets
the bound. When the statement declares a version of that component outside
the range, every workflow of the document is skipped as
`version-out-of-range`, citing that sentence. A component the statement does
not declare decides nothing, and a declared version that is not
`MAJOR.MINOR.PATCH` for a ranged component stops the run. A `formats` list
names the wire formats the server must serve or process for the document,
in the `tech_profiles` vocabulary (`canonical-json`, `canonical-xml`,
`wt-flat`, `wt-structured`, `wt`). When the statement's its-rest profile
leaves one out, every workflow of the document is skipped as
`format-not-claimed`, citing the statement schema's `tech_profiles` clause.
A statement without an its-rest profile claims every format. Only the
documents that need a server to serve or accept a format carry it: the
documents that pin a refusal of XML (406 or 415) run for every party, and the
option arm the statement selects decides them. A statement that selects
`ehr-xml-supported` and does not claim `canonical-xml` therefore runs nothing
of `ehr_xml`, and `validate --statement` refuses that arm (`arm-format`) and a
claimed folder in which nothing is left to run. Every document that sends
`application/json` needs `canonical-json`, so a statement claiming only
canonical XML runs the documents that need no JSON.

The IXIT answers the `declared` inputs a rows file reads. A workflow with an
administrative step needs the administrative principal: without
`--admin-auth`, it is skipped as `not-declared`. A workflow sent to the SMART
Platform base URL needs `--platform-url`, and a row that presents a SMART
scope set needs a token for it (see SMART on openEHR below); either is skipped
as `not-declared` without.

**Credentials.** `--auth` and `--admin-auth` say how the CDR wants each
principal to log in: `none`, `basic` or `bearer`, with the variables
[Logging in to the CDR](authentication.md) lists. Each secret is read from the
environment, from the variable itself or from a file named by the variable
with `_FILE` appended, which is how container secrets are mounted. Setting both is
refused. A variable set to the empty string counts as unset, because a CI
system expands a secret it does not hold to an empty value, so the run stops
on the missing credential instead of presenting an empty one. No credential
is recorded: the HAR carries `[not recorded]` in place of every
`Authorization` value, and records only whether the administrative principal
was supplied, the Platform base URL, and which scope sets the IXIT covers, so
a re-judgement skips the same workflows and rows.

**SMART on openEHR.** The `smart` folder carries the SMART on openEHR
documents. The specification is in the DEVELOPMENT state in Release-1.1.0, so
a statement may leave the folder unclaimed. Two things set it apart.

- **The Platform base URL:** SMART on openEHR serves its discovery document
  "relative to the _Platform_ base URL", which can differ from the openEHR
  REST base `--base-url` names. Pass it as `--platform-url` (for example
  `https://platform.example`). The route lives in
  `catalogue/sources/smart-platform.openapi.yaml`, Veredictum's own OpenAPI
  document, whose root says `x-veredictum-base: platform`; no released
  OpenAPI document declares it.
- **Scoped tokens:** a step that says `x-veredictum-scope` presents an access
  token whose `scope` claim is the named scope set, usually a rows-file input,
  so each row carries its own token. The instrument never asks an
  Authorization Server for one. The IXIT's `scoped_tokens` member says where
  each comes from:

  ```json
  {
    "scoped_tokens": {
      "issued": {
        "system/template-*.r": "VEREDICTUM_SCOPED_TOKEN_TEMPLATE_READ",
        "system/template-*.crud": "VEREDICTUM_SCOPED_TOKEN_TEMPLATE_CRUD"
      },
      "mint": {
        "issuer": "https://issuer.example",
        "subject": "veredictum",
        "audience": "cdr",
        "kid": "test-issuer",
        "ttl_seconds": 300,
        "claims": { "roles": ["clinician"] }
      }
    }
  }
  ```

  `issued` maps a scope set to the variable that holds a token your own
  Authorization Server issued for it, read like every other credential from
  the variable or its `_FILE`. A variable must be named
  `VEREDICTUM_SCOPED_TOKEN_<NAME>`, so an IXIT cannot point the instrument at
  any other secret in the environment. `mint` declares a test issuer your
  deployment trusts: for a scope set with no issued token, the instrument signs
  an RS256 token carrying `iss`, `sub`, `aud`, `iat`, `exp`, the
  space-delimited `scope` and any further `claims`, with the RSA private key in
  `VEREDICTUM_SCOPED_TOKEN_KEY` or `VEREDICTUM_SCOPED_TOKEN_KEY_FILE`. Never
  give it key material a production deployment trusts. Scope sets compare as
  sets, so `"openid user/x.r"` and `"user/x.r openid"` are one. Every declared
  variable is checked before the first request: an unset one stops the run
  with exit status 2. A row whose scope set neither member covers is skipped
  as `not-declared`, citing the SMART sentence that has the Platform evaluate
  scopes when it issues tokens.

  No openEHR specification governs how a test instrument obtains tokens; the
  member and the minting are this instrument's own design.

The SMART text does not say what a request outside its granted scopes
receives, or which operation a scope grants (upstream report #758), so the
catalogue carries the granted side only.

**The server's identity.** `results.json` records once, under `server`, what
the server stated about itself through the ITS-REST System OPTIONS operation
(`OPTIONS <base>/`), in the same classified form `veredictum identify`
prints. When a workflow of the run sent that request, `server.read` is
`exchange` and `server.entry` names the recording entry. When none did, the
run reads it once after the catalogue, through the ordinary principal's
credential, and `server.read` is `read` (or `unanswered` when the request got
no response); the recording's log keeps that read, so a re-judgement records
the same identity and never asks the server. The release the server states is
`server.answer.options.restapi_specs_version`, and a server that states none
leaves it absent. The operation says services SHOULD answer, so no outcome
here counts toward conformance.

When the statement declares `spec_versions.its_rest` and the server states a
different `restapi_specs_version`, compared as written, `results.json` carries
a `review` note of kind `restapi-version-differs` naming both values, and the
run prints it after the row counts. The note changes no row and no exit
status. No openEHR specification governs this comparison; it is this
instrument's own design.

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
    -e VEREDICTUM_CDR_TOKEN \
    ghcr.io/vernum-projecten/veredictum:<tag> \
    test --base-url https://cdr.example/openehr/v1 --auth bearer --out /work/out
```

The image runs as uid 65532, so the mounted directory must be writable by it.

**Other Arazzo tools.** The documents are plain Arazzo 1.1.0, so any Arazzo
tool reads them and a workflow runner can smoke-run them. The verdict is
`test`'s alone: the citations, the judges, the statement-driven selection and
the attribution of every red row live in the `x-veredictum-*` extensions and
in this pipeline, which a generic runner ignores.

## identify

```text
Read a server's identity from the ITS-REST System OPTIONS operation and print it as one JSON document.

Sends `OPTIONS <base>/` with `Accept: application/json`. The document's `outcome` is `offered` (a 200 `Options` document), `not_offered` (any other status from 100 to 599 outside 2xx, which the operation's SHOULD permits), `not_options_document` (a 2xx that is not that document) or `invalid_status` (a code from 600 to 999, which RFC 9110 §15 makes invalid). Nothing is judged and nothing counts toward conformance.

Usage: veredictum identify [OPTIONS] --base-url <BASE_URL>

Options:
      --base-url <BASE_URL>
          The server's base URL, up to and including the openEHR REST base

      --auth <AUTH>
          How the CDR wants the client to log in: `none`, `basic` or `bearer`. The secret never rides argv: each variable can instead name a file through its `_FILE` form.
          
          `none`: the CDR needs no login.
          
          `basic`: a username and password, from `VEREDICTUM_CDR_USER` and `VEREDICTUM_CDR_PASSWORD`.
          
          `bearer`: a token, from `VEREDICTUM_CDR_TOKEN`.
          
          [default: none]

  -h, --help
          Print help (see a summary with '-h')
```

Prints what the server states about itself through the ITS-REST System
`OPTIONS` operation, and exits `0` whatever the server answered: a server that
does not offer the document is within the specification, which says services
SHOULD answer. It exits `2` only when no answer arrived at all, or when the
credential `--auth` names is not supplied. It reads the credential exactly as
`test` does ([Logging in to the CDR](authentication.md)), `_FILE` forms included.

Run it before `test` to see which product and version a run against the same
base URL is about to grade.

The document it prints has an `outcome` of `offered`, `not_offered`,
`not_options_document` or `invalid_status`, and
`schemas/identity.schema.json` describes it, so a script reading it has a
schema to validate against.

## validate

```text
Validate the Arazzo catalogue through every gate, and review one supplied statement against it

Usage: veredictum validate [OPTIONS] --root <ROOT>

Options:
      --root <ROOT>
          The catalogue root, holding `capabilities.yaml` and `cases/`

      --specs <SPECS>
          The vendored openEHR spec tree; enables citation resolution and the surface gate

      --profile <PROFILE>
          The specification profile to report the case citations against, beside the gates.
          
          `released` (the default): the gates alone, against the frozen release. Zero findings is the only passing result.
          
          `development`: the gates as under `released`, then every case citation read against the snapshot under `specs/openehr-development/` beside `--specs`. A citation whose file, section or quote the snapshot lacks is printed as `[info]` and counted per component; it is never a finding. A citation refused for any other reason is a `profile-citation` finding. Needs `--specs`.

          Possible values:
          - development: The pinned `master` of each openEHR repository: the newest text
          - released:    The frozen release: the oracle of every released claim
          
          [default: released]

      --statement <STATEMENT>
          A submitted declaration (`statement.json`) to hold to the static review: its published schema, the capability folders it claims, the option arms it selects, and that every claimed folder holds a workflow its declarations let run.
          
          ISO/IEC 9646-7 assigns an ICS proforma's support and supported-values columns to the supplier of the implementation, so no declaration is committed here and none is swept from the tree.

  -h, --help
          Print help (see a summary with '-h')
```

Every gate over the Arazzo catalogue: the official Arazzo 1.1 schema, source
and parameter resolution, citation resolution against the vendored
specifications, the condition profile, requirement levels, unknown keys, the
rows files, surface coverage, identity and links, and the AQL parser check. It
prints one line per finding and a summary line naming the documents,
workflows, rows files and capability folders it read, and exits `1` if the
finding count is not zero.

`capabilities.yaml` also holds every capability folder to two marks. Its
`floors` give each folder the workflow count it keeps: a folder without a
floor, a floor naming no folder, and a folder holding fewer workflows than its
floor are refused (`arazzo-floor`), and the test suite re-derives every floor
from the catalogue, so a change that adds workflows raises the floor with
them. Its `extension-only` list names the folders verified only over the
routes of Veredictum's own sources, the documents under `catalogue/sources/`
whose `info` carries `x-veredictum-non-openehr: true`, which no openEHR OAS
describes: a listed folder with a step calling an unmarked source, and an
unlisted folder whose every step calls a marked one, are refused
(`arazzo-realization-scope`). Under `documents`, an `applies` range or a
`formats` entry naming no document is refused, as are a range starting after
its end and a format the document never sends in `Accept` or `Content-Type`.
A capability document that sends `application/json`, itself or through a
setup it names in `dependsOn`, and does not state `canonical-json` is refused
too.

`chapters.yaml` is the reading structure every view of a result follows: it
places each capability folder in one chapter and declares each folder's BASE
sections. A document no option arm names is BASE and names its section with
`x-veredictum-section` on its `info`; a document an arm names is OPTION and
names none. A folder in no chapter or in two, a BASE document with no section
or an unknown one, a section no document uses, an OPTION document with a
section, an option with no theme and tier, a spec-boundary option whose
citation names no report, and a `reports.yaml` out of step with the reports
the citations name are refused (`arazzo-structure`). A citation into a
document the release marks RETIRED is refused (`arazzo-citation`).

Supplying `--statement` adds the static review of that one declaration: it is
held to the published statement schema, which requires at least one claimed
capability folder, every capability it claims must be a capability folder of
the catalogue, and every option arm it selects must be an arm of an option in
`capabilities.yaml`, one arm per option. Each claimed folder must then hold a
document the statement lets run: one whose arm it selects, whose `applies`
ranges contain the versions it declares, and whose formats it claims. A claim
nothing can run for is refused (`claim-completeness`), since it would verify
nothing. A selected option arm one of whose documents needs a format the
statement does not claim is refused (`arm-format`): the run would skip that
document, so nothing would verify the arm. No openEHR specification governs the floors, the realization scope
or the claim gates; they are this instrument's own design.

`--profile development` keeps every gate above, against the release, and then
reads every case citation against the development snapshot under
`specs/openehr-development/` beside `--specs`. A citation whose file, section
or sentence the snapshot lacks prints as an `[info]` line naming the case, and
the profile's counts follow per component; neither is a finding, and the exit
status stays `0` for them. A citation the snapshot refuses for any other
reason is a `profile-citation` finding, and so is a citation's `development`
wording that the snapshot does not hold, or one whose released quote still
occurs there unchanged. The repository's own gate runs
`--profile released`, where zero findings is the only passing result, and a
second `--profile development` pass that reports the absences.

## perf

```text
Run a workload document against a live server and write one `perf-result`.

The corpus is seeded (or reused with `--corpus reuse`) before the clock, closed-loop and reported apart. The posture canaries then take their first bracket, and the run after seeding takes at most ten minutes: a 15 s settle, a 20 s warm-up, a 60 s scout ramp judged in 2 s windows, a drain, a ladder of four plateaus at 0.5, 0.75, 0.9 and 1.05 times the knee estimate (a fifth at 0.25 only when the lowest fails), and the closing canaries. The planned duration is printed before the first measured request. A server that answers any exchange `429` gets no record, and a canary that contradicts the declared posture refuses the run.

Usage: veredictum perf [OPTIONS] --root <ROOT> --ixit <IXIT> --workload <WORKLOAD> --out <OUT>

Options:
      --root <ROOT>
          The performance root, `perf/` in this repository (workloads/, payloads.yaml, payloads/, hosts/)

      --ixit <IXIT>
          The ixit topology file (JSON); its environment block is mandatory for a measured run

      --workload <WORKLOAD>
          The workload document to run, an embedded one under `perf/workloads/` or the evaluator's own (labelled `custom`)

      --out <OUT>
          Where the `perf-result` record is written

      --catalogue <CATALOGUE>
          The catalogue root a workload's `requires` resolve against
          
          [default: catalogue]

      --statement <STATEMENT>
          A statement whose claims keep the interactions it covers and drop the rest, with the drop recorded

      --corpus <CORPUS>
          Seed the workload's corpus, or reuse the stored one: a reuse is refused before the warm-up when the server's read-back counts or the stored digest differ

          Possible values:
          - seed:  Seed the workload's recipe before the clock
          - reuse: Reuse the stored corpus when its read-back counts and digest match
          
          [default: seed]

      --corpus-state <CORPUS_STATE>
          The stored corpus state a seeding writes and a reuse reads; defaults to the record's path with the extension `corpus.json`

      --seed-workers <SEED_WORKERS>
          Parallel closed-loop seeding workers
          
          [default: 16]

      --start-rate <START_RATE>
          The warm-up's rate and the scout's starting rate, operations per second
          
          [default: 8]

      --scout-end-rate <SCOUT_END_RATE>
          The scout ramp's end rate, operations per second
          
          [default: 4096]

      --host-prescription <HOST_PRESCRIPTION>
          The id of the host prescription under `perf/hosts/` the run claims to have run on; the record reads the generator's measured fingerprint against it

      --network-leg <NETWORK_LEG>
          Where the load generator sits relative to the server

          Possible values:
          - same-host:    The generator and the server share one host
          - same-zone:    Different hosts in one zone of one provider
          - cross-region: Different regions
          
          [default: same-host]

      --posture <POSTURE>
          The posture profile the run declares: `minimal`, `minimal-signed-digest`, `minimal-signed-pgp` or `clinical-default`. The canaries check it after seeding and after the last plateau, and a contradiction refuses the run
          
          [default: minimal]

  -h, --help
          Print help (see a summary with '-h')
```

Latency is measured from the planned arrival instant under open-loop Poisson
arrivals, so a stall shows up as latency instead of disappearing into a
slowed-down request rate. The run has one shape, and no flag lengthens it: the
ladder finds the knee in at most ten minutes after seeding, and the record
says what such a run cannot show. [Measured performance](running.md#measured-performance)
walks a run phase by phase and explains how to read the record.

`--root` is `perf/` in this repository: the workloads, the payloads they send
and the host prescriptions. `--workload` names one workload document, an
embedded one or your own, which is labelled `custom`. `--catalogue` is only
read for the capability folders and option arms a workload's `requires`
tokens name, and `--statement` drops the interactions your claims do not
cover, with the drop recorded.

The corpus is seeded before the clock, closed-loop, and its listing is stored
beside the record (`--corpus-state`, by default the record's path with the
extension `corpus.json`). `--corpus reuse` reuses that corpus instead of
seeding again, and is refused before the warm-up when the server's read-back
counts, the stored digest or the recipe moved.

The `environment` block is mandatory because a latency number is a claim
about a deployment, and a claim with no deployment described cannot be checked
or reproduced. Every member of it is mandatory here (`hardware_class`,
`cores`, `memory_gb`, `storage_class`, `topology`), and a run whose IXIT lacks
one stops before it sends a request, naming the member. A conformance run
reads only `exclusive_server`, so a conformance-only IXIT may leave the others
out. The optional `containers` block names the server and database containers
and the `database` the server-side readings query.

`--posture` declares the posture profile the canaries check after seeding and
after the last plateau; a contradiction refuses the run. `--host-prescription`
names a prescription under `perf/hosts/`, which the record embeds and reads
the generator's measured fingerprint against, and `--network-leg` says where
the generator sat.

## perf-workloads

```text
Load the performance workloads through their gates and print, for each, the expanded per-operation mix, its read:write ratio and the planned run range after seeding. With --out, write the byte-deterministic manifest of the pinned workloads with their digests and the page generated from it; with --workload, check one document instead, labelled custom unless it matches a pinned digest

Usage: veredictum perf-workloads [OPTIONS] --root <ROOT> --catalogue <CATALOGUE>

Options:
      --root <ROOT>            The performance root, perf/ in this repository (workloads/, payloads.yaml, payloads/)
      --catalogue <CATALOGUE>  The catalogue root a workload's requires tokens resolve against (catalogue/ in this repository)
      --out <OUT>              Output directory for perf-workloads.json and perf-workloads.md (created if missing)
      --workload <WORKLOAD>    One workload document to check and summarize
  -h, --help                   Print help
```

A workload is the traffic a measured run offers: a versioned document under
`perf/workloads/`, published as `schemas/perf-workload.schema.json`. This
command loads every workload through its gates and prints, per workload, the
expanded per-operation mix, the read:write ratio and the planned run range
after seeding. A defect is reported against the file and the field, and the
command exits `1`.

With `--out` it writes `perf-workloads.json`, the byte-deterministic manifest
of the workloads this binary pins by sha256, and `perf-workloads.md`, the page
generated from it: every interaction, operation, query class and payload with
its share and its source. The book's [Performance workloads](perf-workloads.md)
page is that page, written with `--out website/book/src` and held to the
pinned workloads by a test. A committed workload whose bytes match no pin is
refused there, so a changed workload raises its version and its pin together.
The command also holds `perf/payloads.yaml` to what the workloads read: an
entry no workload reads is refused, as is a pinned payload whose bytes moved.

With `--workload` it checks one document of your own instead. A document that
matches no pinned digest is labelled `custom`.

## perf-compare

```text
Compare performance records: align committed `perf-result` files, or run two targets (`--ab`) or one target against itself (`--aa`) interleaved on one host.

With `--result`, two or more records are read into one table, one column per file. Every field on which they describe different things is named above it: the workload digest, the corpus recipe, the posture, the host prescription, the generator's fingerprint and the network leg. Two records also get a ratio per plateau, operation and metric, with both values beside it; plateaus offered different rates are shown and not judged.

With `--ab`, both targets are seeded from the same recipe and seed and stay resident on one host. Each gets its settle, warm-up and scout; the shared ladder is built from the larger knee estimate, so both sides are offered the same rates and holds; and on each rung the plateaus alternate in ABBA order. `--aa` runs the same interleaving with one target on both sides and attaches its floor, the largest A/A ratio per metric, to both records. A difference counts only when its ratio exceeds the tolerance and the attached floor and the two sides' 95 % intervals do not overlap. Both write `a.perf-result.json`, `b.perf-result.json`, `perf-compare.json` and `perf-compare.md` under `--out`, in about twice a single run.

Exit status: `0` when the records align; `1` when any mismatch applies; `2` when a record cannot be read or a run cannot finish.

Usage: veredictum perf-compare [OPTIONS]

Options:
      --result <RESULTS>
          A committed `perf-result`. Repeat the flag once per file; at least two are needed

      --ab <IXIT_A> <IXIT_B>
          Side A's and side B's IXIT, for an interleaved A/B run

      --aa <IXIT>
          The IXIT of the one target an A/A run puts on both sides

      --with-baselines <IXIT>
          The IXIT of a target to run against every pinned reference CDR (EHRbase and FerroEHR), each composed on this host from its own digest-pinned images with the docker CLI and torn down with its volumes afterwards. Each comparison, the target's index relative to that reference, is written under `--out/<reference>/`

      --root <ROOT>
          The performance root, `perf/` in this repository (workloads/, payloads.yaml, payloads/, hosts/). Needed by `--ab` and `--aa`

      --workload <WORKLOAD>
          The workload document both sides are offered. Needed by `--ab` and `--aa`

      --catalogue <CATALOGUE>
          The catalogue root a workload's `requires` resolve against
          
          [default: catalogue]

      --statement <STATEMENT>
          A statement whose claims keep the interactions it covers and drop the rest

      --corpus <CORPUS>
          Seed both corpora, or reuse the ones stored under `--out`

          Possible values:
          - seed:  Seed the workload's recipe before the clock
          - reuse: Reuse the stored corpus when its read-back counts and digest match
          
          [default: seed]

      --seed-workers <SEED_WORKERS>
          Parallel seeding workers
          
          [default: 16]

      --start-rate <START_RATE>
          The warm-up's rate and the scout's starting rate, operations per second
          
          [default: 8]

      --scout-end-rate <SCOUT_END_RATE>
          The scout ramp's end rate, operations per second
          
          [default: 4096]

      --host-prescription <HOST_PRESCRIPTION>
          The id of the host prescription under `perf/hosts/` the run claims

      --network-leg <NETWORK_LEG>
          Where the load generator sits relative to the servers

          Possible values:
          - same-host:    The generator and the server share one host
          - same-zone:    Different hosts in one zone of one provider
          - cross-region: Different regions
          
          [default: same-host]

      --posture <POSTURE>
          The posture profile both sides declare
          
          [default: minimal]

      --tolerance <TOLERANCE>
          The declared tolerance: a ratio within it either way is never a difference
          
          [default: 0.05]

      --floor <FLOOR>
          A `perf-result` or `perf-compare.json` carrying an A/A floor, which the ratios are judged against and an A/B run attaches to both records

      --out <OUT>
          Output directory: the comparison for `--result`, and the records, corpora and comparison for `--ab` and `--aa`

  -h, --help
          Print help (see a summary with '-h')
```

A single run carries sampling intervals and nothing else. Two GitHub-hosted
runs of the same code have read up to 156 % apart, so a number from one run
set beside a number from another says little about the two systems. An
interleaved run puts both sides on one host in one window: each rung of the
shared ladder runs A then B, the next rung B then A, so a drift in the host
during the run lands on both sides.

`--aa` measures what that interleaving leaves: the same target on both sides.
Its floor, the largest ratio per metric, is attached to both records and turns
their `between_run_noise` from `unmeasured` into the floor. Pass that record or
its `perf-compare.json` as `--floor` to an `--ab` run, and a ratio has to clear
the floor as well as `--tolerance` before it reads as a difference.

With `--result`, the records were measured apart, so their plateaus were
offered different rates. The table still lines them up, and the ratios are
shown as `different rates` rather than judged.

## perf-assets

```text
Render the charts of a `perf-result` as SVG, deterministically: the same record always draws the same bytes.

Three charts: `perf-saturation.svg` (p50, p95 and p99 against achieved throughput over the scout windows and the plateaus, the knee bracket shaded, the Universal Scalability Law fit dashed), `perf-throughput.svg` (achieved against offered with the identity line) and `perf-operations.svg` (each operation's latency at the knee at its judged quantile, beside its budget). A quantile that does not resolve is marked below the plot and never joined to its neighbours.

Usage: veredictum perf-assets [OPTIONS] --results <RESULTS> --out <OUT>

Options:
      --results <RESULTS>
          The committed `perf-result.json`

      --out <OUT>
          Output directory for the SVG files

      --summary <SUMMARY>
          Also write the `export --markdown` view of the record to this path, the book's build-time include

  -h, --help
          Print help (see a summary with '-h')
```

## perf-judge

```text
Re-derive every number of a `perf-result` record from its recorded histograms, and compare the result with the record byte for byte.

The record is checked against `perf-result.schema.json`, its `derived` part is computed again from its `raw` part, and every derived field that differs is printed with its JSON pointer, the recorded value and the re-derived one. The file must also equal the canonical rendering of the re-derived record, so a number written in a different form counts as a difference.

Exit status: `0` when the record reproduces byte for byte; `1` when a derived field differs or the file is not in canonical form; `2` when the file cannot be read, is not JSON, breaks the schema, or its raw part does not re-derive.

Usage: veredictum perf-judge --result <RESULT>

Options:
      --result <RESULT>
          The record (`perf-result.json`)

  -h, --help
          Print help (see a summary with '-h')
```

## export

```text
Write the views of a result document a vendor's CI shows, each a byte-deterministic function of the document.

An Arazzo results document has three views: JUnit XML, CTRF 0.2.0 JSON and a Markdown summary. A `perf-result`, told apart by its `family`, has the Markdown summary only, so `--junit` and `--ctrf` refuse it.

A result judged under the development profile names the profile and every snapshot commit in each view: a `profile` property on every JUnit suite, a `specification` member in the CTRF environment, and a `Specification` row with a warning in the Markdown summary.

Usage: veredictum export [OPTIONS] --results <RESULTS>

Options:
      --results <RESULTS>
          The `results.json` the Arazzo evaluator wrote, or a `perf-result.json`

      --junit <JUNIT>
          Write the JUnit XML view here

      --ctrf <CTRF>
          Write the CTRF JSON view here

      --markdown <MARKDOWN>
          Write the Markdown summary here. Without this flag and without a job summary file, the summary is printed to standard output

      --step-summary <STEP_SUMMARY>
          The GitHub Actions job summary file the Markdown summary is appended to. GitHub sets the variable in every job step
          
          [env: GITHUB_STEP_SUMMARY]

      --annotations <ANNOTATIONS>
          Write the GitHub Actions annotations here: one `::error` workflow command per failed or errored row and one `::warning` per warning row, titled with the case id and carrying what was checked, the cited sentence and what was expected and received. GitHub shows at most 10 of each kind per step, so at most 10 of each are written and a closing `::notice` names how many more rows the summary lists. An Arazzo results document only

  -h, --help
          Print help (see a summary with '-h')
```

`export` reads the `results.json` the Arazzo evaluator writes and renders the
three views a CI system shows. Each is a pure function of that document, so
exporting the same results twice writes the same bytes, and none of them
carries a clock reading the results document does not carry.

| Status | JUnit XML | CTRF 0.2.0 |
|---|---|---|
| passed | a bare `<testcase>` | `passed` |
| failed | `<failure>` | `failed` |
| warning | `<system-out>`, no `<failure>` | `other`, with `rawStatus: warning` |
| errored | `<error>` | `other`, with `rawStatus: errored` |
| skipped | `<skipped>` | `skipped` |

A warning row is a SHOULD the server did not follow, which never counts
against the server, so no view counts it as failed and a CI that fails a job
on a failed test does not fail it on one. Its JUnit `<testcase>` carries the
four-part explanation in `<system-out>`, opened by a line that says it is
a warning, and no `failures` count includes it. In CTRF it is `other`, and
`summary.failed` excludes it.

A workflow is a JUnit `<testsuite>` and a row is a `<testcase>` named
`<case> row <n>`. The text of a `<failure>` or `<error>` is the row's
four-part explanation, its `message` is a one-line summary, and its `type`
says whose problem the row is (`server` or `instrument`). A
`<skipped>` carries the skip reason and the citation that makes the case not
apply. A suite whose case the results place carries `<properties>` naming the
part, chapter, band, capability, section or option and arm, formalism, TRIAL
mark and upstream reports, each property named `eu.veredictum.<name>`. Every
`time` is `0`, because the results document carries no timing.

JUnit XML has no single owner, so the view is held to the two schemas the tools
that read it publish: the document to the Jenkins xUnit plugin's `junit-10.xsd`,
which defines the `<testsuites>` root, and each `<testsuite>` to Apache Maven
Surefire's test report schema. Both are vendored, and a CI gate validates the
pinned views against them with `xmllint`.

In CTRF, every test has `type` `conformance`, a `suite` path of the document
then the part, chapter, band and section or option it is read under, and
`labels` naming where it sits. A red row's `message` is the same four-part
explanation. Everything CTRF does not define sits in `extra` under the one
namespaced key `eu.veredictum`, as CTRF §4.6.1 asks: the case id, the row, the
skip reason, the citation and, where the citation names one, the link to its
upstream report. The CTRF document validates against the schema the CTRF
project publishes for `specVersion` 0.2.0, its latest release.

**What the Markdown summary holds.** It is written for a person reading it on
GitHub, in two parts. A table names the server and the instrument, and a
summary table counts the rows of BASE and OPTIONS. `## BASE` follows chapter by
chapter, each chapter a table of its bands and each band a table of its
sections. `## OPTIONS` follows the same chapters and bands, each band a table
of its options with their kind (Recommended (SHOULD), Optional (MAY), or
`@SpecDiverge` for an option that exists only because the released text is
unclear) and the
arm the statement selected, and a collapsed note on what each option decides.
The cases resting on TRIAL text sit in their band under OPTIONS. Passed rows
are only counted. Each failed, warning or errored row is a collapsed block
whose summary names the outcome, the case and where it sits; opening it shows
the step, what was checked, expected against received, the exchange, whose
problem it is and the row's inputs as a table, with the cited sentence quoted
below it. The warning rows follow with their citations, and the skipped rows
counted by reason with the full list collapsed. A case resting on text an
upstream report records as unclear or contradictory is tagged `@SpecDiverge`
where it appears, and a closing note counts those cases and links the reports
they are judged by. JUnit carries the tag as the `eu.veredictum.tag` property
and CTRF in the test's standard `tags`.

**Where the Markdown summary goes.** GitHub Actions sets
`GITHUB_STEP_SUMMARY` in every job step, and `export` appends the summary to
that file, so the job's summary page shows it with no third-party action.
`--markdown <file>` writes it to a file as well. With neither, the summary is
printed to standard output and every other console line goes to standard
error, so the summary can be piped.

**A vendor's GitHub Actions job.** [The GitHub Action](github-action.md) runs
[`test`](#test) and then `export` in one step, with the image of the release it
belongs to:

```yaml
      - uses: Vernum-Projecten/Veredictum@<commit-sha> # vX.Y.Z
        env:
          VEREDICTUM_CDR_USER: ${{ secrets.CDR_USER }}
          VEREDICTUM_CDR_PASSWORD: ${{ secrets.CDR_PASSWORD }}
        with:
          base-url: ${{ vars.CDR_BASE_URL }}
          statement: conformance/statement.json
          auth: basic
          out-dir: conformance
```

The step writes `junit.xml`, `ctrf.json` and `summary.md` into `conformance/`,
appends the summary to the job summary, annotates the rows that need attention,
and fails the job when a row counts against the server. Its page has a
complete workflow, and [GitLab CI](gitlab-ci.md) has the same as a template.

`--annotations <file>` writes those annotations as GitHub Actions workflow
commands: one `::error` per failed or errored row and one `::warning` per
warning row, at most 10 of each, then a `::notice` naming how many more rows
the summary lists.

The JUnit file is read by GitLab (`artifacts:reports:junit`), Jenkins (the
JUnit plugin) and most other CI systems. `conformance/recording.har` holds the
server's response bodies verbatim, so it belongs where the operator keeps the
run, never in a public artifact.

A `perf-result.json` has one exported view, the Markdown summary, with the
same content `show` prints laid out as tables. `--junit`, `--ctrf` and
`--annotations` refuse it with exit status 2, since a performance record has
no test cases to list.

## badge

```text
Draw the static badges of a result document, for a README.

Each badge is written as `<name>.svg`, a flat badge drawn without a font or the network, and as `<name>.json`, a Shields.io endpoint document (<https://shields.io/badges/endpoint-badge>), and each is a byte-deterministic function of the document. Every badge reports the vendor's own run, and none is a certificate.

Each badge carries one fact: the label names the subject and the message gives one short value. The SVG title and `aria-label` carry the full text.

An Arazzo results document draws four: `base` (the BASE rows), `options` (the rows of the selected option arms and of the cases resting on TRIAL text), `profile` (the specification profile) and `instrument` (`self-run` and the version). A counting badge reads passed out of the rows that bear on the verdict (passed, failed, errored), then its warning count; its title adds the failed and errored counts. A warning row, a SHOULD the server did not follow, is neither a pass nor a failure and stays out of the ratio. A counting badge of a development result says `dev` in its label, so a development tally never reads as a released one. A counting badge is green only when no row failed or errored: red when a row counts against the server (it failed, or errored with the server or no one named), orange when none does but an errored row is inconclusive, grey when nothing passed. A warning changes no colour.

A `perf-result` draws three: `perf` (the workload in the label, the knee in whole requests per second, rounded outward), `noise` (whether an A/A floor is attached) and `instrument`. A knee is a reading and never a pass or a failure, so it is drawn in a neutral colour.

Exit status: `0` when every badge was written; `2` when the document cannot be read or a file cannot be written.

Usage: veredictum badge --results <RESULTS> --out <OUT>

Options:
      --results <RESULTS>
          The `results.json` the Arazzo evaluator wrote, or a `perf-result.json`

      --out <OUT>
          Output directory for the badges (created if missing)

  -h, --help
          Print help (see a summary with '-h')
```

`badge` draws the static badges a vendor shows in a README, from the result
document it is given. Each badge is a pure function of that document, so
drawing the same document twice writes the same bytes. The SVG is measured
with a fixed table of character widths, so no font and no network is read.

| Document | Files written |
|---|---|
| an Arazzo `results.json` | `base`, `options`, `profile` and `instrument`, each `.svg` and `.json` |
| a `perf-result.json` | `perf`, `noise` and `instrument`, each `.svg` and `.json` |

```bash
veredictum badge --results conformance/results.json --out conformance/badges
```

The `.json` file is a Shields.io endpoint document, so a badge can also be
served by Shields from wherever the file is published. Every badge reports the
vendor's own run and is never a certificate. [Badges](badges.md) has what each
badge says, the colour rules, and how to publish them from GitHub, GitLab or
any other CI.

## show

```text
Show a result document in the terminal: an Arazzo results document one line per chapter, band and option, each with its counts and a bar, or a `perf-result` one line per plateau.

A result reads in two parts. BASE comes first, chapter by chapter and band by band, in the order the catalogue declares. OPTIONS follows by the same chapters and bands, one line per option naming its selected arm and its kind (Recommended (SHOULD), Optional (MAY), or `@SpecDiverge` for an option that exists only because the released text is unclear), with the cases resting on TRIAL text in their band. A closing note counts the cases tagged `@SpecDiverge` and links the reports they are judged by. Every mark is a glyph as well as a colour: ✓ passed (green), ✕ failed (red), ? errored (yellow), ! warning and ○ skipped (grey). A warning row is a SHOULD the server did not follow and is never shown as a failure. A result judged under the development profile opens with a line naming the profile and every snapshot commit, and saying it is never a released result.

A `perf-result` reads as a header naming the target, environment, host, workload (a `custom` one named so), corpus, the knee estimate, the between-run noise and the record's scope; one line per plateau with offered and achieved rates, p50 to max, the error share and a ✓ or ✕ qualification mark, with `unsteady` where recorded; the operations at the knee and at the lowest plateau with the quantile each is judged at (`n/r` where a quantile does not resolve); the knee, peak, population and fit sentences; a sparkline of p99 against the offered rate; and the server-side readings as context. It fits 80 columns. `--failed` does not apply to it.

Exit status: `0` when the view was printed; `2` when the document cannot be read.

Usage: veredictum show [OPTIONS] <RESULTS>

Arguments:
  <RESULTS>
          The `results.json` the Arazzo evaluator wrote, or a `perf-result.json`

Options:
      --failed
          Expand every failed and errored row with its four-part explanation, then list the warning rows apart

      --color <COLOR>
          When to colour the view. `auto` colours when standard output is a terminal and `NO_COLOR` is unset or empty (<https://no-color.org/>)

          Possible values:
          - auto:   Colour on a terminal unless `NO_COLOR` is set
          - always: Always colour
          - never:  Never colour
          
          [default: auto]

  -h, --help
          Print help (see a summary with '-h')
```

`show` reads the `results.json` the Arazzo evaluator writes and prints it in
the terminal, so a result can be read without exporting it first.
[`test`](#test) prints the same view when a run finishes.

Each line is one group of rows, with a bar of 24 cells and the count of each
mark. The bar gives every mark that has a row at least one cell, so a single
failure among a thousand passes still shows. The cells differ in shape as well
as colour: `█` passed, `▓` failed, `▒` errored, `░` warning, `·` skipped.

```text
BASE
  Every BASE case                           ████████████▓▓▓▓▓▓▓▓░░░░  ✓   3  ✕   2  ?   0  !   1  ○   0
  EHR                                       ████████████▓▓▓▓▓▓▓▓░░░░  ✓   3  ✕   2  ?   0  !   1  ○   0
    COMPOSITION                             ████████████▓▓▓▓▓▓░░░░░░  ✓   2  ✕   1  ?   0  !   1  ○   0
    DIRECTORY                               no cases
```

A result reads in two parts, the ones every view uses. BASE lists each chapter
and band in the order the catalogue declares. OPTIONS lists the same chapters
and bands, one line per option with the arm the statement selected and its
kind: Recommended (SHOULD), Optional (MAY), or `@SpecDiverge` for an option
that exists only because the released text is unclear or contradictory. A
closing note counts the cases tagged `@SpecDiverge` and names the upstream
reports they are judged by.
An option with no arm selected is listed once and its rows count nowhere. The
cases whose citations rest on text the release marks TRIAL are listed in their
band under OPTIONS, never in a BASE count. Report numbers come from the snapshot
the catalogue commits; nothing is fetched while the view is printed.

A warning row is a SHOULD the server did not follow. It is counted apart
from the failed rows, and `--failed` lists it under its own heading after the
failed and errored rows, never among them.

**Colour.** With `--color auto`, the default, the view is coloured only when
standard output is a terminal and `NO_COLOR` is unset or empty, as
<https://no-color.org/> asks. `--color always` and `--color never` override
both, so `veredictum show results.json --color always | less -R` keeps the
colour.

**A performance record.** Given a `perf-result.json`, which names its
`family`, `show` prints the record instead: a header with the target, the
environment, the workload and its digest, the corpus, the between-run noise
and the scope of one run; a line per plateau with offered and achieved rates,
p50 to max and the error share, marked `✓` when it qualifies and `✕` when it
fails, with `unsteady` where the halves disagree; the operations at the knee
and at the lowest plateau with the quantile each is judged at; the knee, peak,
population and fit sentences; a sparkline of p99 against the offered rate;
and the server-side readings. A quantile with fewer than 10 samples above it
prints `n/r`, not resolvable. Every number comes from the record, and the view
fits 80 columns.

## verify

```text
Verify a conformance certificate offline against a JWK Set, and print what it states.

A certificate is a JWS in the compact serialization, signed with Ed25519 (`EdDSA`), whose header names its key by `kid`. It is refused when the header or the payload changed after signing, when the set holds no key under its `kid`, when its algorithm or type is another, when the set marks the key revoked, and when the key expired before the certificate's `issued_at`. Nothing is fetched: the set is the file you name.

The payload carries the SHA-256 digests of the run's `results.json` and HAR recording, so `sha256sum` over the two files checks that they are the exchanges the certificate covers.

Exit status: `0` when the certificate verifies; `1` when it is refused; `2` when a file cannot be read or the JWK Set itself is refused.

Usage: veredictum verify --jwks <JWKS> <CERTIFICATE>

Arguments:
  <CERTIFICATE>
          The certificate (`certificate.jws`)

Options:
      --jwks <JWKS>
          The JWK Set holding the verifying keys: the published `jwks.json`, or a copy of it

  -h, --help
          Print help (see a summary with '-h')
```

The offline check of a conformance certificate. It reads the two files you
name and reaches no network, so a certificate verifies with the published JWK
Set alone: the copy at `https://console.veredictum.eu/.well-known/jwks.json`,
or the one committed in this repository as `deploy/hosted/jwks.json`. Any JOSE
library verifies the same signature; what this command adds is the refusal of
a key the set marks `revoked_at`, and of a certificate issued after its key's
`expires_at`, which a generic library ignores.

A verified certificate prints what it states: the server with who supplied
each identity field, the base URL, the catalogue digest, the engine version and
its approval status, the verdict summary, and the SHA-256 digests of the run's
`results.json` and HAR recording. Run `sha256sum` over the two files a vendor
received to check they are the exchanges the certificate covers. The payload's
JSON Schema is `schemas/certificate.schema.json`, and the key set's is
`schemas/jwks.schema.json`.
