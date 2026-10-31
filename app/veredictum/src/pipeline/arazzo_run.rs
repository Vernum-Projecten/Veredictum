// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Driving the Arazzo catalogue against a server under test, or re-judging
//! the recording of such a run without one.
//!
//! The seam gates the whole catalogue first, exactly as `validate` does, and
//! runs nothing when the gate has a finding. It then plans every document
//! under the party's declarations ([`crate::arazzo::catalogue`]) and either
//! drives the documents over HTTP into one HAR recording, or replays a
//! recording through them. The outcome carries the results document and
//! the recording; where they are written is the caller's decision.
//!
//! The results record what the server stated through System OPTIONS
//! ([`crate::arazzo::results::ServerIdentity`]), read from the recording so a
//! re-judgement records the same, and the run's static review compares the
//! statement's `spec_versions.its_rest` with the stated
//! `restapi_specs_version` into [`ReviewNote`]s, which never change a row.
//!
//! The caller hands over where each credential comes from, never its value
//! ([`CredentialSource`]). The value is read only inside the transport that
//! sends it ([`CredentialedTransport`]), so nothing the run returns can carry
//! it, and no credential is recorded. The scoped tokens the IXIT declares
//! are located the same way ([`ScopedSources`]), before anything runs.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::arazzo::catalogue::{self, Declarations, Selection};
use crate::arazzo::credential::{CredentialSource, CredentialedTransport, Role, ScopedSources};
use crate::arazzo::evaluate::RunConfig;
use crate::arazzo::gates::check_catalogue;
use crate::arazzo::har::Har;
use crate::arazzo::reading::Tally;
use crate::arazzo::results::{ResultRow, Results, ReviewNote, Status, Whose, relink};
use crate::arazzo::spec_profile::{SpecProfile, Specification};
use crate::exec::transport::HttpTransport;
use crate::ixit::{DeclaredEnvironment, Ixit};
use crate::pipeline::{Error, RenderedFile, load_party_json, read_json};
use crate::schema::ixit_schema;
use crate::vocab::SpecComponent;

/// The file a run's results are written to.
pub const RESULTS_FILE: &str = "results.json";

/// The file a live run's HAR recording is written to.
pub const RECORDING_FILE: &str = "recording.har";

/// The catalogue and the declarations one run reads.
#[derive(Debug, Clone, Copy)]
pub struct ArazzoRunRequest<'a> {
    /// The catalogue root, holding `capabilities.yaml` and `cases/`.
    pub root: &'a Path,
    /// The vendored openEHR spec tree (`specs/openehr`), whose grandparent is
    /// the root citations and OAS sources resolve against. Without it, the
    /// catalogue root's parent is that root.
    pub specs: Option<&'a Path>,
    /// The party's statement, whose `claims.capabilities` and `options`
    /// decide what runs.
    pub statement: Option<&'a Path>,
    /// The party's IXIT, which a rows file's `declared` inputs read.
    pub ixit: Option<&'a Path>,
    /// Only these documents, by id; every document when empty.
    pub documents: &'a [String],
    /// The specification profile whose text the case citations are read
    /// against. The catalogue is gated against the release whatever the
    /// profile.
    pub profile: SpecProfile,
}

/// What the run is driven against.
#[derive(Debug, Clone)]
#[expect(
    clippy::large_enum_variant,
    reason = "one value is built and moved once per run, so boxing a variant saves nothing"
)]
pub enum Drive<'a> {
    /// A server under test, over HTTP.
    Live {
        /// The base URL every OAS path is appended to.
        base_url: &'a str,
        /// Where the ordinary principal's credential comes from.
        ordinary: CredentialSource,
        /// Where the administrative principal's credential comes from; with
        /// [`CredentialSource::None`], every workflow that needs it is
        /// skipped as `not-declared`.
        administrative: CredentialSource,
        /// The SMART Platform base URL a Platform source's paths are appended
        /// to; without it, every workflow with a step sent to it is skipped as
        /// `not-declared`.
        platform_url: Option<&'a str>,
    },
    /// The HAR recording of an earlier run, without a server.
    Rejudge {
        /// The recording.
        recording: &'a Path,
    },
}

