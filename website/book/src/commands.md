# Command reference

<!-- toc -->

Each code block is the binary's own `veredictum <command> --help`, rendered by
`scripts/render/commands-md.sh`, in the order `veredictum --help` lists them.
The notes under each are hand-written in `website/book/commands/`.

| Command | Does |
|---|---|
| `validate` | checks the catalogue |
| `test` | drives the catalogue, or re-judges its recording |
| `export` | renders a results document as JUnit XML, CTRF and a Markdown summary |
| `perf` | measures a workload against a live server |
| `perf-compare` | compares performance records, or runs two servers interleaved |
| `perf-judge` | re-derives a performance record |
| `verify` | checks a conformance certificate offline |

The rest render or explore.

## emit-schemas

```text
Write the published JSON-Schema set (byte-deterministic)

Usage: veredictum emit-schemas --out <OUT>

Options:
      --out <OUT>  Output directory (created if missing)
  -h, --help       Print help
```

Byte-deterministic. The repository's `schemas/` directory is this command's
output, drift-tested against it. Author against these schemas when writing your
own catalogue or harness.

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
          
          `development` (the default): the pinned `master` of each openEHR specification repository under `specs/openehr-development/`, the line that carries the newest text. A case citing text the snapshot lacks is skipped as `cited-text-absent`, naming what is missing, and a case whose cited text the snapshot contests is skipped as `contested-in-development`, naming its upstream report. The results name the profile and every snapshot commit, and a development result never builds a certificate and is never compared with or counted toward a released result.
          
          `released`: the frozen release under `specs/openehr/`. Only a released result backs a released claim or a certificate. A case resting on development text alone, a citation under `specs/openehr-development/`, is skipped as `development-only`.
          
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

`test` drives the Arazzo catalogue under `catalogue/`. (The removed `run`
subcommand exists under no name.)

**What runs.** Every document under `cases/<capability>/`, in path order.
Setups under `cases/_setup/` are not cases; they run where a case names them
in `dependsOn`, and each workflow runs its own, so nothing set up carries over.
`--document <ID>` narrows the run for re-driving one family during a fix; a
narrowed `results.json` is never the record you submit.

**What runs last.** A workflow whose rows file says `server: exclusive`
changes state it did not create, such as deleting every EHR. It runs only when
the IXIT declares `environment.exclusive_server: true`, in a final phase after
every other workflow. Within a phase, documents keep path order and workflows
document order; `results.json` lists rows in run order, and a re-judgement
follows it. A workflow with no inputs carries the marker in a rows file with no
`rows` and runs once. No openEHR specification governs this order; it is this
instrument's own design.

**The IXIT.** `test` reads `exclusive_server` alone (the other `environment`
members are for `perf` and `perf-compare`) and no `instances`, since
`--base-url` names the server: `{ "environment": { "exclusive_server": true } }`
is a whole IXIT. It must pass the published IXIT schema first, or the run
stops with exit status 2 naming file and violation (a misspelled member, say).

**Which text.** `--profile development` (default) reads citations against the
pinned `master` snapshot under `specs/openehr-development/`; `--profile
released` reads the frozen release under `specs/openehr/`. A case whose cited
file, section or sentence the snapshot lacks is skipped as
`cited-text-absent`, naming the profile and what is missing. A case resting
on development text alone (a citation under `specs/openehr-development/`) runs
under `development` only; `released` skips it as `development-only`.
`results.json` records the profile under `specification`
with the repository, branch and commit of every snapshot directory, and every
view names them. A development result never builds a certificate and is never
compared with or counted toward a released result. The catalogue is gated
against the release under either profile. Re-judge with the recorded run's
profile.

**The claim.** A statement claims at least one capability folder under
`cases/` (`ehr`, `demographic`, `admin`, …) in `claims.capabilities`.

