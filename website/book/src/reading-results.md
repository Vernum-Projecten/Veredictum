# Reading a result

<!-- toc -->

This page is for the person deciding whether to buy a CDR, who has a vendor's
conformance result in hand and no openEHR specification open beside it. It
says what each kind of row means for that decision, why the result carries
no severity scale, and what a result can and cannot tell you about the
product.

A result is the `results.json` a run wrote (`schemas/arazzo-results.schema.json`),
or a view of it: [`veredictum show`](commands.md#show), the Markdown summary
of [`export`](commands.md#export), [the console](console.md#the-results) or
the [badges](badges.md). Every view counts the same rows the same way.

## BASE and options

Each row belongs to BASE or to an option, and the views report the two apart,
chapter by chapter (`catalogue/chapters.yaml`).

- **BASE** is what every platform CDR must do over the released openEHR REST
  API: create and read an EHR, commit and read a COMPOSITION, run a query, and
  so on. A case is BASE when no option arm names its document.
- **An option** is a behaviour the released text leaves to the implementation,
  such as serving canonical XML beside JSON. Both answers are conformant.
  `catalogue/capabilities.yaml` names each option, its arms and the sentence
  that leaves the choice open, and the vendor's statement picks one arm. Only
  the cases of the picked arm run. Cases resting on TRIAL text are read with
  the options too.

## The four kinds of row

Every row ends `passed`, `failed`, `errored` or `skipped`, and a failed row
may be a warning. The examples below are row shapes from `results.json`,
shortened to the members that matter here. The case ids and quoted sentences
are real ones from the catalogue; the `what`, `expected` and `received` texts
are abridged, and the failed option row's values are illustrative.

### A failed BASE row

```json
{
  "case": "ehr/create_ehr-same_ehr_twice-with_id",
  "status": "failed",
  "explanation": {
    "step": "create-again",
    "what": "The second create with an ehr_id already in use is refused.",
    "why": {
      "source": "specs/openehr/ITS-REST/specifications/responses/409_EHR_with_id.yaml",
      "quote": "Can happen when the supplied `ehr_id` is already used by an existing EHR."
    },
    "expected": "status 409",
    "received": "status 201",
    "whose": "server"
  }
}
```

The server did something the released specification does not allow, in a
behaviour every platform CDR must have. Here it accepted a second EHR under an
identifier already in use, where the specification assigns 409 Conflict.
`whose: server` is the instrument's attribution: the row counts against the
server. The row carries the exchange it was judged on, so the vendor and you
can see exactly what was sent and what came back.

For a purchase, a failed BASE row is a defect in core behaviour. Ask the
vendor for the fix and a fresh run, or for the reason the row should not
apply, which they must ground in the cited sentence. The chapter the row sits
in tells you which area of the product is affected.

### A failed option row

```json
{
  "case": "ehr_xml/get_ehr-xml",
  "status": "failed",
  "explanation": {
    "step": "read",
    "expected": "status 200, Content-Type application/xml",
    "received": "status 406",
    "whose": "server"
  }
}
```

The vendor's statement picked the arm `ehr-xml-supported`, so the vendor
claimed the server serves an EHR as canonical XML, and the server did not.
The release lets a server serve XML or not (`Resources.md`, Data
representation: "Services MUST support at least one of the openEHR **XML** or
**JSON** canonical formats for resource representation."), so declining XML
is conformant. Claiming XML and then not serving it is not.

For a purchase, a failed option row means the product does not do what its
own statement says it does. It matters exactly as much as that option matters
to you. If you need XML, it is as serious as a BASE failure; if you do not, it
still tells you the statement is inaccurate.

### A warning

```json
{
  "case": "ehr_status_update/update-stale_if_match-etag_latest",
  "status": "failed",
  "explanation": {
    "why": {
      "source": "specs/openehr/ITS-REST/specifications/docs/overview/Requests_and_responses.md",
      "quote": "SHOULD return also latest `version_uid` in the `ETag` response headers.",
      "level": "should"
    },
    "whose": "warning"
  }
}
```

A warning is a failed row whose every failing check quotes a SHOULD. The
released text recommends the behaviour without requiring it, and
RFC 2119, which the ITS-REST preface adopts, lets an implementation ignore a
SHOULD for a valid reason. So a warning is reported with its full explanation
and never counts against the server: it leaves the exit status at `0` and
changes no badge colour. Here the server answered a stale update correctly
with 412 but did not name the latest version in its `ETag`.

For a purchase, a warning is a question to the vendor ("why not?"), and
usually a small one. It is never grounds on its own to call the product
non-conformant.

### A skipped row

```json
{
  "case": "ehr_xml/get_ehr-xml",
  "status": "skipped",
  "skip": {
    "reason": "option-not-selected",
    "why": {
      "source": "specs/openehr/ITS-REST/specifications/docs/overview/Resources.md",
      "section": "Data representation",
      "quote": "Services MUST support at least one of the openEHR **XML** or **JSON** canonical formats for resource representation."
    }
  }
}
```

The row did not run, and it names why. `option-not-selected` means the
vendor's statement picked the other arm of the option, here that the server
does not serve XML. The other reasons are fixed:

| Reason | What it means |
|---|---|
| `not-claimed` | the statement does not claim the capability the case belongs to, for example demographics |
| `option-not-selected` | the statement picked the other arm of the option |
| `format-not-claimed` | the case needs a wire format the statement does not list |
| `version-out-of-range` | the case applies to specification versions the statement does not claim |
| `extension-not-offered` | the case drives an extension route the vendor does not declare |
| `not-declared` | the vendor did not supply a value or login the case needs, such as an administrator account |
| `cited-text-absent` | the specification text the case cites is absent from the profile the run used |

For a purchase, a skipped row is neither good nor bad news about the server.
It is a fact about the claim. A thin claim shows up as many skipped rows, so
read the skipped count and the `not-claimed` capabilities beside the passed
count: a result where everything passed because almost nothing was claimed
proves little.

### An errored row

An errored row is the fifth outcome you may see. The instrument could not
judge it: the connection dropped, or the response could not be read. Unless
the record names the server, an errored row is inconclusive. It counts
against nobody, a run whose only red rows are inconclusive exits `3`, and
the badges show it in orange. Ask for a re-run.

## No severity beyond MUST and SHOULD

The result has two levels: a MUST the server broke is a failure, and a SHOULD
it did not follow is a warning. There is no "critical", "major" or "minor".

The reason is that the released texts grade requirements with RFC 2119
keywords and nothing finer. The ITS-REST preface
(`specs/openehr/ITS-REST/specifications/docs/overview/Preface.md`,
Requirements) adopts them: "The key words "MUST", "MUST NOT", "REQUIRED",
"SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT
RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as
described in BCP 14". Every check in the catalogue states the level of the
sentence it quotes, and `validate` refuses a check whose level does not match
its quote (`docs/ARCHITECTURE.md` §15.9, the level gate). A grade such as "this
failure means the product is not a CDR at all" would rest on a judgement no
released sentence makes, and the instrument only asserts what a released
sentence says ([the method](methodology.md#the-oracle-is-the-released-specification-text)).

What you get instead is the chapter. Failures cluster by area (EHR,
COMPOSITION, Query, Demographic and so on), so you can weigh them by what
your project needs.

## Is this a real CDR?

A result answers part of that question, and it is worth being exact about
which part.

A result can show:

- **The server speaks the released openEHR REST API.** Each passed BASE row
  is one behaviour, done over the wire the way a released sentence requires,
  against the deployment at the `base_url` the result names. Passed rows
  across the EHR, COMPOSITION, CONTRIBUTION and Query chapters mean the core
  of a platform CDR is there and behaves as specified.
- **It refuses what it must refuse.** The catalogue keeps an invalid twin for
  every refusal the specification requires, so a server that accepts anything
  fails those rows ([the method](methodology.md#positive-and-negative-testing-carry-equal-weight)).
- **Which parts are claimed.** The statement names the capabilities, option
  arms, wire formats and specification versions, and every row outside the
  claim is skipped with its reason.
- **Which text it was judged against.** The `specification` member names the
  released or development profile, and only a released result backs a
  released claim.

A result cannot show:

- **That it holds for another version or deployment.** It is one run of one
  product version at one address. A new product version needs a new
  statement and a new run (`docs/ARCHITECTURE.md` §9, validity and
  supersession).
- **That the run was not tuned.** A vendor's own run is a first-party
  attestation, and every badge says `self-run`. The `instrument_status`
  member reads `pre-approval` for every 0.x version, which the openEHR SEC has
  not approved. To check a claim yourself, run the instrument against the
  vendor's deployment with their IXIT ([Running the instrument](running.md)),
  or ask for a witnessed re-run.
- **How fast it is, or how it scales.** Conformance rows carry no timing
  verdict. Measured performance is a separate record that is never a pass or
  a fail ([Measured performance](running.md#measured-performance)).
- **How secure it is.** Security cases check functional behaviour, such as an
  unauthenticated request being refused. Attack resistance and cryptographic
  strength belong to security evaluation, outside conformance
  (`docs/ARCHITECTURE.md` §6.3).
- **Anything the catalogue does not cover.** Coverage of the released
  operations is machine-checked, and every uncovered branch is a recorded gap
  with its tracker issue (`catalogue/cases/surface.yaml`). The instrument
  tests platform CDRs only ([scope](methodology.md#what-the-instrument-does-not-test)).
- **Clinical fitness.** Whether the product suits your clinical workflows,
  your templates and your data is outside any conformance test.