/// The outcome of one run.
#[derive(Debug, Clone)]
pub struct ArazzoRunOutcome {
    /// The results document.
    pub results: Results,
    /// The recording of a live run; `None` for a re-judgement.
    pub recording: Option<Har>,
}

impl ArazzoRunOutcome {
    /// The row counts by status.
    #[must_use]
    pub fn tally(&self) -> Tally {
        Tally::of(&self.results.rows)
    }

    /// The rows that bear on the verdict and are not green: every errored
    /// row, and every failed row that is not a warning.
    pub fn red_rows(&self) -> impl Iterator<Item = &ResultRow> {
        self.results.rows.iter().filter(|row| is_red(row))
    }

    /// Whether every row that bears on the verdict passed or was skipped.
    #[must_use]
    pub fn is_green(&self) -> bool {
        self.red_rows().next().is_none()
    }

    /// The rows counted by where each stands against the server, which
    /// decides the run's exit status.
    #[must_use]
    pub fn verdict(&self) -> RunVerdict {
        RunVerdict::of(&self.results.rows)
    }

    /// The files the run writes: [`RESULTS_FILE`], and [`RECORDING_FILE`]
    /// for a live run.
    ///
    /// # Errors
    /// [`Error::Serialize`] or [`Error::Recording`] when a document cannot be
    /// serialized, which a well-formed value never yields.
    pub fn files(&self) -> Result<Vec<RenderedFile>, Error> {
        let mut files = vec![RenderedFile {
            name: RESULTS_FILE.to_owned(),
            body: self.results.to_text().map_err(|source| Error::Serialize {
                context: "serialize the results document".to_owned(),
                source,
            })?,
        }];
        if let Some(recording) = &self.recording {
            files.push(RenderedFile {
                name: RECORDING_FILE.to_owned(),
                body: recording.to_text()?,
            });
        }
        Ok(files)
    }
}

/// Whether `row` bears on the verdict and is not green: it counts against
/// the server or is inconclusive. A warning row, a SHOULD the server did not
/// follow, never counts against a capability.
pub(crate) fn is_red(row: &ResultRow) -> bool {
    Bearing::of(row) != Bearing::Clear
}

/// Where one row stands against the server, read from the attribution its
/// record already carries (`explanation.whose`). No openEHR spec governs
/// this — our own design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bearing {
    /// Passed, skipped, or failed only as a warning.
    Clear,
    /// Failed or errored with the server as whose problem it is, or with no
    /// attribution recorded at all.
    AgainstServer,
    /// Failed or errored with the instrument as whose problem it is: the
    /// instrument could not judge the row, and it does not count against the
    /// vendor.
    Inconclusive,
}

impl Bearing {
    /// The bearing of `row`.
    #[must_use]
    pub fn of(row: &ResultRow) -> Self {
        let whose = row
            .explanation
            .as_ref()
            .map(|explanation| explanation.whose);
        match (row.status, whose) {
            (Status::Passed | Status::Skipped, _) | (Status::Failed, Some(Whose::Warning)) => {
                Self::Clear
            }
            (Status::Failed | Status::Errored, Some(Whose::Instrument)) => Self::Inconclusive,
            // NOTE: no openEHR spec governs this — our own design; a red row whose record
            // names no instrument attribution counts against the server, so it never passes.
            (Status::Failed | Status::Errored, _) => Self::AgainstServer,
        }
    }
}

/// The exit status of a run with a row that counts against the server.
pub const EXIT_AGAINST_SERVER: u8 = 1;

/// The exit status of a run with no row against the server and at least one
/// inconclusive row.
pub const EXIT_INCONCLUSIVE: u8 = 3;

/// A run's rows counted by where each stands against the server.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunVerdict {
    /// The rows that count against the server.
    pub against_server: usize,
    /// The rows the instrument could not judge.
    pub inconclusive: usize,
}