| Statement | Effect |
|---|---|
| folder not claimed | its documents skip as `not-claimed`, citing the statement schema's claims clause; setups a claimed document needs still run |
| `options` | one arm per option in `capabilities.yaml`; other arms' documents skip as `option-not-selected` |
| a name that is no folder, a token that is no arm, two arms of one option | the run stops |
| no statement | every capability runs, no arm selected |

No openEHR specification governs the claim mechanism; it is this instrument's
own design.

```json
{
  "claims": { "capabilities": ["ehr", "ehr_status", "composition", "query"] },
  "options": ["ehr-xml-unsupported", "authentication-required"],
  "spec_versions": { "its_rest": "1.1.0" },
  "tech_profiles": [{ "its": "its-rest", "formats": ["canonical-json", "wt-flat"] }]
}
```

**Versions and formats.** `capabilities.yaml` says under `documents:` what a
document needs:

- **`applies`:** an inclusive `min`, `max` or both (`MAJOR.MINOR.PATCH`) on one
  component's version (`rm`, `base`, `am`, `aql`, `its_rest`, `term`: the keys
  of `spec_versions`), citing the sentence setting the bound. A declared
  version outside it skips the document as `version-out-of-range`. An
  undeclared component decides nothing; a malformed version for a ranged
  component stops the run.
- **`formats`:** wire formats the server must serve or process
  (`canonical-json`, `canonical-xml`, `wt-flat`, `wt-structured`, `wt`). One
  missing from the statement's its-rest profile skips the document as
  `format-not-claimed`, citing the schema's `tech_profiles` clause. No its-rest
  profile claims every format.

Documents pinning a refusal of XML (406 or 415) carry no format and run for
every party, decided by the selected arm. So selecting `ehr-xml-supported`
without claiming `canonical-xml` runs nothing of `ehr_xml`, and
`validate --statement` refuses that arm (`arm-format`) and any claimed folder
left with nothing to run. Every document sending `application/json` needs
`canonical-json`, so a statement claiming only canonical XML runs the
documents that need no JSON.

**Declared inputs.** The IXIT answers a rows file's `declared` inputs. Skipped
as `not-declared`: a workflow with an administrative step without
`--admin-auth`, one sent to the SMART Platform base URL without
`--platform-url`, and a row whose SMART scope set has no token.

**Credentials.** `--auth` and `--admin-auth` take `none`, `basic` or `bearer`,
with the variables [Logging in to the CDR](authentication.md) lists. A secret
comes from the variable or from the file its `_FILE` twin names (setting both
is refused). An empty variable counts as unset, since CI expands a missing
secret to empty. The HAR records `[not recorded]` for every `Authorization`
value, plus whether the administrative principal was supplied, the Platform
base URL and the covered scope sets, so a re-judgement skips the same rows.

**SMART on openEHR.** The `smart` folder; the specification is DEVELOPMENT in
Release-1.1.0, so a statement may leave it unclaimed.

- **`--platform-url`:** SMART serves discovery "relative to the _Platform_
  base URL", which can differ from `--base-url` (for example
  `https://platform.example`). The route is in
  `catalogue/sources/smart-platform.openapi.yaml`, Veredictum's own document
  with root `x-veredictum-base: platform`; no released OpenAPI document
  declares it.
- **Scoped tokens:** a step with `x-veredictum-scope` presents a token whose
  `scope` claim is the named set, usually per row. The instrument never asks
  an Authorization Server; the IXIT's `scoped_tokens` supplies them:

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

  - **`issued`:** scope set to a variable (or its `_FILE`) holding a token
    your Authorization Server issued. Names must be
    `VEREDICTUM_SCOPED_TOKEN_<NAME>`, so an IXIT cannot reach other secrets.
  - **`mint`:** a test issuer your deployment trusts. For a set with no issued
    token, the instrument signs RS256 with `iss`, `sub`, `aud`, `iat`, `exp`,
    the space-delimited `scope` and any `claims`, using the RSA private key in
    `VEREDICTUM_SCOPED_TOKEN_KEY` or `VEREDICTUM_SCOPED_TOKEN_KEY_FILE`. Never
    give it key material a production deployment trusts.

  Scope sets compare as sets (`"openid user/x.r"` equals `"user/x.r openid"`).
  An unset declared variable stops the run before the first request with exit
  status 2. A row neither member covers skips as `not-declared`, citing the
  SMART sentence that has the Platform evaluate scopes when issuing tokens. No
  openEHR specification governs how a test instrument obtains tokens; this is
  the instrument's own design.

