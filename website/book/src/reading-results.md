# Reading a result

<!-- toc -->

For a buyer holding a vendor's result: what each kind of row means for a
purchase, and what a result cannot tell you.

A result is the `results.json` a run wrote (`schemas/arazzo-results.schema.json`)
or a view of it: [`veredictum show`](commands.md#show), the Markdown summary
of [`export`](commands.md#export), [the console](console.md#the-results) or
the [badges](badges.md). Every view counts the same rows the same way.

## BASE and options

The views report BASE and options apart, chapter by chapter
(`catalogue/chapters.yaml`).

- **BASE:** what every platform CDR must do over the released openEHR REST
  API, such as create an EHR, commit a COMPOSITION or run a query. A case is
  BASE when no option arm names its document.
- **Option:** a behaviour the release leaves to the implementation, such as
  serving canonical XML; both answers are conformant.
  `catalogue/capabilities.yaml` names each option, its arms and the sentence
  leaving the choice open. The vendor's statement picks an arm, and only its
  cases run. Cases resting on TRIAL text are read with the options.

## The kinds of row

Rows end `passed`, `failed`, `errored` or `skipped`; a failed row may be a
warning. The examples are cut to the members that matter. Case ids and quotes
are real; other texts are abridged, and the failed option row is illustrative.

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

The server accepted a second EHR under an identifier in use, where the
specification assigns 409 Conflict. `whose: server` counts it against the
server, and the row carries the exchange it was judged on.

**For a purchase:** a core defect. Ask for a fix and a fresh run, or a reason
grounded in the cited sentence. The chapter names the affected area.

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

The statement picked `ehr-xml-supported` and the server did not serve XML.
Declining XML is conformant (`Resources.md`, Data representation: "Services
MUST support at least one of the openEHR **XML** or **JSON** canonical formats
for resource representation."); claiming it and not serving it is not.

**For a purchase:** the product does not do what its statement says. As
serious as a BASE failure if you need XML; otherwise, the statement is still
inaccurate.

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

Every failing check quotes a SHOULD, which RFC 2119 (adopted by the ITS-REST
preface) lets an implementation skip for a valid reason. A warning is reported
in full, never counts against the server, keeps exit status `0` and changes no
badge colour. Here the server answered a stale update with 412 but left the
latest version out of its `ETag`.

**For a purchase:** a question for the vendor, never on its own grounds to
call the product non-conformant.

### A skipped row

```json
{
  "case": "ehr_xml/get_ehr-xml",
  "status": "skipped",
  "skip": {
    "reason": "option-not-selected",
    "why": {
      "source": "specs/openehr/ITS-REST/specifications/docs/overview/Resources.md",
      "section": "Data representation"
    }
  }
}
```

The row did not run; it names its reason and the citation that makes the case
not apply.

| Reason | Meaning |
|---|---|
| `not-claimed` | the statement does not claim the case's capability, for example demographics |
| `option-not-selected` | the statement picked the other arm of the option |
| `format-not-claimed` | the case needs a wire format the statement does not list |
| `version-out-of-range` | the case applies to specification versions the statement does not claim |
| `extension-not-offered` | the case drives an extension route the vendor does not declare |
| `not-declared` | the vendor did not supply a value or login the case needs, such as an administrator account |
| `cited-text-absent` | the cited text is absent from the profile the run used |
| `development-only` | the case rests on text only the development snapshot holds, and the run used the released profile |
| `contested-in-development` | the development snapshot keeps the cited text but contradicts it elsewhere, as an upstream report records, and the run used the development profile |

**For a purchase:** a fact about the claim, not the server. Read the skipped
count and the `not-claimed` capabilities beside the passed count: everything
passing because almost nothing was claimed proves little.

### An errored row

The instrument could not judge: the connection dropped, or the response was
unreadable. Unless the record names the server, it is inconclusive and counts
against nobody. A run whose only red rows are inconclusive exits `3`, and the
badges show orange. Ask for a re-run.

## No severity beyond MUST and SHOULD

A broken MUST is a failure, an unmet SHOULD a warning; there is no "critical",
"major" or "minor". The released texts grade with RFC 2119 keywords and
nothing finer. The ITS-REST preface
(`specs/openehr/ITS-REST/specifications/docs/overview/Preface.md`,
Requirements): "The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL
NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and
"OPTIONAL" in this document are to be interpreted as described in BCP 14".
Every check states its quote's level, and `validate` refuses a mismatch
(`docs/ARCHITECTURE.md` §15.9, the level gate). A finer grade would rest on a
judgement no released sentence makes
([the method](methodology.md#the-oracle-is-the-released-specification-text)).
Weigh failures by chapter (EHR, COMPOSITION, Query, Demographic and so on)
against what your project needs.

## Is this a real CDR?

| A result can show | A result cannot show |
|---|---|
| **The API:** each passed BASE row is one behaviour done as a released sentence requires, at the result's `base_url`. Passes across EHR, COMPOSITION, CONTRIBUTION and Query mean the core of a platform CDR is there | **Another version or deployment:** a new product version needs a new statement and run (`docs/ARCHITECTURE.md` §9, validity and supersession) |
| **Refusals:** every required refusal has an invalid twin, so an accept-anything server fails ([the method](methodology.md#positive-and-negative-testing-carry-equal-weight)) | **An untuned run:** a vendor's run is a first-party attestation; every badge says `self-run`, and `instrument_status` reads `pre-approval` for every 0.x version, which the openEHR SEC has not approved. Run it yourself with the vendor's IXIT ([Running the instrument](running.md)) or ask for a witnessed re-run |
| **The claim:** capabilities, option arms, wire formats and specification versions; rows outside it are skipped with a reason | **Speed or scale:** conformance rows carry no timing; measured performance is a separate record, never pass or fail ([Measured performance](running.md#measured-performance)) |
| **The text:** `specification` names the released or development profile; only a released result backs a released claim | **Security:** security cases check behaviour such as refusing an unauthenticated request; attack resistance and cryptographic strength are security evaluation (`docs/ARCHITECTURE.md` §6.3) |
| | **The uncovered:** every uncovered operation branch is a recorded gap with its tracker issue (`catalogue/cases/surface.yaml`); only platform CDRs are in [scope](methodology.md#what-the-instrument-does-not-test) |
| | **Clinical fitness:** suitability for your workflows, templates and data is outside any conformance test |