impl RunVerdict {
    /// Counts `rows`.
    #[must_use]
    pub fn of<'a>(rows: impl IntoIterator<Item = &'a ResultRow>) -> Self {
        let mut verdict = Self::default();
        for row in rows {
            let count = match Bearing::of(row) {
                Bearing::Clear => continue,
                Bearing::AgainstServer => &mut verdict.against_server,
                Bearing::Inconclusive => &mut verdict.inconclusive,
            };
            *count = count.saturating_add(1);
        }
        verdict
    }

    /// The exit status the run ends with: [`EXIT_AGAINST_SERVER`] when any
    /// row counts against the server, whatever else the run holds; otherwise
    /// [`EXIT_INCONCLUSIVE`] when any row is inconclusive; otherwise `0`.
    #[must_use]
    pub const fn exit_status(self) -> u8 {
        if self.against_server > 0 {
            EXIT_AGAINST_SERVER
        } else if self.inconclusive > 0 {
            EXIT_INCONCLUSIVE
        } else {
            0
        }
    }
}

impl std::fmt::Display for RunVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} row(s) against the server, {} inconclusive row(s)",
            self.against_server, self.inconclusive
        )
    }
}

/// The root citations and OAS sources resolve against: the catalogue root's
/// parent, where the vendored `specs/` tree sits beside it.
#[must_use]
pub fn repo_root_of(root: &Path) -> PathBuf {
    root.join("..")
}

impl ArazzoRunRequest<'_> {
    /// The root citations and OAS sources resolve against: the grandparent
    /// of [`Self::specs`] when it is given, as `validate --specs` reads it,
    /// and otherwise [`repo_root_of`] the catalogue root.
    #[must_use]
    pub fn repo_root(&self) -> PathBuf {
        self.specs
            .map_or_else(|| repo_root_of(self.root), |specs| specs.join("../.."))
    }

    /// The vendored openEHR spec tree the catalogue is gated against:
    /// [`Self::specs`] when it is given, and otherwise `specs/openehr` under
    /// [`Self::repo_root`].
    #[must_use]
    pub fn spec_root(&self) -> PathBuf {
        self.specs
            .map_or_else(|| self.repo_root().join("specs/openehr"), Path::to_path_buf)
    }
}

/// What a run drives.
#[expect(
    clippy::large_enum_variant,
    reason = "one value is built and moved once per run, so boxing a variant saves nothing"
)]
enum Target {
    /// A server, under this configuration, with credentials from these
    /// sources.
    Live {
        config: RunConfig,
        ordinary: CredentialSource,
        administrative: CredentialSource,
        scoped: ScopedSources,
    },
    /// This recording.
    Rejudge(Har),
}

/// The `Authorization` value a principal's steps carry through the run: its
/// marker when it presents a credential.
fn marker(source: &CredentialSource, role: Role) -> Option<String> {
    source.is_supplied().then(|| role.marker().to_owned())
}

/// Reads the IXIT at `path` through the published IXIT schema and the typed
/// model, returning the document a rows file's `declared` inputs read and
/// the environment it declares.
///
/// # Errors
/// [`Error::Party`] naming the file and every schema violation, or the
/// typed-model failure.
fn read_ixit(path: &Path) -> Result<(Value, Option<DeclaredEnvironment>), Error> {
    let value: Value = load_party_json(path, &ixit_schema(), "ixit")?;
    let ixit: Ixit = serde_json::from_value(value.clone())
        .map_err(|e| Error::Party(format!("{}: model: {e}", path.display())))?;
    Ok((value, ixit.environment))
}