The SMART text does not say what a request outside its granted scopes
receives, or which operation a scope grants (upstream report #758), so only
the granted side is tested.

**The server's identity.** `results.json` records under `server` what the
ITS-REST System OPTIONS operation (`OPTIONS <base>/`) returned, classified as
`veredictum identify` prints it. If a workflow sent it, `server.read` is
`exchange` and `server.entry` names the recording entry. Otherwise the run
reads it once after the catalogue with the ordinary credential (`read`, or
`unanswered` without a response), and the recording keeps it for
re-judgement. The stated release is
`server.answer.options.restapi_specs_version`, absent if none. Services SHOULD
answer, so nothing here counts toward conformance. When it differs (as
written) from the statement's `spec_versions.its_rest`, a `review` note of
kind `restapi-version-differs` names both and prints after the row counts,
changing no row or exit status. No openEHR specification governs this
comparison; it is this instrument's own design.

**Re-judging.** `--rejudge <RECORDING>` answers every request from the
recording and reuses every recorded input, so `results.json` comes out
byte-identical. Pass the recorded run's root, statement, IXIT and
`--document` list; a replay that does not line up is refused.

**In the container image.** The entry point passes any argument but
`healthcheck` to the engine, and `VEREDICTUM_CATALOGUE` points `--root` at the
shipped catalogue. The image runs as uid 65532, so the mount must be writable
by it.

```bash
docker run --rm -v "$PWD/out:/work/out" \
    -e VEREDICTUM_CDR_TOKEN \
    ghcr.io/vernum-projecten/veredictum:<tag> \
    test --base-url https://cdr.example/openehr/v1 --auth bearer --out /work/out
```

**Other Arazzo tools** read the documents and can smoke-run them. The verdict
is `test`'s alone: citations, judges, statement-driven selection and
attribution live in the `x-veredictum-*` extensions, which a generic runner
ignores.

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
`OPTIONS` operation. Run it before `test` to see which product and version a
run against that base URL will grade.

- **Exit `0`:** whatever the server answered, since the specification says
  services SHOULD answer and a server may decline.
- **Exit `2`:** no answer arrived, or the credential `--auth` names is not
  supplied. Credentials are read as `test` reads them
  ([Logging in to the CDR](authentication.md)), `_FILE` forms included.

The printed document's `outcome` is `offered`, `not_offered`,
`not_options_document` or `invalid_status`, and
`schemas/identity.schema.json` describes it.

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
          
          `development`: the gates as under `released`, then every case citation read against the snapshot under `specs/openehr-development/` beside `--specs`. A citation whose file, section or quote the snapshot lacks is printed as `[info]` and counted per component; it is never a finding. A citation refused for any other reason is a `profile-citation` finding. One more line counts the development-only workflows, the released cases whose cited text the snapshot rewords or removes, and the released cases whose cited text it contests. Needs `--specs`.
          
          A development-only citation, one under `specs/openehr-development/`, is checked against the snapshot under both profiles, and a bad one is a finding.

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
specifications, the condition profile, requirement levels, unknown keys, rows
files, surface coverage, identity and links, and the AQL parser check. It
prints one line per finding and a summary naming the documents, workflows,
rows files and capability folders read, and exits `1` on any finding.

**`capabilities.yaml` gates.**

- **`floors` (`arazzo-floor`):** each folder's minimum workflow count. Refused:
  a folder without a floor, a floor naming no folder, a folder below its floor.
  The test suite re-derives every floor, so adding workflows raises the floor.
- **`extension-only` (`arazzo-realization-scope`):** the folders verified only
  over Veredictum's own sources (documents under `catalogue/sources/` whose
  `info` carries `x-veredictum-non-openehr: true`, which no openEHR OAS
  describes). Refused: a listed folder with a step calling an unmarked source,
  and an unlisted folder whose every step calls a marked one.
