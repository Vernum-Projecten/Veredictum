# About the project

<!-- toc -->

## Lineage

ISO/IEC 9646 standardized this architecture in 1991, and ETSI, the Bluetooth
SIG and USB-IF still use it: a supplier's conformance statement (ICS) selects
cases from an Abstract Test Suite, the supplier's IXIT supplies instance
parameters, and verdicts land in a standardized report. Here the catalogue is
the ATS, `statement.json` the ICS and `ixit.json` the IXIT.

openEHR's conformance component defined the right concepts and stalled: last
content amendment March 2022, no assessment layer, zero AQL test cases. It
guides which behaviours to cover; the released specifications decide
correctness.

## Origin of the name

*Veredictum* is medieval Latin for "truly spoken", *vere dictum*, the word that
became *verdict*. The instrument runs the catalogue against a CDR and speaks a
verdict on what it observed; the front-page seal marks it.

## Design record

[`docs/ARCHITECTURE.md`][architecture] holds the reasoning: the Arazzo
catalogue with its extensions, condition profile and gates, the party
documents, the verdict computation, and the population-anchored
performance-class model with its hospital-simulation journey decomposition.
It also records the evidence for this shape: the state of the openEHR CNF
component, how other standards run conformance, and the ISO/IEC 9646 and
CASCO vocabulary.

## Credits

- **The openEHR SEC and the CNF authors:** the
  [Conformance component](https://specifications.openehr.org/releases/CNF/development).
  Its Conformance Guide and Platform Conformance Test Schedule set the SUT
  model, profile matrix and certificate shape, and name what a platform
  product must be tested for. The schedule's amendment record names T Beale,
  B Naess, I McNicoll, C Chevalley, H Frankel, S Iancu, B Lah and W Wagner,
  beside P Pazos. 349 of the 1146 case cores of the former case-core catalogue
  cited a Test Schedule chapter.
- **Pablo Pazos (CaboLabs):** the fleshed EHR, COMPOSITION, CONTRIBUTION and
  DIRECTORY chapters, the schedule's usable core, raised as Test Schedule
  revisions 0.8.0 (23 Nov 2021) through 0.8.6 (24 Mar 2022); co-author with
  T Beale of the Conformance Guide's initial writing. He wrote the original
  2019 EHRbase conformance tests at Hannover Medical School, continued in his
  [openEHR conformance verification framework][pazos-framework] with its own
  conformance testing specification, and has argued for openEHR conformance
  testing on the community forums for years. 127 of the 1146 former case cores
  cited the four chapters those revisions wrote.
- **The EHRbase and vitasystems team:** the executable battery. The 223 Robot
  files the CNF component vendored name Wladislaw Wagner (Vitasystems GmbH),
  Pablo Pazos and Jake Smolka (Hannover Medical School) in their copyright
  headers; the team maintains them as its
  [integration tests](https://github.com/ehrbase/integration-tests). 156 of the
  462 provenance records of the former corpus named that set as the source of
  the entry's bytes or template skeleton, each re-adjudicated against the
  released specifications.
- **The openEHR Foundation:** the
  [released specifications](https://specifications.openehr.org/) every
  expectation cites, and the ITS-XML and ITS-JSON schema bundles and ITS-REST
  OpenAPI documents the workflows resolve against.

The Test Schedule chapters are cited as a coverage guide. The correctness
authority is always the released specification a case cites.

## Licence

Apache-2.0. Attribution travels with every copy and derivative through the
licence and the `NOTICE` file, as its section 4 requires. Vendored
specification text and clinical models keep their upstream terms, recorded
per tree in `PROVENANCE.md` and machine-readably in `REUSE.toml`.

## What the front-page badges read

Every badge is a live reading except SLSA.

- **SonarQube Cloud:** the analysis lane in `.github/workflows/sonar.yml`,
  coverage included.
- **OpenSSF:** the weekly Scorecard analysis and Best Practices project 14252.
  The Scorecard workflow's header explains each check below its ceiling.
  Fuzzing reads low because Scorecard detects only OSS-Fuzz and integrations
  it knows; the `fuzz/` harnesses
  ([#11](https://github.com/Vernum-Projecten/Veredictum/issues/11)) compile on
  every pull request and campaign weekly.
- **Registries:** crates.io, docs.rs and GHCR, read directly: the published
  version, docs.rs red on a failed build, the package's own pull count.
- **SLSA:** static. Binaries and images build in reusable workflows
  (`release-build.yml`, `build-image.yml`) per GitHub's documented SLSA Build
  L3 construction, each artifact carries a signed provenance attestation on
  its digest, and [Installation](installation.md#from-a-release-binary) shows
  the `gh attestation verify` check.

[architecture]: https://github.com/Vernum-Projecten/Veredictum/blob/main/docs/ARCHITECTURE.md
[pazos-framework]: https://github.com/ppazos/openehr-conformance-verification
