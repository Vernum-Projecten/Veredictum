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
`cited-text-absent`, naming the profile and what is missing. `results.json` records the profile under `specification`
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