- **`documents`:** refused are an `applies` range or `formats` entry naming no
  document, a range starting after its end, a format the document never sends
  in `Accept` or `Content-Type`, and a document that sends
  `application/json` (itself or through a `dependsOn` setup) without stating
  `canonical-json`.

**Development setups (`arazzo-development-setup`).** A case that rests on
released text and depends, through `dependsOn` at any depth, on a workflow
citing development text alone is refused: the setup would run under the
released profile and the released verdict would rest on development text. A
development-only case may depend on anything.

**`chapters.yaml` (`arazzo-structure`)** is the reading structure every view
follows: it places each capability folder in one chapter and declares each
folder's BASE sections. A document no option arm names is BASE and names its
section with `x-veredictum-section` on its `info`; one an arm names is OPTION
and names none. Refused: a folder in no chapter or two, a BASE document with no
or an unknown section, an unused section, an OPTION document with a section,
an option with no theme and tier, a spec-boundary option whose citation names
no report, and a `reports.yaml` out of step with the reports citations name. A
citation into a document the release marks RETIRED is refused
(`arazzo-citation`).

**`--statement`** adds the static review of one declaration. It must pass the
published statement schema (at least one claimed capability folder), claim
only catalogue folders, and select only arms of options in
`capabilities.yaml`, one per option. Each claimed folder needs a document the
statement lets run: its arm selected, its `applies` ranges containing the
declared versions, its formats claimed. Refused:

- **`claim-completeness`:** a claim nothing can run for.
- **`arm-format`:** a selected arm with a document needing a format the
  statement does not claim, since the run would skip it and nothing would
  verify the arm.

No openEHR specification governs the floors, the realization scope or the
claim gates; they are this instrument's own design.

