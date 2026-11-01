# The web console

The console is the instrument in a browser, shipped as its own container
image. It reads the catalogue and the vendored specification text from mounted
paths and reaches the engine only through the published `veredictum` crate, so
every number it shows the command line prints too. It runs functional
conformance; `perf` and `perf-compare` stay on the command line.

**The screenshots** come from the console's browser journeys
(`scripts/ui-e2e.sh` with `UI_E2E_DOCS_SHOTS=1`), 1440×900, light and dark,
refreshed in the pull request that changes the interface. Capture mode fixes
the per-run facts on screen (clock `00:00`, run id, fixture server address) so
an unchanged interface gives identical images; what a run writes is
unaffected. The sidebar names engine 0.3.1 in every capture, whatever version
is current, so a release changes no image. Captures read a frozen excerpt (`fixtures/console/catalogue/`:
four capability folders, 166 cases), so their counts are the excerpt's.

## The landing

Four counts: cases (one per Arazzo workflow), Arazzo documents, capability
folders and gate findings. Findings above zero mean the catalogue needs work
before any server is graded.

![The console landing in light mode](console/img/landing-light.png)
![The console landing in dark mode](console/img/landing-dark.png)

## The catalogue explorer

Capability folders with their case counts. Search and page state live in the
URL, so a view can be shared and survives a refresh.

![The capability list in light mode](console/img/catalogue-light.png)
![The capability list in dark mode](console/img/catalogue-dark.png)

A capability lists its cases by Arazzo document, each with its id and summary.

![A capability's case listing in light mode](console/img/chapter-light.png)
![A capability's case listing in dark mode](console/img/chapter-dark.png)

## One case in full

The description, the specification citation, the option arm its document
tests (if any), and every step with its operation. Only a better reading of
the cited text can refute the expectation.

![A case detail in light mode](console/img/case-light.png)
![A case detail in dark mode](console/img/case-dark.png)

## The run wizard

**Connect:** the CDR base URL, the display identity, the authentication
choice from the ixit's vocabulary, and a probe whose answer renders verbatim.
An administrative credential is optional; the run gets it through
`--admin-auth` and `VEREDICTUM_CDR_ADMIN_*`
([Logging in to the CDR](authentication.md)). Without it, workflows with an
administrative step skip as `not-declared` and the IXIT names no
administrative principal. Display name and version stay editable after the
probe; editing the address or a credential withholds Continue until you probe
again.

![The connect step in light mode](console/img/connect-light.png)
![The connect step in dark mode](console/img/connect-dark.png)

**Scope:** paste the vendor's own statement.json (the ICS: claimed
capabilities and selected option arms). The console ships no declaration,
since ISO/IEC 9646-7 assigns an ICS proforma's support and supported-values
columns to the implementation's supplier.

- **Compose on screen:** pick capability folders (each shows its documents and
  cases); the claim is written into the same box with product identity from
  Connect. Options stay undeclared, since only the server's party knows its arm.
- **Save:** checks the published statement schema, then shows the claim
  overview (product, capability count) beside the selection preview.
- **Empty box:** an honest no-claim run; every capability runs, no arm is
  selected.
- **Document-id filter:** runs only documents whose id contains it.

![The scope step in light mode](console/img/scope-light.png)

**Run:** the engine's own `veredictum test`, credentials only in the spawned
process's environment. The live screen shows server and version, the elapsed
clock and the engine's output tail, then links to the results.

![A finished run on the live screen](console/img/live-light.png)

## The results

Failed rows first, then errored rows (the instrument could not judge; the
detail names why). A row's detail is the four-part explanation (what was
checked, why with the cited sentence, expected against received, whose
problem), which [the attribution](methodology.md#when-a-run-goes-red) turns
on; [Reading a result](reading-results.md) explains each kind for a buyer. It
ends with the exchange, verbatim from `recording.har`. That recording can hold
real patient data and stays in the operator's output directory.

![The results surface with a detail open](console/img/results-light.png)

## Certificates, and trial mode

| `VEREDICTUM_CERTIFICATES` | Behaviour |
|---|---|
| `issue` | reads the Ed25519 key in `VEREDICTUM_SIGNING_KEY_FILE` with key id `VEREDICTUM_SIGNING_KID`; refuses to start unless the JWK Set at `/.well-known/jwks.json` publishes it unrevoked and unexpired; writes `certificate.jws` beside each finished run's results, offered as a download |
| unset, `trial` or anything else | trial mode (default): every page's banner says so, results say no certificate is issued, no signing key is read |

[Verifying a certificate](trust.md#verifying-a-certificate) is the reader's
side.

## Running it in public

[console.veredictum.eu](https://console.veredictum.eu) is this console served
publicly. Its settings are documented in `deploy/hosted/`.

| Variable | Effect |
|---|---|
| `VEREDICTUM_POSTURE` | `hosted` resolves the name first, then refuses loopback, private, link-local, unique-local, unspecified, multicast, carrier-shared and broadcast addresses in both families before any socket opens. `local` (default) refuses nothing, for a CDR at `localhost`. Any other value refuses to start |
| `VEREDICTUM_CLIENT_IP_HEADER` | the header carrying the real client address behind a proxy, which the per-visitor caps key on. Only the named header is read, never an unconditional `X-Forwarded-For`. Unset: the socket peer |
| `VEREDICTUM_METRICS_TOKEN` | `/metrics` serves Prometheus text to a bearer of this token: runs by outcome, refusals by guard and reason, server-function answers by status class, the queue, container memory and CPU. Labels are fixed words, never visitor input. Unset: `404`. An unusable token refuses to start |

Each run also writes a JSON line to standard error at engine start and end:
id, outcome, durations and case counts.

## An address the console does not serve

A path outside the route tree answers `404` in the same chrome, names the
missed path and offers two ways on. Nothing was run or judged.

![The 404 page in light mode](console/img/not-found-light.png)
![The 404 page in dark mode](console/img/not-found-dark.png)

## Two real servers, side by side

The catalogue's EHR documents, through the same wizard, against FerroEHR's
quickstart and EHRbase's official pairing at their latest images. Every
difference traces to a case id and its citation. Captured by the harness that
gates the console (`UI_E2E_REAL_SUTS=1 scripts/ui-e2e.sh`).

| FerroEHR (latest) | EHRbase (latest) |
|---|---|
| ![FerroEHR results](console/img/results-ferroehr-light.png) | ![EHRbase results](console/img/results-ehrbase-light.png) |
