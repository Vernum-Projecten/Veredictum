# The conformance method

<!-- toc -->

## Why an independent instrument

A vendor's own suite shares the server's reading of the specification, and
when the two disagree the suite usually gets adjusted. Veredictum accepts one
authority, the released specifications: every expectation names its section,
and only a better reading of that text refutes it. No server behaviour, vendor
documentation or stalled upstream suite sets an expected value. Silences get
no case ([below](#where-the-specification-does-not-say)), and every failure is
attributed before anything changes, with the instrument suspected ahead of the
server ([below](#when-a-run-goes-red)).

## What the instrument does not test

Veredictum tests platform CDRs over ITS-REST: servers holding openEHR EHRs and
demographic records behind the released openEHR REST API. Every case is an
Arazzo workflow calling operations of the released ITS-REST OpenAPI documents
(`docs/ARCHITECTURE.md` §15.2); the capability folders under
`catalogue/cases/` are what a statement can claim.

| Out of scope | Why |
|---|---|
| Integration brokers and middleware | no openEHR platform service, and no released text defines a broker's wire behaviour |
| Analytics platforms | AQL is tested where the release places it, a CDR's Query API (the `query` capability); a separate analytics product has no released wire contract |
| Clinical applications | they are CDR clients; judging a client needs a server that grades requests, a different instrument |
| CKM and other knowledge repositories | their API is not in the released specifications; a CDR's Definition API is tested (the `definition` capability) |
| Archetypes and templates as files | ADL and OPT validity is for modelling tools under the AM; templates here are test material, and the CDR's handling of them is what is judged |
| Protocols other than REST | until a second ITS has its own Arazzo documents (`docs/ARCHITECTURE.md` §15.2) |

[Reading a result](reading-results.md) lists what a result can and cannot say.

## The oracle is the released specification text

The vendored openEHR specification text is the oracle and is never a suspect.

- **ITS-REST** binds each operation to the wire; the Reference Model, AQL and
  the other released components define what it carries.
- **The released OpenAPI bundle** grounds an expectation where the ITS-REST
  prose is silent, cited as the OpenAPI document. The prose wins any conflict.
- **Not authority:** the openEHR Conformance component never reached a stable
  release. Its Platform Conformance Test Schedule and Robot suites say what to
  cover, not the correct answer, and the released component wins any
  disagreement.

## Two specification profiles

| Profile | Text | Role |
|---|---|---|
| `development` (default) | pinned snapshot of each openEHR repository's `master` under `specs/openehr-development/`, one directory per repository, `PROVENANCE.md` naming repository, branch and commit | the latest text; every upstream patch is written against it |
| `released` | the frozen release under `specs/openehr/`, never patched | the oracle of every released claim |

A case cites the same section under both. Where the snapshot rewords a
sentence and the requirement is unchanged in substance, the citation's
`development` member quotes the snapshot, read only by that profile. A
development run computes real verdicts. A case whose cited text the snapshot
lacks is skipped as `cited-text-absent`, naming the profile and what is
missing. The results, `show`, every export and the console name the profile
and every snapshot commit.

Released results alone back a released claim or a certificate. A development
result is never mixed with, compared as, or counted toward a released result,
a certificate or the released BASE and OPTIONS tally; certificate issuance
refuses it, so a certificate needs a `--profile released` run. `validate`
gates the catalogue against the release, zero findings the only pass;
`--profile development` adds the snapshot's absences as information.

### Keeping the pinned text current

`scripts/vendor/spec-docs-development.sh` pins each `master` by commit in its
`COMPONENTS` table; `scripts/vendor/spec-docs.sh` pins each released component
at its `Release-*` tag. The daily `upstream-watch` workflow opens an
`upstream-change` issue when either moves:

- `openEHR <repository>: master moved past the development snapshot`
- `openEHR <repository>: <tag> released`

The body lists commits since the pin and their files, those a citation or an
`upstream/` patch names first. A later move rewrites it and comments; the issue
closes when the pin catches up. Components pinned to a frozen commit (CNF,
ITS-JSON, ITS-XML) get no release issue. The workflow writes only to this
repository.

To act on one:

1. Read the listed commits, citation and patch files first.
2. Move the pin and re-vendor: the development table and
   `scripts/vendor/spec-docs-development.sh`, or the released table and
   `scripts/vendor/spec-docs.sh`. A released move changes the oracle and takes
   the care of an expectation change.
3. Run `validate` under both profiles and re-read every moved citation
   first-hand. An unchanged requirement gains or updates its `development`
   member; a changed one goes through the attribution law.
4. Re-apply `upstream/<repository>/` (`scripts/checks/upstream-patches-apply.sh`).
   A patch that no longer applies means upstream touched the text: record the
   fixing commit on the `upstream-report` issue, or rewrite the patch.

## The catalogue's shape: one behaviour, one case

Every check carries the released sentence assigning its expected value, quoted
verbatim and resolved against the vendored text by `validate`. That sentence
is the authority, never the operation's OpenAPI fragment alone. One behaviour
per case, so a red row names one defect.

## Positive and negative testing carry equal weight

A server that accepts everything passes every positive test. So every invalid
shape is its own refusal workflow, naming its defect and reference, beside a
valid twin that proves acceptance. A lenient server fails the invalid twin,
and deleting one would silently narrow the claim.

- **Content negotiation:** an unsatisfiable `Accept` gets 406, an unsupported
  payload media type 415.
- **Identity and state conflicts:** duplicate identifier, stale preceding
  version, missing version precondition, deleting the already deleted.
- **Content validity:** missing mandatory attributes, empty lists the Reference
  Model requires filled, structurally wrong documents, the wrong template.
- **Scope negatives:** a format applied where the specification does not
  define it.
- **Authentication:** every platform route refuses an unauthenticated request.

Invalid fixtures are raw bytes, since a typed model cannot build them; that
also catches codec bugs.

## When a run goes red

A red row means the specification, catalogue and server disagree. Which one is
wrong is decided before anything changes. Two reflexes are refused:

1. **"The catalogue must be wrong, the server is right."** An expectation
   traces to a citation, so only a better reading refutes it.
2. **"The server must be wrong, the instrument is right."** The instrument is
   what people trust, so it is suspected first. The first live triage
   attributed 7 of 7 diagnosed defects to the instrument, none to the server.

| Suspect | What it means | What happens |
|---|---|---|
| **The server under test** | it violates the released specification | the attribution, with the reproduced exchange and the decisive sentence quoted; the catalogue does not change |
| **The instrument** | the server was correct; the runner misdrove or misjudged | the runner is fixed; the rows were inconclusive, never server failures |
| **The catalogue** | the expectation is wrong against the specification | the artifact is fixed, with a new cited source |

Per row: read what was sent and received, read what the workflow expects and
why, derive first-hand what a conformant server must return, and compare. A
transport fault or an unresolvable step is inconclusive, never a server
failure.

> [!WARNING]
> A catalogue expectation is never adjusted to match observed server behaviour.
> A server response is evidence in the comparison, never the reference.

## Where the specification does not say

Where the released text is silent or two documents disagree, no workflow
asserts the behaviour. It becomes one of:

- **An upstream report:** an `upstream-report` issue here with the released
  text, a proposed patch and the resolution sought. A citation resting on that
  text names it in `report:`.
- **An option:** where the release leaves the choice to the implementation
  (such as serving canonical XML), both answers are conformant.
  `capabilities.yaml` names the option, its arms and the sentence leaving it
  open; the statement picks an arm, and only that arm's workflows run.

And:

- **A silence has to be proven:** a claimed silence the specification defines
  is a catalogue defect, and its workflow becomes gating.
- **Nothing is absorbed:** every carried divergence has an outbound report,
  and CI refuses a `report:` naming no `upstream-report`
  issue.
- **A private resolution is non-conformant:** a harness deciding a silence for
  itself is not implementing this catalogue.

## Verdicts are computed, never asserted

A verdict is a pure function of the statement (claimed capabilities, selected
arms), the IXIT, the catalogue and the recorded exchanges. Two independent
runners must compute identical verdicts from them, which the verification pack
checks. A hand-typed certificate row is a defect. Recording and judging are
separate, so anyone holding the files can re-derive the documents.

Two machineries share this ([`docs/ARCHITECTURE.md`][architecture] §8):

- **Conformance by assertion:** the statement selects documents, criteria
  judge each exchange, and each row ends passed, failed, errored or skipped. A
  skip names its reason (`not-claimed`, `option-not-selected`, `not-declared`)
  so a thin claim shows. A red row carries what was checked, why, expected and
  received.
- **Measured performance:** a declared clinical workload climbs a load ladder
  to the knee, and every number is re-derived from the record's embedded HDR
  histograms, so a stored summary is tamper-checked. It earns no class and
  enters no conformance verdict. Servers are compared interleaved on one host:
  an A/A run of one target against itself sets the noise floor, and an A/B
  difference counts only above it.

Load is open-loop: seeded Poisson arrivals at planned instants, latency
measured from the planned instant, so coordinated omission cannot hide a
stall.

The knee is read against population: about 46 clinical documents per person
per year, a busy hour about eight times the average, and 10 to 50 reads per
write give 13 to 75 peak API arrivals per second per 100,000 people
(derivation from OECD, Eurostat and NHS statistics in
[`docs/ARCHITECTURE.md`][architecture] §8.14). The corpus scale is a size,
never a pass or fail:

| Scale | Corpus |
|---|---|
| POC | 10,000 EHRs |
| S | 100,000 EHRs |
| L | 1,000,000 EHRs |
| R | 10,000,000 EHRs |

## Coverage is a mandate

A gate enumerates the released operations, response branches and negotiation
headers from the released ITS-REST OpenAPI documents, and fails on any without
a covering step or a recorded gap naming its tracker issue. Cases are added,
never removed to make a run go green.

[architecture]: https://github.com/Vernum-Projecten/Veredictum/blob/main/docs/ARCHITECTURE.md