**`--profile development`** keeps every gate above against the release, then
reads every case citation against the snapshot under
`specs/openehr-development/` beside `--specs`. A citation whose file, section
or sentence the snapshot lacks prints as an `[info]` line naming the case,
followed by per-component counts; neither is a finding, and the exit status
stays `0`. A `profile-citation` finding is a citation the snapshot refuses for
any other reason, a `development` wording the snapshot does not hold, or one
whose released quote still occurs there unchanged. A last line counts the
development-only workflows and the released cases whose cited text the
snapshot rewords or removes. A citation under `specs/openehr-development/`
names development text alone; both profiles check it against the snapshot, and
a bad one is a finding. The repository's own gate
runs `--profile released`, where zero findings is the only pass, and a second
`--profile development` pass that reports the absences.

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
arrivals, so a stall shows up as latency. The run has one shape and no flag
lengthens it: the ladder finds the knee in at most ten minutes after seeding,
and the record states what such a run cannot show.
[Measured performance](running.md#measured-performance) walks a run phase by
phase and explains the record.

- **`--root`:** `perf/` in this repository: workloads, the payloads they send,
  and host prescriptions.
- **`--workload`:** one workload document, embedded or your own (labelled
  `custom`).
- **`--catalogue`:** read only for the capability folders and option arms a
  workload's `requires` tokens name.
- **`--statement`:** drops the interactions your claims do not cover, and
  records the drop.
- **`--corpus-state`:** where the seeded corpus listing is stored (default:
  the record's path with extension `corpus.json`). Seeding runs before the
  clock, closed-loop.
- **`--corpus reuse`:** reuses that corpus; refused before the warm-up when the
  server's read-back counts, the stored digest or the recipe moved.
- **`--posture`:** the posture profile the canaries check after seeding and
  after the last plateau. A contradiction refuses the run.
- **`--host-prescription`:** a prescription under `perf/hosts/`, embedded in
  the record and read against the generator's measured fingerprint.
- **`--network-leg`:** where the generator sat.

The IXIT's `environment` block is mandatory, because a latency number is a
claim about a deployment. Every member is required here (`hardware_class`,
`cores`, `memory_gb`, `storage_class`, `topology`), and a missing one stops the
run before any request, naming it. A conformance run reads only
`exclusive_server`. The optional `containers` block names the server and
database containers and the `database` the server-side readings query.

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
after seeding. A defect is reported against file and field, with exit `1`.

- **`--out`:** writes `perf-workloads.json`, the byte-deterministic manifest of
  the workloads this binary pins by sha256, and `perf-workloads.md`, the page
  generated from it (every interaction, operation, query class and payload
  with its share and source). The book's
  [Performance workloads](perf-workloads.md) page is written with
  `--out website/book/src` and held to the pins by a test. A committed
  workload whose bytes match no pin is refused, so a workload's version and
  pin change together. `perf/payloads.yaml` is held to what the workloads
  read: an unread entry or a pinned payload whose bytes moved is refused.
- **`--workload`:** checks one document of your own. One matching no pinned
  digest is labelled `custom`.

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

Two GitHub-hosted runs of the same code have read up to 156 % apart, so
numbers from separate runs say little about two systems. An interleaved run
puts both sides on one host in one window: each rung of the shared ladder runs
A then B, the next B then A, so host drift lands on both sides.

- **`--aa`:** the same target on both sides, measuring what interleaving
  leaves. Its floor, the largest ratio per metric, is attached to both records
  and turns their `between_run_noise` from `unmeasured` into the floor.
- **`--ab` with `--floor`:** pass an A/A record or its `perf-compare.json`, and
  a ratio must clear the floor as well as `--tolerance` to read as a
  difference.
- **`--result`:** records measured apart were offered different rates. The
  table still lines them up, with ratios shown as `different rates`, unjudged.

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

`export` renders the Arazzo `results.json` as the views a CI system shows.
Each is a pure function of that document, so the same results write the same
bytes, with no clock reading the results do not carry.

| Status | JUnit XML | CTRF 0.2.0 |
|---|---|---|
| passed | a bare `<testcase>` | `passed` |
| failed | `<failure>` | `failed` |
| warning | `<system-out>`, no `<failure>` | `other`, with `rawStatus: warning` |
| errored | `<error>` | `other`, with `rawStatus: errored` |
| skipped | `<skipped>` | `skipped` |

A warning is a SHOULD the server did not follow and never counts against it,
so no view counts it as failed and a CI job does not fail on it. In JUnit it
is the four-part explanation in `<system-out>`, opened by a line naming it a
warning, outside every `failures` count. In CTRF it is `other`, outside
`summary.failed`.

**JUnit XML.** A workflow is a `<testsuite>`; a row is a `<testcase>` named
`<case> row <n>`.

- **`<failure>` and `<error>`:** text is the four-part explanation, `message`
  a one-line summary, `type` whose problem it is (`server` or `instrument`).
- **`<skipped>`:** the skip reason and the citation that makes the case not
  apply.
- **`<properties>`:** on a suite the results place, named
  `eu.veredictum.<name>`: part, chapter, band, capability, section or option
  and arm, formalism, TRIAL mark and upstream reports.
- **`time`:** always `0`, since the results carry no timing.

JUnit XML has no single owner, so the view is held to two published schemas:
the document to the Jenkins xUnit plugin's `junit-10.xsd` (which defines the
`<testsuites>` root), each `<testsuite>` to Apache Maven Surefire's test
report schema. Both are vendored, and a CI gate validates the pinned views
against them with `xmllint`.

**CTRF.** Every test has `type` `conformance`, a `suite` path (the document,
then part, chapter, band and section or option) and `labels` naming where it
sits. A red row's `message` is the four-part explanation. Everything CTRF does
not define sits in `extra` under the key `eu.veredictum`, as CTRF §4.6.1
asks: case id, row, skip reason, citation and, where the citation names one,
the upstream report link. The document validates against the CTRF project's
schema for `specVersion` 0.2.0, its latest release.

**The Markdown summary**, for a person reading it on GitHub:

- A table naming the server and the instrument, and a summary table counting
  BASE and OPTIONS rows.
- `## BASE`, chapter by chapter: each chapter a table of its bands, each band
  a table of its sections.
- `## OPTIONS`, by the same chapters and bands: each band a table of its
  options with their kind (Recommended (SHOULD), Optional (MAY), or
  `@SpecDiverge` for an option that exists only because the released text is
  unclear), the selected arm, and a collapsed note on what the option decides.
  Cases resting on TRIAL text sit in their band here.
- Passed rows are only counted. Each failed, warning or errored row is a
  collapsed block naming outcome, case and place; open, it shows the step, what
  was checked, expected against received, the exchange, whose problem it is,
  the row's inputs as a table, and the cited sentence.
- Warning rows with their citations, then skipped rows counted by reason, with
  the full list collapsed.
- A case resting on text an upstream report records as unclear or
  contradictory is tagged `@SpecDiverge`, and a closing note counts those cases
  and links their reports. JUnit carries the tag as the `eu.veredictum.tag`
  property, CTRF in the test's standard `tags`.

**Where the summary goes.** `export` appends it to `GITHUB_STEP_SUMMARY` when
GitHub Actions sets it, so the job summary shows it with no third-party
action. `--markdown <file>` writes it to a file too. With neither, it goes to
standard output and every other line to standard error, so it can be piped.

**A vendor's GitHub Actions job.** [The GitHub Action](github-action.md) runs
[`test`](#test) and then `export` in one step, with its release's image:

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
appends the summary to the job summary, annotates the rows that need
attention, and fails the job when a row counts against the server. Its page
has a complete workflow, and [GitLab CI](gitlab-ci.md) has a template.

`--annotations <file>` writes those annotations as workflow commands: one
`::error` per failed or errored row and one `::warning` per warning row, at
most 10 of each, then a `::notice` counting the rows the summary lists beyond
them.

GitLab (`artifacts:reports:junit`), Jenkins (the JUnit plugin) and most other
CI systems read the JUnit file. `conformance/recording.har` holds response
bodies verbatim, so it stays with the operator, never in a public artifact.

A `perf-result.json` has one view, the Markdown summary, holding what `show`
prints as tables. `--junit`, `--ctrf` and `--annotations` refuse it with exit
status 2, since a performance record has no test cases.

## badge

```text
Draw the static badges of a result document, for a README.

Each badge is written as `<name>.svg`, a flat badge drawn without a font or the network, and as `<name>.json`, a Shields.io endpoint document (<https://shields.io/badges/endpoint-badge>), and each is a byte-deterministic function of the document. Every badge reports the vendor's own run, and none is a certificate.

Each badge carries one fact: the label names the subject and the message gives one short value. The SVG title and `aria-label` carry the full text.

An Arazzo results document draws four: `base` (the BASE rows), `options` (the rows of the selected option arms and of the cases resting on TRIAL text), `profile` (the specification profile) and `instrument` (`self-run` and the version). A counting badge reads passed out of the rows that bear on the verdict (passed, failed, errored), then the failed and errored counts. A warning row, a SHOULD the server did not follow, changes no colour and shows on no badge; the results carry it. A counting badge of a development result says `dev` in its label, so a development tally never reads as a released one. A counting badge is green only when no row failed or errored: red when a row counts against the server (it failed, or errored with the server or no one named), orange when none does but an errored row is inconclusive, grey when nothing passed. A warning changes no colour.

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

`badge` draws a vendor's static README badges from a result document. Each
badge is a pure function of that document, so the same input writes the same
bytes. Text is measured with a fixed width table, so no font or network is
read.

| Document | Files written |
|---|---|
| an Arazzo `results.json` | `base`, `options`, `profile` and `instrument`, each `.svg` and `.json` |
| a `perf-result.json` | `perf`, `noise` and `instrument`, each `.svg` and `.json` |

```bash
veredictum badge --results conformance/results.json --out conformance/badges
```

The `.json` file is a Shields.io endpoint document, so Shields can serve the
badge from wherever the file is published. Every badge reports the vendor's
own run and is never a certificate. [Badges](badges.md) covers what each says,
the colour rules, and publishing from GitHub, GitLab or any other CI.

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

`show` prints a `results.json` in the terminal, the same view
[`test`](#test) prints when a run finishes.

Each line is one group of rows: a 24-cell bar and the count of each mark.
Every mark with a row gets at least one cell, so one failure among a thousand
passes still shows. Cells differ in shape as well as colour: `█` passed, `▓`
failed, `▒` errored, `░` warning, `·` skipped.

```text
BASE
  Every BASE case                           ████████████▓▓▓▓▓▓▓▓░░░░  ✓   3  ✕   2  ?   0  !   1  ○   0
  EHR                                       ████████████▓▓▓▓▓▓▓▓░░░░  ✓   3  ✕   2  ?   0  !   1  ○   0
    COMPOSITION                             ████████████▓▓▓▓▓▓░░░░░░  ✓   2  ✕   1  ?   0  !   1  ○   0
    DIRECTORY                               no cases
```

- **BASE:** each chapter and band in catalogue order.
- **OPTIONS:** the same chapters and bands, one line per option with the
  selected arm and its kind: Recommended (SHOULD), Optional (MAY), or
  `@SpecDiverge` for an option that exists only because the released text is
  unclear or contradictory. An option with no arm selected is listed once and
  its rows count nowhere. Cases citing text the release marks TRIAL are listed
  in their band here, never in a BASE count.
- **Closing note:** counts the `@SpecDiverge` cases and names the upstream
  reports they are judged by. Report numbers come from the catalogue's
  committed snapshot; nothing is fetched.

Warning rows are counted apart from failures, and `--failed` lists them under
their own heading after the failed and errored rows.

**Colour.** `--color auto`, the default, colours only when standard output is
a terminal and `NO_COLOR` is unset or empty (<https://no-color.org/>).
`--color always` and `--color never` override both, so
`veredictum show results.json --color always | less -R` keeps colour.

**A performance record.** Given a `perf-result.json` (it names its `family`),
`show` prints, within 80 columns and only from the record:

- a header: target, environment, workload and digest, corpus, between-run
  noise and the scope of one run;
- a line per plateau: offered and achieved rates, p50 to max, error share,
  `✓` when it qualifies and `✕` when it fails, `unsteady` where the halves
  disagree;
- the operations at the knee and at the lowest plateau, with each one's
  judged quantile (`n/r`, not resolvable, below 10 samples above it);
- the knee, peak, population and fit sentences;
- a sparkline of p99 against offered rate, and the server-side readings.

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
name and no network, so the published JWK Set is enough: the copy at
`https://console.veredictum.eu/.well-known/jwks.json`, or
`deploy/hosted/jwks.json` in this repository. Any JOSE library verifies the
signature; this command also refuses a key the set marks `revoked_at` and a
certificate issued after its key's `expires_at`, which a generic library
ignores.

A verified certificate prints the server with who supplied each identity
field, the base URL, the catalogue digest, the engine version and its approval
status, the verdict summary, and the SHA-256 digests of the run's
`results.json` and HAR recording. Run `sha256sum` over the two files a vendor
supplied to check they are the ones the certificate covers. The payload's
schema is `schemas/certificate.schema.json`; the key set's is
`schemas/jwks.schema.json`.