/// Runs the catalogue live, or re-judges a recording of it.
///
/// Everything the party supplies is read and checked before the gate, so an
/// unknown document or capability is refused at once.
///
/// # Errors
/// [`Error::Read`] or [`Error::Parse`] for the statement, [`Error::Party`]
/// for an IXIT that fails the published IXIT schema or the typed model,
/// [`Error::Recording`] when the recording does not read,
/// [`Error::ArazzoCatalogue`] when the catalogue cannot be planned,
/// [`Error::ArazzoGate`] when the catalogue has a gate finding,
/// [`Error::Snapshot`] when the development profile finds no snapshot to read,
/// [`Error::Instrument`] when no HTTP client can be built,
/// [`Error::Credential`] when a located credential no longer reads, or the
/// IXIT's scoped tokens are malformed or not in the environment, and
/// [`Error::ArazzoRun`] when a document cannot be run or the recording does
/// not replay.
pub fn run_catalogue(
    request: &ArazzoRunRequest<'_>,
    drive: Drive<'_>,
) -> Result<ArazzoRunOutcome, Error> {
    let repo_root = request.repo_root();
    let read = |path: Option<&Path>, context: &str| -> Result<Option<Value>, Error> {
        path.map(|path| read_json(path, context)).transpose()
    };
    let (ixit, environment) = match request.ixit {
        Some(path) => {
            let (value, environment) = read_ixit(path)?;
            (Some(value), environment)
        }
        None => (None, None),
    };
    let mut declarations = Declarations {
        statement: read(request.statement, "the statement")?,
        ixit,
        environment,
        administrative: false,
        platform: false,
        profile: request.profile,
    };
    let target = match drive {
        Drive::Live {
            base_url,
            ordinary,
            administrative,
            platform_url,
        } => {
            let scoped = ScopedSources::locate(declarations.ixit.as_ref())?;
            let config = RunConfig {
                authorization: marker(&ordinary, Role::Ordinary),
                administrative: marker(&administrative, Role::Administrative),
                platform_url: platform_url.map(str::to_owned),
                scoped: scoped.coverage(),
                ..RunConfig::new(base_url)
            };
            declarations.administrative = administrative.is_supplied();
            declarations.platform = platform_url.is_some();
            Target::Live {
                config,
                ordinary,
                administrative,
                scoped,
            }
        }
        Drive::Rejudge { recording } => {
            let recording = Har::read(recording)?;
            declarations.administrative = recording.log.veredictum.administrative;
            declarations.platform = recording.log.veredictum.platform_url.is_some();
            Target::Rejudge(recording)
        }
    };
    let selection = Selection::read(
        request.root,
        declarations.statement.as_ref(),
        request.documents,
    )?;
    let declared_its_rest = catalogue::declared_versions(
        declarations.statement.as_ref(),
        &BTreeSet::from([SpecComponent::ItsRest]),
    )?
    .remove(&SpecComponent::ItsRest)
    .map(|version| version.to_string());
    let findings = check_catalogue(request.root, Some(&request.spec_root()));
    if !findings.is_empty() {
        return Err(Error::ArazzoGate(findings));
    }
    let specification = Specification::of(request.profile, &repo_root)?;
    let planned = selection.load(&repo_root, &declarations)?;
    let mut outcome = match target {
        Target::Live {
            config,
            ordinary,
            administrative,
            scoped,
        } => {
            let http = HttpTransport::new().map_err(Error::Instrument)?;
            let mut transport = CredentialedTransport::new(http, &ordinary, &administrative)?
                .with_scoped(&scoped)?;
            let (results, recording) =
                catalogue::run(&planned, &config, &repo_root, &mut transport)?;
            ArazzoRunOutcome {
                results,
                recording: Some(recording),
            }
        }
        Target::Rejudge(recording) => ArazzoRunOutcome {
            results: catalogue::rejudge(&planned, &repo_root, &recording)?,
            recording: None,
        },
    };
    outcome.results.outline = selection.outline(&planned);
    name_the_text(&mut outcome.results, specification, &planned, &repo_root);
    if let Some(server) = &outcome.results.server {
        outcome.results.review = ReviewNote::review(declared_its_rest.as_deref(), server);
    }
    Ok(outcome)
}

