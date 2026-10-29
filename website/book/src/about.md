# About the project

Where the method comes from, why the instrument has its name, where the
design reasoning lives, whose work it builds on, and the licence it ships under.

<!-- toc -->

## Lineage

None of the vocabulary here is invented. ISO/IEC 9646 standardized this
architecture in 1991: a supplier's conformance statement (ICS) selects the
applicable cases from an Abstract Test Suite, the supplier's IXIT provides the
instance parameters to run them, and verdicts land in a standardized report.
ETSI, the Bluetooth SIG and USB-IF still run on it. In those terms the
catalogue is the ATS, `statement.json` is the ICS, and `ixit.json` is the IXIT.

openEHR's own conformance component defined the right concepts and then
stalled: its last content amendment is from March 2022, its assessment layer
was never written, and it carries zero AQL test cases. That component remains
the structural guide for which behaviours need covering. It is never the
correctness authority; the released specifications are.

## Origin of the name

*Veredictum* is medieval Latin for "truly spoken", *vere dictum*, and it is the
word that became the English *verdict*. That is what this instrument produces:
it runs the catalogue against a running CDR and speaks a verdict about what it
observed. The seal on the project's front page is the mark of that verdict.

## Design record

[`docs/ARCHITECTURE.md`][architecture] carries the reasoning rather than a
summary of it: the Arazzo catalogue with its extensions, condition profile and
gates, the party documents, the verdict computation, and the population-anchored performance-class model with its
hospital-simulation journey decomposition. It also records the evidence base
for why the instrument exists in this shape: the state of the official openEHR
CNF component, how other standards run conformance, and the ISO/IEC 9646 and
CASCO vocabulary the scheme is built in.

## Credits

The conformance work here stands on work other people did first. Each entry
below says what that work contributed to this catalogue.

- **The openEHR SEC and the CNF authors:** the
  [Conformance component](https://specifications.openehr.org/releases/CNF/development),
  whose Conformance Guide and Platform Conformance Test Schedule set the SUT
  model, the profile matrix and the certificate shape, and say which behaviours
  a platform product has to be tested for. The schedule's amendment record
  names T Beale, B Naess, I McNicoll, C Chevalley, H Frankel, S Iancu, B Lah
  and W Wagner across its revisions, beside P Pazos. 349 of the 1146 case cores
  of the former case-core catalogue cited one of its Test Schedule chapters.
- **Pablo Pazos (CaboLabs):** the fleshed EHR, COMPOSITION, CONTRIBUTION and
  DIRECTORY chapters of that schedule, which are its usable core. The amendment
  record names him as the raiser of Test Schedule revisions 0.8.0
  (23 Nov 2021) through 0.8.6 (24 Mar 2022), and as co-author of the
  Conformance Guide's initial writing with T Beale. He wrote the original 2019
  EHRbase conformance tests at Hannover Medical School, and his
  [openEHR conformance verification framework][pazos-framework]
  is the expanded continuation of that work, carrying a conformance testing
  specification of its own. He has argued the case for openEHR conformance
  testing on the community forums for years. 127 of the 1146 former case cores
  cited the four chapters those revisions wrote.
- **The EHRbase and vitasystems team:** the executable battery. The 223 Robot
  files the CNF component vendored name Wladislaw Wagner (Vitasystems GmbH),
  Pablo Pazos and Jake Smolka (Hannover Medical School) in their copyright
  headers, and the team maintains that set as its
  [integration tests](https://github.com/ehrbase/integration-tests). 156 of the
  462 provenance records of the former corpus named that set as the source of
  the entry's bytes or of its template skeleton, each one re-adjudicated against
  the released specifications.
- **The openEHR Foundation:** the
  [released specifications](https://specifications.openehr.org/) every
  expectation in the catalogue cites, and the machine-readable artifacts the
  workflows resolve against: the ITS-XML and ITS-JSON schema bundles and the
  ITS-REST OpenAPI documents.

The Test Schedule chapters are cited as the structural guide to which
behaviours need covering. The correctness authority is always the released
specification a case cites.

## Licence

Apache-2.0. Attribution travels with every copy and derivative through the
licence and the `NOTICE` file, as its section 4 requires. The vendored
specification text and clinical models keep their upstream terms, recorded per
tree in `PROVENANCE.md` and declared machine-readably in `REUSE.toml`.

## What the front-page badges read

Every badge on the repository's front page is a live reading, not a claim. The
SonarQube Cloud badges come from the analysis lane in
`.github/workflows/sonar.yml`, coverage included; the two OpenSSF badges are
the Scorecard weekly analysis and Best Practices project 14252; the registry
badges read crates.io, docs.rs and the GHCR package directly, so the version
shown is whatever is actually published, the docs.rs badge goes red if a
docs.rs build fails, and the pull count is the package's own. Several read
below their ceiling today, and the Scorecard workflow's header says why, check
by check. Fuzzing reads low by Scorecard's own detection (it looks for OSS-Fuzz
and the integrations it knows), while the harnesses exist under `fuzz/`
([#11](https://github.com/Vernum-Projecten/Veredictum/issues/11)): CI compiles
them per pull request and campaigns weekly. Those are the honest numbers, and
they are the baseline the next ones are measured against.

The SLSA badge is the one static image there, and its claim is substantiated
rather than asserted: binaries and images build inside reusable workflows
(`release-build.yml`, `build-image.yml`) per GitHub's documented SLSA Build L3
construction, each artifact carries a signed provenance attestation on its
digest, and [Installation](installation.md#from-a-release-binary) shows the
`gh attestation verify` invocation that checks it.

[architecture]: https://github.com/Vernum-Projecten/Veredictum/blob/main/docs/ARCHITECTURE.md
[pazos-framework]: https://github.com/ppazos/openehr-conformance-verification
