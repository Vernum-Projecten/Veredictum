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
system. Three commands measure (`perf`, `stress`, `bench`), `verdicts`
re-derives the measured class verdicts, `verify` checks a conformance
certificate offline, and the rest render or explore.

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

The whole catalogue is gated first, as `validate` gates it, and nothing runs while the gate has a finding. Every document under `cases/` runs, in path order, except the shared setups under `cases/_setup/`, which run only where a case names them in `dependsOn`. A workflow whose rows file says `server: exclusive` runs in a final phase, after every other workflow, and a re-judgement replays in the same order. Every row is printed, and a failed or errored row with its four-part explanation.

A credential is read from the environment, never from an argument: from the variables `--auth` and `--admin-auth` name, or from the file named by the same variable with `_FILE` appended. Neither credential is recorded. The recording holds the server's response bodies verbatim, so it belongs where the operator keeps the run.

Exit status: `0` when every row passed or was skipped, or failed only as advisory (a SHOULD, which never counts against a capability); `1` when any other row failed or errored; `2` when the run could not start or finish: a usage error, a gate finding, an unreadable file, a missing credential, or a recording that does not replay.

Usage: veredictum test [OPTIONS]

Options:
      --root <ROOT>
          The catalogue root, holding `capabilities.yaml` and `cases/`. Its parent holds the vendored `specs/` tree its citations and OAS sources resolve against. The container image sets it to the catalogue it ships
          
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
of `ehr_xml`, and `validate --statement` refuses a claimed folder in which
nothing is left to run.

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

Sends `OPTIONS <base>/` with `Accept: application/json`. The document's `outcome` is `offered` (a 200 `Options` document), `not_offered` (any status outside 2xx, which the operation's SHOULD permits) or `not_options_document` (a 2xx that is not that document). Nothing is judged and nothing counts toward conformance.

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

## validate

```text
Validate the Arazzo catalogue through every gate, and review one supplied statement against it

Usage: veredictum validate [OPTIONS] --root <ROOT>

Options:
      --root <ROOT>
          The catalogue root, holding `capabilities.yaml` and `cases/`

      --specs <SPECS>
          The vendored openEHR spec tree; enables citation resolution and the surface gate

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
routes of `catalogue/sources/veredictum-extensions.openapi.yaml`, which no
openEHR OAS describes: a listed folder with a step calling another source,
and an unlisted folder whose every step calls that one, are refused
(`arazzo-realization-scope`). Under `documents`, an `applies` range or a
`formats` entry naming no document is refused, as are a range starting after
its end and a format the document never sends in `Accept` or `Content-Type`.

Supplying `--statement` adds the static review of that one declaration: it is
held to the published statement schema, which requires at least one claimed
capability folder, every capability it claims must be a capability folder of
the catalogue, and every option arm it selects must be an arm of an option in
`capabilities.yaml`, one arm per option. Each claimed folder must then hold a
document the statement lets run: one whose arm it selects, whose `applies`
ranges contain the versions it declares, and whose formats it claims. A claim
nothing can run for is refused (`claim-completeness`), since it would verify
nothing. No openEHR specification governs the floors, the realization scope
or the claim gates; they are this instrument's own design.

## perf

```text
Execute the performance schedule's open-loop measured run(s) against a live SUT and merge the §8.10 measurement records into a results.json (conformance-by-measurement), creating it when `--results` names no file.

A new record carries the SUT identity the server states through System OPTIONS, read before any seeding, with `--sut-name` and `--sut-version` filling what it does not state. It records no conformance outcome: nothing selected a statement, so it is `statement_blind`, and the verification pack is `not_run`. An existing file keeps its header and gains or replaces the measured case's record.

Usage: veredictum perf [OPTIONS] --root <ROOT> --ixit <IXIT> --results <RESULTS> --class <CLASS>