/// Records `specification` on `results`, and under a profile other than the
/// released one points every citation of a row at that profile's text.
fn name_the_text(
    results: &mut Results,
    specification: Specification,
    planned: &[catalogue::Planned],
    repo_root: &Path,
) {
    if !specification.is_released() {
        let citations: Vec<_> = planned
            .iter()
            .flat_map(|planned| planned.document.description().workflows.iter())
            .flat_map(crate::arazzo::structure::workflow_citations)
            .collect();
        relink(
            &mut results.rows,
            repo_root,
            specification.profile(),
            &citations,
        );
    }
    results.specification = specification;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arazzo::credential::{ADMINISTRATIVE_MARKER, Locator, ORDINARY_MARKER};
    use crate::arazzo::results::Explanation;
    use std::collections::BTreeMap;

    #[test]
    fn a_principal_carries_its_marker_only_when_it_presents_a_credential() {
        let token = CredentialSource::Bearer {
            token: Locator::Variable("T".to_owned()),
        };
        assert_eq!(
            marker(&token, Role::Ordinary).as_deref(),
            Some(ORDINARY_MARKER)
        );
        assert_eq!(
            marker(&token, Role::Administrative).as_deref(),
            Some(ADMINISTRATIVE_MARKER)
        );
        assert_eq!(marker(&CredentialSource::None, Role::Administrative), None);
    }

    fn row(status: Status, whose: Option<Whose>) -> ResultRow {
        ResultRow {
            case: "ehr/w".to_owned(),
            row: 0,
            status,
            inputs: BTreeMap::new(),
            generated: Vec::new(),
            skip: None,
            explanation: whose.map(|whose| Explanation {
                step: "s".to_owned(),
                what: "w".to_owned(),
                why: None,
                expected: "e".to_owned(),
                received: "r".to_owned(),
                exchange: None,
                whose,
            }),
        }
    }

    #[test]
    fn a_warning_row_is_the_only_failed_row_that_stays_green() {
        assert!(!is_red(&row(Status::Passed, None)));
        assert!(!is_red(&row(Status::Skipped, None)));
        assert!(!is_red(&row(Status::Failed, Some(Whose::Warning))));
        assert!(is_red(&row(Status::Failed, Some(Whose::Server))));
        assert!(is_red(&row(Status::Failed, None)));
        assert!(is_red(&row(Status::Errored, Some(Whose::Instrument))));
    }

    #[test]
    fn a_row_bears_on_the_server_by_the_attribution_it_carries() {
        let cases = [
            (Status::Passed, None, Bearing::Clear),
            (Status::Skipped, None, Bearing::Clear),
            (Status::Failed, Some(Whose::Warning), Bearing::Clear),
            (Status::Failed, Some(Whose::Server), Bearing::AgainstServer),
            (Status::Errored, Some(Whose::Server), Bearing::AgainstServer),
            (Status::Failed, None, Bearing::AgainstServer),
            (Status::Errored, None, Bearing::AgainstServer),
            (
                Status::Errored,
                Some(Whose::Instrument),
                Bearing::Inconclusive,
            ),
            (
                Status::Failed,
                Some(Whose::Instrument),
                Bearing::Inconclusive,
            ),
        ];
        for (status, whose, bearing) in cases {
            assert_eq!(
                Bearing::of(&row(status, whose)),
                bearing,
                "{status:?} {whose:?}"
            );
        }
    }

    #[test]
    fn only_instrument_errored_rows_exit_three() {
        let rows = [
            row(Status::Passed, None),
            row(Status::Errored, Some(Whose::Instrument)),
            row(Status::Errored, Some(Whose::Instrument)),
            row(Status::Failed, Some(Whose::Warning)),
        ];
        let verdict = RunVerdict::of(&rows);
        assert_eq!(
            verdict,
            RunVerdict {
                against_server: 0,
                inconclusive: 2
            }
        );
        assert_eq!(verdict.exit_status(), EXIT_INCONCLUSIVE);
        assert_eq!(
            verdict.to_string(),
            "0 row(s) against the server, 2 inconclusive row(s)"
        );
    }

    #[test]
    fn a_server_failed_row_exits_one_whatever_else_the_run_holds() {
        let rows = [
            row(Status::Errored, Some(Whose::Instrument)),
            row(Status::Failed, Some(Whose::Server)),
            row(Status::Failed, Some(Whose::Warning)),
            row(Status::Skipped, None),
            row(Status::Passed, None),
        ];
        let verdict = RunVerdict::of(&rows);
        assert_eq!(verdict.against_server, 1);
        assert_eq!(verdict.inconclusive, 1);
        assert_eq!(verdict.exit_status(), EXIT_AGAINST_SERVER);
    }

    #[test]
    fn a_run_with_no_red_row_exits_zero() {
        let rows = [
            row(Status::Passed, None),
            row(Status::Skipped, None),
            row(Status::Failed, Some(Whose::Warning)),
        ];
        assert_eq!(RunVerdict::of(&rows).exit_status(), 0);
        assert_eq!(RunVerdict::of(&[]).exit_status(), 0);
    }
}
