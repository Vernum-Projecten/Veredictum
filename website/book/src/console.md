# The web console

The console is the instrument with a browser in front of it. It ships as its
own container image, reads the catalogue and the vendored specification text
from paths you mount, and reaches the engine only through the published
`veredictum` crate: every number it shows is one the command line prints too.

The console runs functional conformance. The measured instruments (`perf`,
`stress`, `aql-probe` and `bench`) are run with the command line, and the
console exposes none of them.

Every screenshot below is captured by the console's own browser journeys
(`scripts/ui-e2e.sh` with `UI_E2E_DOCS_SHOTS=1`), in one 1440×900 browser
window, light and dark. They are refreshed in the pull request that changes the
interface, so what you see here is the interface that shipped.

A capture pass serves the console in capture mode, where the three facts one
run stamps show fixed stand-ins: the run clock reads `00:00`, a record digest
reads as all zeros, and a signing time reads `1970-01-01T00:00:00Z`. Your own
runs show the real values, and a sealed record always carries them: the mode
changes what the screen displays, never what is written, sealed or signed. It
exists so a capture pass over an unchanged interface produces identical
images, which is what makes a screenshot diff mean something.

## The landing

The four counts are the catalogue's own: case cores, operation bindings,
capability-matrix rows, and validate findings. A findings count above zero means the
catalogue itself needs attention before any server is graded.

![The console landing in light mode](console/img/landing-light.png)
![The console landing in dark mode](console/img/landing-dark.png)

## The catalogue explorer

Chapters first, each with the number of cases it carries. The search and page
state live in the URL, so a view is shareable and survives a refresh.

![The chapter list in light mode](console/img/catalogue-light.png)
![The chapter list in dark mode](console/img/catalogue-dark.png)

One chapter lists its cases: the identifier, the kind, and the test purpose.
One behaviour per case, so a red row names one defect.

