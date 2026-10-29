---
paths: ["app/veredictum/src/**", "catalogue/**", "fixtures/**", "schemas/**"]
---

# Red-run triage (the attribution law)

> Ported from FerroEHR's `.claude/rules/cnf-triage.md` at the Veredictum split
> (FerroEHR#2789) and adapted: the paths point at this repository's
> layout, and the first suspect bin is the SUT, which is somebody else's code
> and never edited here.

When a run goes red, the failure is attributed BEFORE anything is changed.
**The vendored released openEHR spec text is always right and is never a
suspect.** Every red row is a defect in exactly one of three bins, two of
which are ours. Run the `cnf-triage` agent, or follow this protocol in
session, for every red row. Never "fix" a red run by guessing which side is
wrong.

**Two reflexes, both banned.** The second one is the reason this product
exists as a separate, independent instrument:

1. **"The catalogue must be wrong, the server is right."** This is the reflex
   a vendor brings to a failing conformance run, and the instrument answers it
   by construction: an expectation traces to a spec citation, so it is refuted
   by a better reading of the released text and by nothing else. Not by a
   maintainer's confidence, not by what every other CDR does.
2. **"The server must be wrong, the instrument is right."** Veredictum is the
   thing people are being asked to trust, so it is a suspect on every red row,
   first-class, ahead of the server. The first live triage run in FerroEHR
   attributed 7 of 7 diagnosed defects to the runner and zero to the server.
   An instrument that presumes itself correct is worth nothing to the people
   who rely on its verdicts.

The reference is the spec, every time. This instrument validates the whole
setup (the SUT AND the runner AND the catalogue) against that spec. It is not
a test suite that presumes its own correctness and questions the server.

## The three suspects (and the only fix path for each)

| Bin | What it means | Fix path |
|---|---|---|
| **SUT** | the server under test violates the spec | the attribution recorded on the pull request or issue that found it, carrying the reproduced wire exchange and the spec citation. NEVER a change in this repository, never a catalogue edit that hides it, and never a report filed outside it (below) |
| **Runner machinery** | the SUT behaved correctly but `app/veredictum/src/**` misdrove the case or misjudged the response (the Arazzo reader, runtime expressions, the evaluator and its judges, selection, verdicts) | fix the runner module; the affected rows were inconclusive, not SUT failures |
| **Catalogue artifact** | the hand-authored catalogue is wrong versus the spec (a workflow, its criteria or citations, a rows file, a payload, `capabilities.yaml`) | edit the artifact WITH a new spec-cited source for the corrected expectation |

## Getting the evidence out of a run (do this first)

A triage with no exchanges cannot attribute anything. `veredictum test --out
<RUN>` writes both halves of the triage input: `<RUN>/results.json`, where
every failed and errored row carries its four-part explanation (what was
checked, why, expected, received) and names the HAR entry it was judged on,
and `<RUN>/recording.har`, which holds every request and response the run
exchanged. Read the row first, then the entry it names.

`veredictum test --rejudge <RUN>/recording.har` re-judges the recording without
a server, given the root, statement, IXIT and `--document` list the run was
given. Use it to check that a runner fix changes the verdict on the recorded
exchange, and nothing else: a replay that does not line up with the recording
is refused rather than judged.

The recording carries response bodies verbatim, so it is operator-controlled
output and belongs where the run's own record is stored, never in a shared
log. Credentials are never recorded (`[not recorded]` stands in for every
`Authorization` value).

## The protocol (per red row)

1. Read the observed wire exchange (the red row in `results.json` and the HAR
   entry it names): what was actually sent, what came back, how it was
   judged.
2. Read the workflow and its rows file (what the catalogue expects, and the
   `x-veredictum-cite` behind each criterion). Note exactly what it claims to
   expect and why.
3. Read the governing RELEASED spec text FIRST-HAND: the ITS-REST docs text
   plus the overview `Requests_and_responses`, and the
   RM / QUERY / BASE / AM / TERM / ITS-XML sections that apply. The CNF
   schedule is only a GUIDE to WHICH behaviour to check. Re-derive the correct
   answer from the released component, never from the schedule's own
   assertion, the OAS, the Robot set, memory, or any CDR's behaviour.
4. Derive independently what a conformant server must return for the exchange
   that actually happened, then compare three-way: spec-required versus
   catalogue-expected versus SUT-observed. The mismatching side is the defect.
5. An attribution that names the SUT carries a reproduced exchange (`curl`
   against the running SUT) plus the spec citation, file and section, with the
   decisive sentence quoted. A verdict about somebody else's product is held
   to a higher evidence bar than a verdict about our own.
6. Spec silence or genuine ambiguity becomes an `upstream-report` issue with
   its patch (below), and no case asserts the silent behaviour. Behaviour the
   release leaves to the implementation is an option in
   `catalogue/capabilities.yaml`. Never a private resolution.

## Hard rules

- **Never edit the vendored spec text, and never adjust a catalogue
  expectation to match observed SUT behaviour.** Expectations trace to spec
  text only.
- **Only the RELEASED spec components are the oracle.** Adjudicate against
  RM / BASE / AM / QUERY / TERM / ITS-XML / ITS-REST **docs text**,
  with one ordered supplement: **the vendored released OAS** is part of the
  release's own specification artifacts (the ITS-REST overview
  `Specifications.md` presents them as its computable artifacts) and grounds
  an expectation **where the docs text is SILENT**. It loses to the docs text
  on any conflict, and an OAS-only ground is always cited AS the OAS, by file
  and element, never passed off as docs text. NEVER treat as authority: the
  CNF Platform Conformance Test Schedule (openEHR CNF never released stable;
  it says which behaviour to test, not the correct answer), or the Robot
  suites and data sets (stalled and in places broken). Where any of these
  conflicts with a released component, the released component wins. An
  "ambiguity" that exists ONLY because a guide source is stale, incomplete,
  or self-contradictory, with no released-component ground, is not a spec gap:
  re-ground it on a released component, or drop it and make the case gating.
  The SM was never released and is no part of the oracle (#604): an operation
  the released ITS-REST does not define has no wire form to test, and no case
  asserts one.
- **A red run is not presumptive evidence of a SUT bug, nor of a runner bug.**
  Every row gets the full derivation. No attribution without a citation.
- Transport faults and step-resolution failures classify as inconclusive
  runner-side rows, never SUT failures.
- **A claimed silence is spec-PROVEN, never assumed.** It is confirmed
  first-hand against the released text before a report is filed, and a report
  is a suspect like any other artifact. A claimed silence the spec actually
  DEFINES is a catalogue defect: make the case gating and do not report it.
- Standing test discipline applies: never weaken a test or an expectation to
  go green (`.claude/rules/testing.md`).

## Upstream reports

An outbound report of a released-spec defect, contradiction, or silence is a
**GitHub issue labeled `upstream-report`**, never a markdown ledger file. One
issue per defect, or per group of closely related defects one patch fixes; a
citation that rests on the defect names it via `report: <number>` in its
`x-veredictum-cite` (CI's `scripts/checks/report-refs.sh` refuses a number that
is not an `upstream-report` issue here); the narrative lives only on the issue.

- **Shape**: a plain opening summary, then `## What the released spec says`
  with citations and quotes, `## What this implementation does`,
  `## Resolution sought upstream`, `## What the patch changes`,
  `## Points for the SEC to decide`. No report exists without its patch under
  `upstream/<openEHR repository>/`, written against the vendored commit and
  checked with `git apply --check`. Never ticket-draft framing, and no
  Channel/Status/Ask fields.
- **Grounding**: docs text first. The released OAS is citable only where the
  docs text is silent, is always cited as the OAS, and loses every conflict. A
  behaviour the OAS DEFINES is not a reportable silence, and a "defect" that
  exists only because a stalled guide source is wrong has no
  released-component ground and is not reportable.
- **Lifecycle — verification is TERMINAL.** A new report is created UNVERIFIED
  and enters the current verification milestone. Once re-verified first-hand
  as genuine it gains `upstream-confirmed`, and once its divergence is fully
  adjudicated here, with nothing further pending on our side, it is CLOSED as
  the standing outbound record: the closed issue stays the durable, linkable
  target of every `report:` that names it, so nothing is silently absorbed
  by closing, and the open set stays near zero by design. A confirmed report
  stays open only while something here is genuinely blocked on it, expressed
  as a native `blocked-by` edge. When it turns out to be a docs misreading it
  closes as refuted, every `report:` naming it is removed, and the affected
  case becomes gating. A proposed fix to the released text is a
  patch under `upstream/<openEHR repository>/`, named after the report. When
  the owner files a report on an openEHR channel, the returned key is
  recorded on the issue without reopening it; when upstream later resolves
  one, file the inbound `spec-update` issue with the closed report as its
  provenance, and reopen only if the resolution requires changes here.
- **Labels**: `upstream-report` plus `spec:<component>`, and
  `upstream-confirmed` once verified. The report's issue type is `Bug`, and
  its priority is the `Priority` issue field, both set with
  `scripts/gh/fields.sh`; neither is a label. A work item here waiting on a reported
  defect adds a native `blocked-by` edge to the report issue.

**Nothing is ever filed outside this repository** (owner ruling 2026-10-23):
no issue, pull request or comment on any openEHR repository or channel, and
none on another vendor's repository, EHRbase included. A server defect in
another vendor's CDR is recorded on the pull request or issue that found it
and goes no further. The single exception is the FerroHEALTH organisation,
for a red row genuinely attributed to FerroEHR, and only with the owner's
go-ahead for that report.
