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
//! The caller hands over where each credential comes from, never its value
//! ([`CredentialSource`]). The value is read only inside the transport that
//! sends it ([`CredentialedTransport`]), so nothing the run returns can carry
//! it, and neither credential is recorded.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::arazzo::catalogue::{self, Declarations, Selection};
use crate::arazzo::credential::{CredentialSource, CredentialedTransport, Role};
use crate::arazzo::evaluate::RunConfig;
use crate::arazzo::export::Tally;
use crate::arazzo::gates::check_catalogue;
use crate::arazzo::har::Har;
use crate::arazzo::results::{ResultRow, Results, Status, Whose};
use crate::exec::transport::HttpTransport;
use crate::ixit::{DeclaredEnvironment, Ixit};
use crate::pipeline::{Error, RenderedFile, load_party_json, read_json};
use crate::schema::ixit_schema;

/// The file a run's results are written to.
pub const RESULTS_FILE: &str = "results.json";

/// The file a live run's HAR recording is written to.
pub const RECORDING_FILE: &str = "recording.har";

/// The catalogue and the declarations one run reads.
#[derive(Debug, Clone, Copy)]
pub struct ArazzoRunRequest<'a> {
    /// The catalogue root, holding `capabilities.yaml` and `cases/`. Its
    /// parent is the root citations and OAS sources resolve against.
    pub root: &'a Path,
    /// The party's statement, whose `claims.capabilities` and `options`
    /// decide what runs.
    pub statement: Option<&'a Path>,
    /// The party's IXIT, which a rows file's `declared` inputs read.
    pub ixit: Option<&'a Path>,
    /// Only these documents, by id; every document when empty.
    pub documents: &'a [String],
}

/// What the run is driven against.
#[derive(Debug, Clone)]
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
    /// row, and every failed row that is not advisory.
    pub fn red_rows(&self) -> impl Iterator<Item = &ResultRow> {
        self.results.rows.iter().filter(|row| is_red(row))
    }

    /// Whether every row that bears on the verdict passed or was skipped.
    #[must_use]
    pub fn is_green(&self) -> bool {
        self.red_rows().next().is_none()
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

/// Whether `row` bears on the verdict and is not green. An advisory row,
/// a SHOULD the server did not follow, never counts against a capability.
fn is_red(row: &ResultRow) -> bool {
    match row.status {
        Status::Passed | Status::Skipped => false,
        Status::Errored => true,
        Status::Failed => row
            .explanation
            .as_ref()
            .is_none_or(|explanation| explanation.whose != Whose::Advisory),
    }
}

/// The root citations and OAS sources resolve against: the catalogue root's
/// parent, where the vendored `specs/` tree sits beside it.
#[must_use]
pub fn repo_root_of(root: &Path) -> PathBuf {
    root.join("..")
}

/// What a run drives.
enum Target {
    /// A server, under this configuration, with credentials from these
    /// sources.
    Live {
        config: RunConfig,
        ordinary: CredentialSource,
        administrative: CredentialSource,
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
/// [`Error::Instrument`] when no HTTP client can be built,
/// [`Error::Credential`] when a located credential no longer reads, and
/// [`Error::ArazzoRun`] when a document cannot be run or the recording does
/// not replay.
pub fn run_catalogue(
    request: &ArazzoRunRequest<'_>,
    drive: Drive<'_>,
) -> Result<ArazzoRunOutcome, Error> {
    let repo_root = repo_root_of(request.root);
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
    };
    let target = match drive {
        Drive::Live {
            base_url,
            ordinary,
            administrative,
        } => {
            let config = RunConfig {
                base_url: base_url.to_owned(),
                authorization: marker(&ordinary, Role::Ordinary),
                signing: None,
                administrative: marker(&administrative, Role::Administrative),
            };
            declarations.administrative = administrative.is_supplied();
            Target::Live {
                config,
                ordinary,
                administrative,
            }
        }
        Drive::Rejudge { recording } => {
            let recording = Har::read(recording)?;
            declarations.administrative = recording.log.veredictum.administrative;
            Target::Rejudge(recording)
        }
    };
    let selection = Selection::read(
        request.root,
        declarations.statement.as_ref(),
        request.documents,
    )?;
    let findings = check_catalogue(request.root, Some(&repo_root.join("specs/openehr")));
    if !findings.is_empty() {
        return Err(Error::ArazzoGate(findings));
    }
    let planned = selection.load(&repo_root, &declarations)?;
    match target {
        Target::Live {
            config,
            ordinary,
            administrative,
        } => {
            let http = HttpTransport::new().map_err(Error::Instrument)?;
            let mut transport = CredentialedTransport::new(http, &ordinary, &administrative)?;
            let (results, recording) =
                catalogue::run(&planned, &config, &repo_root, &mut transport)?;
            Ok(ArazzoRunOutcome {
                results,
                recording: Some(recording),
            })
        }
        Target::Rejudge(recording) => Ok(ArazzoRunOutcome {
            results: catalogue::rejudge(&planned, &repo_root, &recording)?,
            recording: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arazzo::credential::{ADMINISTRATIVE_MARKER, ORDINARY_MARKER, SecretSource};
    use crate::arazzo::results::Explanation;
    use std::collections::BTreeMap;

    #[test]
    fn a_principal_carries_its_marker_only_when_it_presents_a_credential() {
        let token = CredentialSource::Bearer {
            token: SecretSource::Variable("T".to_owned()),
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
    fn an_advisory_row_is_the_only_failed_row_that_stays_green() {
        assert!(!is_red(&row(Status::Passed, None)));
        assert!(!is_red(&row(Status::Skipped, None)));
        assert!(!is_red(&row(Status::Failed, Some(Whose::Advisory))));
        assert!(is_red(&row(Status::Failed, Some(Whose::Server))));
        assert!(is_red(&row(Status::Failed, None)));
        assert!(is_red(&row(Status::Errored, Some(Whose::Instrument))));
    }
}