Options:
      --root <ROOT>
          The performance schedule root, `perf/` in this repository (classes/, journeys.yaml, payloads.yaml, payloads/)

      --ixit <IXIT>
          The ixit topology file (JSON) — its environment block is mandatory for a measured run

      --results <RESULTS>
          The measured results record (`results.schema.json`) to merge the measurement records into; created when no file exists at the path

      --sut-name <SUT_NAME>
          The SUT name a new record carries when the server does not state one through System OPTIONS. Unread when `--results` exists

      --sut-version <SUT_VERSION>
          The SUT version a new record carries when the server does not state one through System OPTIONS. Unread when `--results` exists

      --class <CLASS>
          Select the performance case(s) of this class (POC | S | L | R)

      --seed-workers <SEED_WORKERS>
          Parallel seeding workers
          
          [default: 16]

      --hours <HOURS>
          The sustained-window ladder: hours to hold the offered load — 1 (default, the case's normative window) | 2 | 4 | 6 | 8 | 12. A longer window is a STRICTER demonstration and persists like any measured run; nothing shorter than the case exists
          
          [default: 1]

  -h, --help
          Print help (see a summary with '-h')
```

Conformance by measurement. Latency is measured from the planned arrival instant
under open-loop offered load, so a stall shows up as latency instead of
disappearing into a slowed-down request rate. A longer `--hours` window is a
stricter demonstration and persists like any measured run; nothing shorter than
the case exists.

`--root` is the performance schedule, `perf/` in this repository: the class
cases, the journey catalogue and the payloads the journeys send. `stress`,
`aql-probe` and `perf-assets` read the same tree, and none of them reads
`catalogue/`.

`--results` names the measured record. When no file exists there, `perf`
creates it: before it seeds anything, it reads the server's identity through
System OPTIONS, and `--sut-name` and `--sut-version` supply the name and
version the server does not state. The new record holds the measurements and
no conformance outcome. No party statement selected the run, so it is
`statement_blind` with the defaulted technology profile, and its verification
pack is `not_run`. When the file exists, `perf` keeps its header, replaces any
earlier record for the same case and adds the new one. `verdicts` and
`perf-assets` read either kind of record.

The `environment` block is mandatory rather than optional because a latency
number is a claim about a deployment, and a claim with no deployment described
cannot be checked or reproduced. Every member of it is mandatory here
(`hardware_class`, `cores`, `memory_gb`, `storage_class`, `topology`), and a
run whose IXIT lacks one stops before it sends a request, naming the member.
A conformance run reads only `exclusive_server`, so a conformance-only IXIT
may leave the others out.

## stress

```text
Run the step-load STRESS instrument: geometric load steps to the maximum sustainable throughput — where the system breaks (exploration only; never a conformance record, and class-free by design)

Usage: veredictum stress [OPTIONS] --root <ROOT> --ixit <IXIT> --out <OUT>

Options:
      --root <ROOT>                  The performance schedule root, `perf/` in this repository (classes/, journeys.yaml, payloads.yaml, payloads/)
      --ixit <IXIT>                  The ixit topology file (JSON) — its environment block is mandatory (a throughput number without the deployment described is meaningless)
      --out <OUT>                    Where to write the stress report (stress.json)
      --corpus-class <CORPUS_CLASS>  The class-scale corpus the stress runs on (POC | S | L | R — the standardized corpus selector): data volume + workload mix only; no class floor enters the stress report or chart [default: POC]
      --seed-workers <SEED_WORKERS>  Parallel seeding workers [default: 16]
      --step-secs <STEP_SECS>        Each load step's recorded hold, seconds (short + intense by design) [default: 120]
      --bisections <BISECTIONS>      Post-breach bisection refinements [default: 3]
      --max-rate <MAX_RATE>          The climb cap (arrivals/s) [default: 4096]
  -h, --help                         Print help
```

Exploration only, and class-free by design: no class floor enters the stress
report or its chart. A `stress.json` is never a conformance record, and quoting
one as if it were is a misuse of the tool.

## bench

```text
Run the universal SPEED benchmark: an embedded pack against any reachable CDR over its base URL, seeded once and measured N times open-loop (comparative speed only; never a conformance record, a certificate, or a performance-class rating)

Usage: veredictum bench [OPTIONS] --base-url <BASE_URL> --out <OUT>

Options:
      --base-url <BASE_URL>
          The system's base URL, up to and including the openEHR REST base (for example `https://cdr.example/openehr/v1`)

      --auth <AUTH>
          How the client presents itself: `none`, `basic` or `bearer`.
          
          The secret never rides argv: `basic` reads `VEREDICTUM_BENCH_PASSWORD` and `bearer` reads `VEREDICTUM_BENCH_TOKEN`.
          
          [default: none]

      --user <USER>
          The user `--auth basic` presents

      --pack <PACK>
          The embedded pack to drive
          
          [default: smoke]

      --posture <POSTURE>
          The posture profile to declare, out of the set the pack defines. Omit to take the pack's first, which is always `minimal`.
          
          The run's canaries check the declaration against the running system before and after the measured window, and a contradiction refuses the run.

      --repetitions <REPETITIONS>
          How many times to repeat the measured phases. A result with fewer than three is recorded as not submittable, and names that as one of its unmet requirements
          
          [default: 3]

      --scale <SCALE>
          Multiply the pack's EHR count by this factor, for a shorter run. Anything but `1.0` takes the run off the pack's pinned configuration, and the record says so
          
          [default: 1]

      --seed-workers <SEED_WORKERS>
          Override the worker count every seed phase declares. Omit to run the pack's own value, which is what its reference figures describe

      --with-baselines
          After the target's run, compose each pinned reference CDR on this host from its own digest-pinned images, drive the same pack at the same seed against it, and record the relative index. Needs the docker CLI; a record without a baseline is not submittable

      --out <OUT>
          Output directory for the result document and its summary

      --label <LABEL>
          A label for this run, which names its column in a comparison and distinguishes its file name in a shared output directory

  -h, --help
          Print help (see a summary with '-h')
```

A bench run needs no artifact root, no IXIT and no party statement. The pack is
compiled into the binary with a sha256 pin on every fixture, and the pins are
recorded in the result.

### The embedded packs

`smoke` proves the engine: one blood-pressure template, a small EHR corpus, and
one mixed open-loop phase over the read, write and query surface.

`community-vitals` reproduces the openEHR community's vital-signs benchmark
harness ([thread 17224](https://discourse.openehr.org/t/17224)) and measures the
same work a second way. Its write phase creates 100 EHRs and commits the same
Vital signs composition 1,000 times into each with `Prefer: return=identifier`,
on one worker, reporting bulk-load throughput plus the whole-loop
milliseconds-per-composition average the thread quotes. Its read phase then runs
twice over that population. `read_walk` is the sequential walk the harness
performs, seven GETs against every committed composition: the latest version,
the same at an instant, the `VERSIONED_COMPOSITION`, its latest version, its
version at that instant, one version by id, and the revision history. It reports
the whole-loop microseconds-per-request average. `read_open_loop` offers the same
seven reads as an arrival schedule pinned at 200/s for 60s after a 15s warmup,
which is where the coordinated-omission-free percentiles come from. The pinned
rate is part of the pack version.

Every number in the record carries the discipline that produced it: a
`closed-loop` whole-loop average and an `open-loop` percentile answer different
questions and are never read against one another. The two fixtures are embedded
byte-identically, the operational template from the vendored CKM export for
template id `Vital signs` and the composition from the attachment on post 8 of
that thread, both pinned by sha256 and verified at load.

`aql-mix` measures query speed over that same population, from the same two
pinned fixtures, so a query figure and a read figure describe the same corpus.
Its seed phase creates 50 EHRs and commits the composition 20 times into each,
on a pool of 8 workers. The pack version pins that population, and it is sized
for query shapes: large enough that a query has to choose an access path, small
enough to load before a measured window opens. The measured phase is open-loop
at 24 arrivals a second for 60s after a 15s warmup, over six query classes at
equal share, so each class is offered at 4 arrivals a second and every class
returns the same number of samples.

| Class | What it probes |
|---|---|
| `adhoc_query_point_lookup` | The indexed-read floor: one composition addressed by its own uid inside one EHR, the cheapest query a server can answer |
| `adhoc_query_ehr_scan` | The loaded-database shape: every composition in one EHR projected by uid, so the cost follows how much that EHR holds |
| `adhoc_query_filtered` | The value index: a systolic magnitude threshold over the observation leaves of one EHR |
| `adhoc_query_population` | The cross-EHR planner: the same threshold with no EHR scope and a `fetch` bound, so the server picks an access path over the whole population |
| `adhoc_query_aggregate` | The columnar shape: one `COUNT` over the population that threshold matches, which returns a single row and reads every value behind it |
| `adhoc_query_ordered_page` | Sorting and pagination: an `ORDER BY` over composition start time read through a moving `fetch` window |

Each class posts its own AQL statement to `/query/aql`, accepts only `200`, and
counts every other answer in its own error class, so a server that refuses one
shape leaves the other five classes' percentiles alone. The systolic threshold,
the page offset, and the EHR or composition each arrival addresses all draw
from the run's seeded streams, so no arrival repeats the previous one's result
set and the whole draw is reproducible from the seed the record discloses. Why
each class exists travels with the pack definition, so a rendered view explains
a column without knowing anything about the pack's internals.

`--scale` shrinks the EHR count for a quick run and changes nothing else.
Anything but `1.0`, or a `--seed-workers` override, takes the run off the pack's
pinned configuration; the record says so in its `scale` block, the summary says
so in prose, and `bench-compare` names it in the header.

Credentials never ride the command line: `--auth basic` reads its password from
`VEREDICTUM_BENCH_PASSWORD` and `--auth bearer` reads its token from
`VEREDICTUM_BENCH_TOKEN`.

Before anything is measured, a preflight reads the template list, uploads the
pack's template, then creates one scratch EHR, commits a composition into it and
reads that composition back. A failure at any of those refuses the run and names
the exchange, so a half-measured document never exists.

The run then seeds its corpus once and repeats the measured phases. Measured
phases are open-loop: arrivals fire at their planned instants whatever the
system is doing, and every latency is measured from the planned instant, so a
stall shows up in every arrival queued behind it.

### Posture profiles

A CDR running without an audit trail and unsigned versions is a different
system from the same CDR with both switched on, and a speed number that does
not say which one it measured cannot be compared with anything. Every pack
therefore defines named posture profiles, and a run declares exactly one.

`minimal` is the bare spec-conformant surface, and every pack defines it:
no audit trail, unsigned versions, commits validated against the operational
template, uncompressed responses, one tenant. Validation sits at `template`
rather than at nothing because the specification puts it there: ITS-REST
`specifications/responses/422.yaml` defines the commit refusal as the case
where the template "is not validating the supplied resource". A server that
accepts anything is below the floor rather than lightly configured.
`community-vitals` also defines `clinical-default`, which is the same surface
with an audit trail written.

The record's `posture` block carries the profile, its summary, and one line per
disclosed item: the audit sink, the version-signing scheme, the
commit-validation depth, the authentication mode, TLS, response compression and
tenancy. Each item is a closed vocabulary, so an unknown token is refused rather
than read as a default.

### Posture canaries

A declaration is a promise, so each item is also probed black-box and labelled
`verified` or `declared-only`, with the exchange behind the label recorded
beside it.

| Item | How it is checked |
|---|---|
| `version_signing` | Versions committed by the run's OWN seed traffic are read back and their `signature` inspected. Sampling the measured population means a scheme switched on around a probe never reaches it. The openPGP armor header separates `pgp` from `digest` |
| `commit_validation` | The pack's pinned invalid twin (that pack's own composition with the mandatory `COMPOSITION.composer` removed) is committed inside the run window, and the answer read |
| `authn` | One read with no `Authorization` header at all, which is the only way to see whether the declared mode is enforced |
| `compression` | One read stating `Accept-Encoding` explicitly, over a client that does not decompress, so `Content-Encoding` survives to be read |
| `tls` | The recorded base URL's own scheme |
| `audit`, `tenancy` | Declared-only. Released ITS-REST surfaces no read resource for either, so the record carries the claim and says it is one |

The canaries run before the measured window and again after it. A reading that
contradicts the declaration refuses the run, naming the item, the declared
value, the observed one and the exchange. So does a pair of brackets that
disagree with each other, because a posture that moved mid-run leaves the
numbers straddling two systems. Neither is recorded as a footnote beside a
published figure.

Baselines run under the profile the target declared and carry their own
verified block, so both sides of a ratio disclose what was switched on behind
them. `bench-compare` states a posture disagreement between columns in the
header, above the numbers.

### Same-machine baselines

`--with-baselines` anchors the run. After the target's own measurement, the
instrument composes each pinned reference CDR on this host, drives the same
pack at the same seed for the same number of repetitions against it, and tears
the stack down with its volumes so the next baseline starts from an empty
database. The record then carries a baseline block per reference, each a full
per-operation summary exactly like the target's, beside the digest-pinned
images, the upstream deployment recipe the topology follows, and the container
ceilings both stacks ran under. Every image is pinned by digest rather than by
tag, so two submitters measure the same bytes.

The flag needs the `docker` CLI. On a host where it does not answer the run is
refused before the target is touched, with the missing binary named; a run
without the flag needs no container runtime at all.

### The relative index

An absolute millisecond describes the system and the machine together, so two
records taken on different hosts cannot be read against one another. The
relative index can. For every phase, operation and metric it is the target's
cross-repetition median divided by the baseline's, both measured on the same
host in the same session, so the machine cancels. On a latency metric a value
below `1.0` means the target answered faster than the baseline and above `1.0`
slower; on throughput the sense inverts, because there a larger number is the
faster system. Each ratio is serialized with the two medians it came from.

Where no ratio can be formed the record says so and why: an operation only one
side measured, a phase only one side ran, or a baseline median of zero. A gap
is recorded rather than omitted, because a missing row in a comparison reads as
agreement.

### Submittability

A record is submittable when it carries at least three repetitions, at least one
same-machine baseline, and no operation that lost more of its arrivals than the
pack's ceiling allows. That ceiling is part of the versioned pack definition and
the record discloses it as `pack.max_failed_share`; every embedded pack pins
`0.01`. The check reads every repetition, phase and operation, on the target and
on every baseline block, because an index divides by a baseline median and a
divisor taken from arrivals that failed describes the failure. A record that
misses any requirement stays valid for local exploration and names the ones it
misses, in the `submittable_unmet` list, in the rendered summary and in every
`bench-compare` column header.

The rendered summary carries a failed-arrival table: one row per side,
repetition and phase, with the phase's own share and the worst operation inside
it, followed by a sentence per reading above the ceiling naming where it went.
The environment fingerprint prints on the summary header and in every comparison
column, so no number is read without the machine it came from.

A bench result is a benchmark record for comparative speed. It is not a
conformance record, not a certificate, and not a performance-class rating; a
bench result may motivate a class run, never substitute for one.

## bench-compare

```text
Align two or more committed bench results into one table, one column per file, with every pack or host mismatch stated in the header

Usage: veredictum bench-compare --result <RESULTS> --out <OUT>

Options:
      --result <RESULTS>  A committed bench result. Repeat the flag once per file; at least two are needed
      --out <OUT>         Output directory for the rendered comparison
  -h, --help              Print help
```

One column per file, one row per phase, operation and metric. Each cell carries
the cross-repetition median with the inter-quartile range beside it, so a reader
sees the spread as well as the number.

Every column header carries the machine the run was generated on, the worst
failed-arrival share the run recorded beside the ceiling its pack pins, and a
column that is not submittable names the requirements it misses rather than
printing one bare `false`. Where the columns carry a relative index, it gets its
own table below the header, at `p50` and `p99` per baseline: that is the part
which survives a change of host.

A mismatch is stated above the table, never under it: columns that ran different
pack versions, columns generated from different hosts, columns that ran at
different scale factors or off the pack's pinned configuration, and columns whose
runs are not submittable are all named in the header, and the command exits `1`
when any of them applies. Columns from different hosts where at least one
carries no relative index are named too, because nothing in that table is
comparable across them. Each row also names the discipline its numbers came
from.

## bench-packs

```text
Write the embedded benchmark pack manifest: every pack's phases with their load discipline, its operation mix with what each entry probes, its posture profiles, its fixture pins with their provenance, its seed, and the submission requirements (byte-deterministic)

Usage: veredictum bench-packs --out <OUT>

Options:
      --out <OUT>  Output directory (created if missing)
  -h, --help       Print help
```

A pack is versioned data compiled into the binary, so the binary is the only
honest source for a description of one. This command writes that description:
per pack the id, the version, the seed every arrival stream draws from, the
failed-arrival ceiling a record is judged against, each phase with its load
discipline and its counts, each measured phase's operation
mix with the share and the probe rationale of every entry, each posture profile
the pack defines with what it declares item by item, and each embedded fixture
with its sha256 pin, its size and where the bytes came from. The document also
carries the boundary statement, the methodology, how a relative index is
derived, what the seed and the posture canaries govern, and the requirements a
record meets before it may be ranked.

Emission is byte-deterministic and every collection is ordered, so regenerating
the file and diffing it is a build gate. The public page at
[veredictum.eu/benchmark-methodology.html](https://veredictum.eu/benchmark-methodology.html)
is generated from the committed copy of this document, and CI refuses a pack
change that leaves either of them stale.

## aql-probe

```text
Run the AQL optimization probe: fire the measurement machinery's AQL vocabulary against a live, freshly seeded SUT, record wire percentiles, and attribute the DB-side cost per probe via `pg_stat_statements` (exploration evidence for the optimization loop — never a conformance record)

Usage: veredictum aql-probe [OPTIONS] --root <ROOT> --ixit <IXIT> --out <OUT>

Options:
      --root <ROOT>                  The performance schedule root, `perf/` in this repository (classes/, journeys.yaml, payloads.yaml, payloads/)
      --ixit <IXIT>                  The ixit topology file (JSON) — the `containers` block enables DB-side attribution and maintenance settling
      --out <OUT>                    Where to write the probe report (aql-probe.json)
      --corpus-class <CORPUS_CLASS>  The class-scale corpus the probes run against (POC | S | L | R) [default: POC]
      --seed-workers <SEED_WORKERS>  Parallel seeding workers [default: 16]
      --requests <REQUESTS>          Requests fired per probe [default: 20]
  -h, --help                         Print help
```

Fires the measurement machinery's AQL vocabulary, records wire percentiles per
probe, and attributes the database-side cost through `pg_stat_statements`. This
is evidence for someone optimizing a server, and it is never a conformance
record.

## stress-compare

```text
Render the cross-SUT stress overlay (both systems' latency-throughput curves on one canvas) FROM two committed stress reports — deterministic, both directions on equal footing

Usage: veredictum stress-compare --left <LEFT> --left-label <LEFT_LABEL> --right <RIGHT> --right-label <RIGHT_LABEL> --out <OUT>

Options:
      --left <LEFT>                The primary SUT's committed stress.json
      --left-label <LEFT_LABEL>    The primary SUT's display label
      --right <RIGHT>              The comparison SUT's committed stress.json
      --right-label <RIGHT_LABEL>  The comparison SUT's display label
      --out <OUT>                  Where to write the overlay SVG
  -h, --help                       Print help
```

Deterministic, and both directions on equal footing: the two curves are drawn by
the same code from the same kind of file, so neither side gets a rendering
advantage.

## perf-assets

```text
Render the published performance SVG assets FROM a committed results.json (deterministic; CI regenerates and diffs — hand-drawn numbers are a build failure)

Usage: veredictum perf-assets [OPTIONS] --root <ROOT> --results <RESULTS> --out <OUT>

Options:
      --root <ROOT>        The performance schedule root, `perf/` in this repository (for the class-ladder floors)
      --results <RESULTS>  The committed results.json carrying the measurement records
      --out <OUT>          Output directory for the SVG files
      --summary <SUMMARY>  Also write the generated Markdown summary (class ladder + measured detail) to this path — the book's build-time include
      --stress <STRESS>    A committed stress report (stress.json) to render the latency-throughput curve from, when one exists
  -h, --help               Print help
```

## verdicts

```text
Re-derive the measured performance-class verdicts from a statement and a results record against the performance schedule (the pure pipeline), and write them as `verdicts.json`

Usage: veredictum verdicts --statement <STATEMENT> --results <RESULTS> --root <ROOT> --out <OUT>

Options:
      --statement <STATEMENT>  The party statement (`statement.json`)
      --results <RESULTS>      The party results (`results.json`)
      --root <ROOT>            The performance schedule root, `perf/` in this repository (classes/, journeys.yaml, payloads.yaml, payloads/)
      --out <OUT>              Output directory for verdicts.json
  -h, --help                   Print help
```

The pure step for a measured campaign. It reaches no network and reads nothing
but its inputs, which is what makes a published verdict re-derivable by anyone
who has the same three: the statement, the results record carrying the
measurements, and the performance schedule under `perf/`.

Each measured class verdict is re-derived from the measurement record's
embedded histograms against its performance case, and a stored verdict that
does not re-derive is a review finding. A claimed class with no measurement,
or a claim with no environment reference, is a review finding too. The command
writes `verdicts.json` and exits `1` when the review found anything.

Conformance verdicts over the Arazzo catalogue are `veredictum test`'s results
document and its exit status.

## export

```text
Write the views of an Arazzo results document a vendor's CI shows: JUnit XML, CTRF 0.2.0 JSON and a Markdown summary, each a byte-deterministic function of the document

Usage: veredictum export [OPTIONS] --results <RESULTS>

Options:
      --results <RESULTS>            The `results.json` the Arazzo evaluator wrote
      --junit <JUNIT>                Write the JUnit XML view here
      --ctrf <CTRF>                  Write the CTRF JSON view here
      --markdown <MARKDOWN>          Write the Markdown summary here. Without this flag and without a job summary file, the summary is printed to standard output
      --step-summary <STEP_SUMMARY>  The GitHub Actions job summary file the Markdown summary is appended to. GitHub sets the variable in every job step [env: GITHUB_STEP_SUMMARY]
  -h, --help                         Print help
```

`export` reads the `results.json` the Arazzo evaluator writes and renders the
three views a CI system shows. Each is a pure function of that document, so
exporting the same results twice writes the same bytes, and none of them
carries a clock reading the results document does not carry.

| Status | JUnit XML | CTRF 0.2.0 |
|---|---|---|
| passed | a bare `<testcase>` | `passed` |
| failed | `<failure>` | `failed` |
| errored | `<error>` | `other`, with `rawStatus: errored` |
| skipped | `<skipped>` | `skipped` |

A workflow is a JUnit `<testsuite>` and a row is a `<testcase>` named
`<case> row <n>`. The text of a `<failure>` or `<error>` is the row's
four-part explanation, its `message` is a one-line summary, and its `type`
says whose problem the row is (`server`, `advisory` or `instrument`). A
`<skipped>` carries the skip reason and the citation that makes the case not
apply. In CTRF, a red row's `message` is the same four-part explanation, and
each test's `extra` carries the case id, the row, the skip reason, the
citation and, where the citation names one, the link to its upstream report.
The CTRF document validates against the schema the CTRF project publishes for
`specVersion` 0.2.0.

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
appends the summary to the job summary, and fails the job when a row failed or
errored. Its page has a complete workflow.

The JUnit file is read by GitLab (`artifacts:reports:junit`), Jenkins (the
JUnit plugin) and most other CI systems. `conformance/recording.har` holds the
server's response bodies verbatim, so it belongs where the operator keeps the
run, never in a public artifact.

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