![A chapter's case listing in light mode](console/img/chapter-light.png)
![A chapter's case listing in dark mode](console/img/chapter-dark.png)

## One case in full

The case detail carries the description, the specification citations the
expectation stands on, the operation bindings that realize it on the wire, and
the corpus fixtures it uses. The citation list is the point: an expectation is
refuted by a better reading of the cited text, and by nothing else.

![A case detail in light mode](console/img/case-light.png)
![A case detail in dark mode](console/img/case-dark.png)

## The run wizard

Grading starts at Connect: the CDR base URL, the display identity, the
authentication choice from the ixit's own vocabulary, and a probe whose
answer renders verbatim before anything continues.

![The connect step in light mode](console/img/connect-light.png)
![The connect step in dark mode](console/img/connect-dark.png)

Scope takes the claim the run grades: the vendor's own statement.json (the
ICS, the document that says which profiles and capabilities are claimed),
pasted into the box. The console offers no declaration of its own to load,
because the claim belongs to whoever makes it: ISO/IEC 9646-7 assigns an ICS
proforma's support and supported-values columns to the supplier of the
implementation, and the proforma itself is the capability matrix this
instrument publishes.

The claim can also be built on the screen. The tier row offers CORE, STANDARD,
OPTIONS and SEC-BASIC with the capabilities the capability matrix puts in each
tier and the number of catalogue cases those capabilities gate, counted through
the same matrix walk the judgement computes each profile verdict from.
Composing writes that claim into the same box, product identity taken from the
Connect step, so the operator reads the exact document the run will be graded
against. Option branches stay undeclared there: only the party running the
server knows which branch it realizes.

The document is held to the published statement schema before anything is
stored, and saving answers with the claim overview (product, claimed tiers,
capability count) beside the selection preview, so the screen says what will run
before anything starts. An empty box is an honest no-claim run. The verdict
later certifies exactly the pasted claim against the recorded evidence.

Scope also decides whether the run keeps its wire. "Record the wire exchanges"
is off by default; ticked, the run writes `transcript.json` beside its
`results.json`, carrying every request and response it drove. The artifact
records a server's response bodies verbatim, so it can hold real patient data
from the deployment being graded: it lands in the operator's own output
directory, the sealed record covers it, and the `authorization` request
header's value is withheld.

![The scope step in light mode](console/img/scope-light.png)

The live screen renders the engine's own progress stream: the case counter,
the elapsed clock, a moving-median estimate labelled as such, and the tail of
the engine's output. When the run finishes the outcome links straight to the
record.

![A finished run on the live screen](console/img/live-light.png)

## The record: results and verdicts

Results reads the finished run's own record, red rows first. A row links to
its detail: the recorded reason beside the case's specification citations,
which is what [the attribution](methodology.md#when-a-run-goes-red) of that row
turns on.

The detail ends with the wire. A run driven with the recording box ticked
shows each exchange as its request and response panes, verbatim, which is
where a triage starts. A run driven without it says so in the same place.

![The results surface with a detail open](console/img/results-light.png)

Verdicts is the same pure function the command line runs, over the same
record: the profile matrix with its coverage bounds printed, and the rendered
documents byte-for-byte.

![The verdicts surface in light mode](console/img/verdicts-light.png)
![The verdicts surface in dark mode](console/img/verdicts-dark.png)

## The export: one signed record

A verdict nobody can check is an opinion. The export step on the verdicts
screen hands the run to the pinned instrument's own `verdicts --sign-key`,
which writes the rendered documents, a digest manifest over them, and a
detached OpenPGP signature over that manifest. The console adds no sealing of
its own; it supplies the key path and reads the result back.

![Preparing the export on the verdicts screen](console/img/verdicts-export-light.png)

Beside the sealed set the console renders three files a party publishes. Each
carries the record digest prefix, so the artwork names the bytes it certifies
rather than being a logo anyone could copy:

- **The seal card** is the brand's certificate master with its three slots
  filled: the product under test, the profile verdict, and the moment the
  verdict was spoken, read back from the signature's own creation time because
  the record carries no wall clock.
- **The badge** is a compact SVG for a README, with the digest prefix in its
  title and the check command in its source. The copy-paste snippets link it
  to the report beside it.
- **The report** is one self-contained HTML file of everything the results and
  verdicts screens show, with the full digest, the signer fingerprint and the
  signing time in its footer.

All three are pure functions of the record: the same bundle in produces the
same bytes out, which is what lets anyone re-render them and compare.

The seal card, the badge and the report sit deliberately outside the manifest.
The manifest covers what the instrument judged; these are renderings about
that judgement, and signing a rendering of a rendering would say nothing extra.

A sealed bundle is ordinary files, so anyone checks it without this console:

```text
veredictum verify-record --record <dir> --key <public-key>
```

`gpg --verify` accepts the manifest's detached signature too, and `sha256sum`
re-derives every digest it names.

Two keys are configuration. `VEREDICTUM_SIGN_KEY` names the armored secret key
the bundle is sealed with, and `VEREDICTUM_VERIFY_KEY` names the public half.
The console asks for both because it will not print a signing time it has not
checked: after sealing, it verifies its own bundle before stating who signed
it and when. With neither mounted the section says so and offers no button.

## Running it in public

The hosted instrument at
[console.veredictum.eu](https://console.veredictum.eu) is this console served
publicly, and a public console has two problems a local one does not.

**It drives whatever endpoint a visitor names.** `VEREDICTUM_POSTURE=hosted`
turns on the target guard: loopback, private, link-local, unique-local,
unspecified, multicast, carrier-shared and broadcast addresses are refused in
both families before any socket opens, and the name is resolved first, because
a name under a visitor's control resolving to a private address is the whole
problem. The variable's default is `local`, which refuses nothing — driving a
CDR at `localhost` is the normal local case. A value that is neither refuses
to start, rather than falling back to the permissive one on a typo.

**It has to tell its visitors apart.** With no login, that is the peer address.
Behind a proxy the peer is the proxy, so `VEREDICTUM_CLIENT_IP_HEADER` names
the header carrying the real client address — and only a header the operator
names is read, because an unconditionally trusted `X-Forwarded-For` would let
any visitor claim any identity and defeat the per-visitor caps. Unset, the
socket peer is the whole answer.

Both are documented with the rest of the hosted posture in the repository's
`deploy/hosted/` directory.

## An address the console does not serve

A path outside the route tree answers `404` and renders it: the same chrome,
the path that missed named back to you, and two ways on. Nothing was run and
nothing was judged, which is what the page says.

![The 404 page in light mode](console/img/not-found-light.png)
![The 404 page in dark mode](console/img/not-found-dark.png)

## Two real servers, side by side

The same catalogue, the same wizard and the same judgement, driven against two
live CDRs pulled at their latest published images: FerroEHR's quickstart and
EHRbase's official pairing. The run behind each column is the EHR-service case
family, and every difference below traces to a case id and its citation.

| FerroEHR (latest) | EHRbase (latest) |
|---|---|
| ![FerroEHR results](console/img/results-ferroehr-light.png) | ![EHRbase results](console/img/results-ehrbase-light.png) |
| ![FerroEHR verdicts](console/img/verdicts-ferroehr-light.png) | ![EHRbase verdicts](console/img/verdicts-ehrbase-light.png) |

These captures come from the same E2E harness that gates the console
(`UI_E2E_REAL_SUTS=1 scripts/ui-e2e.sh`): the browser drives the real wizard
against the real servers, and the book shows what it photographed.
