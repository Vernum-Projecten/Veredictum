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

**Which text.** `--profile development`, the default, reads every citation
against the pinned snapshot of the openEHR development line under
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
