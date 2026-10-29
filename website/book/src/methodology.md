# The conformance method

<!-- toc -->

A conformance verdict is only worth what its method is worth. This chapter sets
the method out.

## The oracle is the released specification text

The vendored openEHR specification text is the oracle, and it is never a
suspect. Every expectation in the catalogue cites the section it comes from, and
the only thing that can refute an expectation is a better reading of that text.
Not a maintainer's confidence, and not what every other CDR happens to do.

For a functional case the ITS-REST specification binds each operation to the
wire, and the Reference Model, AQL and the other released components define
what the operation carries. Where the ITS-REST prose is silent, the released
OpenAPI bundle shipped with the same release grounds the expectation, cited as
the OpenAPI document and never passed off as prose. It loses on any conflict
with the prose.

Some sources look like authority and are not. The official openEHR Conformance
component never reached a stable release: its Platform Conformance Test Schedule
says which behaviours are worth testing, which is genuinely useful, but it does
not settle what the correct answer is, and in places it contradicts a released
component. Its Robot test suites are in the same position. Where any of those
disagrees with a released component, the released component wins.

## The catalogue's shape: one behaviour, one case

A case is an Arazzo workflow: its steps call the operations of the released
openEHR REST API OpenAPI documents, and every check it makes carries the
released sentence that assigns the expected value, quoted verbatim and
resolved against the vendored text by `validate`. The workflow names the
operation; the sentence it quotes is the authority, never the operation's
OpenAPI fragment alone.

Cases stay small on purpose. One behaviour per case means a red row names one
defect, which is the difference between a report a vendor can act on and a report
they have to reverse-engineer.

## Positive and negative testing carry equal weight

A server that accepts everything passes every positive test. That is why the
catalogue treats a refusal as a first-class behaviour with its own cases.

Every invalid shape is kept as its own refusal workflow, with the defect it
carries and the specification reference that makes it invalid. Each one has a
valid twin. The valid twin proves the server accepts what it must
accept; the invalid twin pins the refusal, so a lenient server fails it. Deleting
an invalid shape would silently narrow the claim, so it never happens.

The negative surface the catalogue covers includes:

- **Content negotiation.** An `Accept` header nothing can satisfy must produce
  406; an unsupported payload media type must produce 415.
- **Identity and state conflicts.** A duplicate identifier, a stale preceding
  version, a missing version precondition, a delete of something already
  deleted.
- **Content validity.** Missing mandatory attributes, empty lists where the
  Reference Model requires at least one member, structurally wrong documents,
  and content committed against the wrong template.
- **Scope negatives.** A format applied where the specification does not define
  it.
- **Authentication.** A sweep over the route table checking that every platform
  route refuses an unauthenticated request.

An invalid payload cannot be authored through a typed model, because a typed
model will not construct it. Those fixtures are therefore raw bytes, which is
also what makes them catch encoder and decoder bugs that a
construct-then-serialize fixture cannot reach.

## When a run goes red

A red row is a statement that the specification, the catalogue and the server do
not all three agree. Which one is wrong is decided before anything is changed.

Two reflexes are both refused, and the second is why this instrument exists
separately from any server:

1. **"The catalogue must be wrong, the server is right."** This is the reflex a
   vendor brings to a failing conformance run. The catalogue answers it by
   construction: an expectation traces to a citation, so it is refuted by a
   better reading of the released text and by nothing else.
2. **"The server must be wrong, the instrument is right."** Veredictum is the
   thing people are being asked to trust, so it is a suspect on every red row,
   ahead of the server. The first live triage attributed 7 of 7 diagnosed
   defects to the instrument and none to the server under test.

Each red row is attributed to exactly one of three suspects:

| Suspect | What it means | What happens |
|---|---|---|
| **The server under test** | The server violates the released specification | The attribution, carrying the reproduced wire exchange and the citation. Nothing changes in the catalogue |
| **The instrument** | The server behaved correctly and the runner misdrove the case or misjudged the response | The runner is fixed. Those rows were inconclusive, never server failures |
| **The catalogue** | The expectation is wrong against the specification | The artifact is fixed, with a new cited source for the corrected expectation |

The derivation per row is the same every time: read what was actually sent and
received, read what the workflow claims to expect and why, then read
the governing released specification text first-hand and derive independently
what a conformant server must return for the exchange that actually happened.
The three-way comparison names the defect.

Two rules keep that honest. A transport fault or a step that could not be
resolved classifies as inconclusive on the instrument's side, never as a server
failure. And an attribution naming the server carries the reproduced exchange
plus the citation with the decisive sentence quoted, because a verdict about
somebody else's product is held to a higher evidence bar than a verdict about
our own.

> [!WARNING]
> A catalogue expectation is never adjusted to match observed server behaviour.
> A server response is evidence in the three-way comparison. It is not the
> reference, and treating it as one is how a conformance suite quietly becomes a
> description of whatever the last server did.

## Where the specification does not say

Sometimes the released text leaves a behaviour undefined, or two released
documents disagree. Inventing an expectation there would fail every conformant
server and teach nobody anything. Hiding it would be worse.

Instead, no workflow asserts it, and it takes one of two forms:

- **An upstream report.** The silence or contradiction is an
  `upstream-report` issue in this repository, with what the released text
  says, a proposed patch to it, and the resolution sought. A citation that
  rests on the defective text names that issue in its `report:` field.
- **An option.** Where the release leaves a behaviour to the implementation,
  for example whether a server serves canonical XML, both answers are
  conformant. `capabilities.yaml` names the option and its arms with the
  sentence that leaves the choice open, the party's statement picks one arm,
  and only the workflows of that arm run.

Three properties make this a transparency mechanism rather than an excuse list:

- **A silence has to be proven.** A claimed silence that the specification
  actually defines is a catalogue defect: the workflow asserting it becomes
  gating.
- **Nothing is absorbed.** A carried divergence always has an outbound report
  attached, and CI refuses a `report:` that does not name an
  `upstream-report` issue.
- **A private resolution is non-conformant.** A harness that quietly decides a
  silence for itself is not implementing this catalogue.

## Verdicts are computed, never asserted

A verdict is a pure function of the party's statement, the recorded results, the
catalogue and the capability matrix. The recording step and the judging step are
separate commands, so the record survives the judgement and anyone holding the
same files can re-derive the same documents.

Measured performance works the same way. A class verdict is re-derived from the
HDR histograms embedded in the record, so the stored summary is tamper-checked
rather than trusted, and latency is measured from the planned arrival instant
under open-loop offered load, which is what stops coordinated omission from
hiding a stall.

## Coverage is a mandate

A green run over a thin catalogue proves nothing, so coverage is machine-checked
rather than asserted. A gate enumerates the released operations, their
response branches and the negotiation headers from the released ITS-REST
OpenAPI documents alone, and fails on any that has neither a covering step nor
a recorded gap naming its tracker issue.

A behaviour the specification defines and the catalogue misses is a gap to
close. Cases are added; they are never
removed to make a run go green.
