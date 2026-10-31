# The conformance method

<!-- toc -->

A conformance verdict is only worth what its method is worth. This chapter sets
the method out.

## Why an independent instrument

A vendor's own test suite cannot answer the question a hospital procurement is
asking. The suite and the server come from the same people, built on the same
reading of the specification, and when the two disagree it is usually the suite
that gets adjusted.

Veredictum is built so that adjustment has nowhere to happen. The released
specifications are the only authority it accepts: every expectation in the
catalogue names the section it comes from, so it can be refuted by a better
reading of that text and by nothing else. No server's behaviour, no vendor's
documentation, and no stalled upstream test suite ever sets an expected value.
Where the released text is genuinely silent or contradicts itself, no case
asserts the behaviour: the gap is an `upstream-report` issue in the repository,
with a proposed patch to the specification, or an option the party's statement
answers ([below](#where-the-specification-does-not-say)). A private resolution
never happens. Every failure is attributed before anything is changed, and the
instrument itself is a suspect ahead of the server
([below](#when-a-run-goes-red)).

## What the instrument does not test

Veredictum tests platform CDRs over ITS-REST: servers that hold openEHR EHRs
and demographic records and expose them through the released openEHR REST API.
Every case is an Arazzo workflow whose steps call the operations of the
released ITS-REST OpenAPI documents (`docs/ARCHITECTURE.md` §15.2), and the
capability folders under `catalogue/cases/` are the parts of that API a
statement can claim. A product the instrument can test is one it can drive
over that wire.

Some products in the openEHR ecosystem are out of scope, each for a stated
reason:

- **Integration brokers and middleware.** A broker moves and transforms data
  between systems. It exposes no openEHR platform service of its own, and no
  released openEHR specification defines what a broker must do on the wire,
  so there is nothing for a case to cite.
- **Analytics platforms.** The instrument tests AQL where the released text
  places it: the Query API of a platform CDR, against data the run committed
  (the `query` capability). A separate analytics product that reads openEHR
  data has no released wire contract of its own to test.
- **Clinical applications.** A form, a dashboard or a patient app is a
  client of a CDR. The instrument is itself a client and judges servers;
  judging a client needs a test server that grades the requests it receives,
  which is a different instrument.
- **CKM and other knowledge repositories.** The Clinical Knowledge Manager
  curates archetypes and templates through its own API, which is no part of
  the released openEHR specifications. The Definition API of a CDR, which
  stores templates for the CDR's own use, is tested (the `definition`
  capability).
- **Conformance of archetypes and templates as artefacts.** Whether an
  archetype is valid ADL or a template a valid OPT is a property of the model
  file, judged by modelling tools against the AM specifications. The
  instrument uploads templates as test material and judges how the CDR
  handles them and the data committed against them; it never grades a
  template itself.

A platform that offers the openEHR services over another protocol than REST
is out of scope as well, until a second ITS has its own Arazzo documents
(`docs/ARCHITECTURE.md` §15.2). [Reading a result](reading-results.md) lists
what a result can and cannot say about the products that are in scope.

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

## Two specification profiles

A run reads its citations against one of two profiles.

- **`development`:** the default run profile and the latest text. It is a
  pinned snapshot of the `master` of each openEHR specification repository,
  the line that carries the newest text,
  vendored under `specs/openehr-development/` with one directory per
  repository and a `PROVENANCE.md` naming the repository, the branch and the
  commit. Every patch this project proposes upstream is written against it.
- **`released`:** selectable and frozen. It is the vendored release under
  `specs/openehr/`, where nothing is ever patched, and it is the oracle of
  every released claim.

A case cites the same section under both profiles, and only the text it is
checked against changes. Where the snapshot rewords a cited sentence and the
requirement is unchanged in substance, the citation carries a `development`
member quoting the snapshot's sentence, which the development profile reads in
place of the released one and the released profile never reads. A development run computes real verdicts against the
development text. A case whose cited file, section or sentence the snapshot
lacks is skipped as `cited-text-absent`, and the skip names the profile and
what is missing. The results, `show`, every export and the console name the
profile and every snapshot commit.

Released results alone back a released claim or a certificate. A development
result is never mixed with, compared as, or counted toward a released result,
a certificate or the released BASE and OPTIONS tally: certificate issuance
refuses a development record, so a certificate needs an explicit
`--profile released` run. `validate` gates the catalogue against the release,
and zero findings stays the only passing result; `validate --profile
development` adds the snapshot's absences as information.

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

A verdict is a pure function of four inputs: the party's statement (the
capabilities the server claims and the option arms it sits on), the IXIT, the
catalogue, and the recorded exchanges. Nothing else enters. Two independent
runners given the same four inputs must compute identical verdicts, and the
verification pack exists to check exactly that. A certificate row a human
typed is a defect. The recording step and the judging step are separate, so
the record survives the judgement and anyone holding the same files can
re-derive the same documents.

Two machineries share that discipline
([`docs/ARCHITECTURE.md`][architecture] §8):

- **Conformance by assertion:** the statement selects the applicable
  documents, the criteria judge each recorded exchange, and each row ends
  passed, failed, errored or skipped. A skipped row names its reason
  (`not-claimed`, `option-not-selected`, `not-declared`), so a thin claim is
  visible instead of silently green, and a red row carries what was checked,
  why, what was expected and what came back.
- **Measured performance:** a declared clinical workload is offered on a
  load ladder up to the knee, and every number of the record is re-derived
  from the HDR histograms embedded in it, so a stored summary is
  tamper-checked rather than trusted. A performance record earns no class and
  enters no conformance verdict.

Load is offered open-loop: arrivals follow a seeded Poisson schedule of
planned instants, and latency is measured from the planned instant rather than
the actual send. A stalled server therefore accumulates the delay it caused,
which is what stops coordinated omission from hiding a stall behind a
slowed-down client.

A record puts its knee in context with a population reading rather than a
concurrent-user guess: about 46 clinical documents per person per year, a busy
hour of about eight times the average rate and 10 to 50 reads per write give
13 to 75 peak API arrivals per second per 100,000 people, with the full
derivation from OECD, Eurostat and NHS activity statistics in
[`docs/ARCHITECTURE.md`][architecture] §8.14. The corpus is seeded at a named
scale, which is a size and never a pass or fail:

| Scale | Corpus |
|---|---|
| POC | 10,000 EHRs |
| S | 100,000 EHRs |
| L | 1,000,000 EHRs |
| R | 10,000,000 EHRs |

## Coverage is a mandate

A green run over a thin catalogue proves nothing, so coverage is machine-checked
rather than asserted. A gate enumerates the released operations, their
response branches and the negotiation headers from the released ITS-REST
OpenAPI documents alone, and fails on any that has neither a covering step nor
a recorded gap naming its tracker issue.

A behaviour the specification defines and the catalogue misses is a gap to
close. Cases are added; they are never
removed to make a run go green.

[architecture]: https://github.com/Vernum-Projecten/Veredictum/blob/main/docs/ARCHITECTURE.md
