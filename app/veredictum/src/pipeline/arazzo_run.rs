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
//! A credential is read from the environment, never from an argument: a
//! variable `NAME`, or a file named by `NAME_FILE`, the convention container
//! secrets are mounted under. Neither the credential nor the
//! administrative one is recorded.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde_json::Value;

use crate::arazzo::catalogue::{self, Declarations, Selection};
use crate::arazzo::evaluate::RunConfig;
use crate::arazzo::export::Tally;
use crate::arazzo::gates::check_catalogue;
use crate::arazzo::har::Har;
use crate::arazzo::results::{ResultRow, Results, Status, Whose};
use crate::bench::client::AuthKind;
use crate::exec::transport::HttpTransport;
use crate::pipeline::{Error, RenderedFile, read_json};

/// The file a run's results are written to.
pub const RESULTS_FILE: &str = "results.json";

/// The file a live run's HAR recording is written to.
pub const RECORDING_FILE: &str = "recording.har";

/// The suffix of the variable that names a file holding a secret.
pub const FILE_SUFFIX: &str = "_FILE";

/// The user of the administrative principal under `basic`.
pub const ADMIN_USER_ENV: &str = "VEREDICTUM_ADMIN_USER";

/// The password of the administrative principal under `basic`.
pub const ADMIN_PASSWORD_ENV: &str = "VEREDICTUM_ADMIN_PASSWORD";

/// The token of the administrative principal under `bearer`.
pub const ADMIN_TOKEN_ENV: &str = "VEREDICTUM_ADMIN_TOKEN";

/// The catalogue and the declarations one run reads.
#[derive(Debug, Clone, Copy)]
pub struct ArazzoRunRequest<'a> {
    /// The catalogue root, holding `capabilities.yaml` and `cases/`. Its
    /// parent is the root citations and OAS sources resolve against.
    pub root: &'a Path,
    /// The party's statement, whose `options` select the option arms.
    pub statement: Option<&'a Path>,
    /// The party's IXIT, which a rows file's `declared` inputs read.
    pub ixit: Option<&'a Path>,
    /// Only these documents, by id; every document when empty.
    pub documents: &'a [String],
}

/// What the run is driven against.
#[derive(Debug, Clone, Copy)]
pub enum Drive<'a> {
    /// A server under test, over HTTP.
    Live {
        /// The base URL every OAS path is appended to.
        base_url: &'a str,
        /// How the ordinary principal presents itself.
        auth: AuthKind,
        /// How the administrative principal presents itself; with
        /// [`AuthKind::None`], every workflow that needs it is skipped as
        /// `not-declared`.
        admin_auth: AuthKind,
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

/// What a run drives, once its credentials or its recording are read.
enum Target {
    /// A server, under this configuration.
    Live(RunConfig),
    /// This recording.
    Rejudge(Har),
}

/// Runs the catalogue live, or re-judges a recording of it.
///
/// Everything the party supplies is read and checked before the gate, so a
/// missing credential or an unknown document is refused at once.
///
/// # Errors
/// [`Error::Read`] or [`Error::Parse`] for the statement or the IXIT,
/// [`Error::Credential`] when a credential is not supplied,
/// [`Error::Recording`] when the recording does not read,
/// [`Error::ArazzoCatalogue`] when the catalogue cannot be planned,
/// [`Error::ArazzoGate`] when the catalogue has a gate finding,
/// [`Error::Instrument`] when no HTTP client can be built, and
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
    let mut declarations = Declarations {
        statement: read(request.statement, "the statement")?,
        ixit: read(request.ixit, "the IXIT")?,
        administrative: false,
    };
    let target = match drive {
        Drive::Live {
            base_url,
            auth,
            admin_auth,
        } => {
            let lookup = |name: &str| std::env::var(name).ok();
            let config = RunConfig {
                base_url: base_url.to_owned(),
                authorization: authorization(auth, &SUT_ENVS, &lookup)?,
                signing: None,
                administrative: authorization(admin_auth, &ADMIN_ENVS, &lookup)?,
            };
            declarations.administrative = config.administrative.is_some();
            Target::Live(config)
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
        Target::Live(config) => {
            let mut transport = HttpTransport::new().map_err(Error::Instrument)?;
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

/// The variables one principal's credential is read from.
struct Envs {
    user: &'static str,
    password: &'static str,
    token: &'static str,
}

const SUT_ENVS: Envs = Envs {
    user: crate::identity::USER_ENV,
    password: crate::identity::PASSWORD_ENV,
    token: crate::identity::TOKEN_ENV,
};

const ADMIN_ENVS: Envs = Envs {
    user: ADMIN_USER_ENV,
    password: ADMIN_PASSWORD_ENV,
    token: ADMIN_TOKEN_ENV,
};

/// The `Authorization` value `auth` presents, its secrets read through
/// `lookup`; `None` under [`AuthKind::None`].
fn authorization(
    auth: AuthKind,
    envs: &Envs,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<Option<String>, Error> {
    match auth {
        AuthKind::None => Ok(None),
        AuthKind::Basic => {
            let user = secret(envs.user, lookup)?;
            let password = secret(envs.password, lookup)?;
            let pair =
                base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"));
            Ok(Some(format!("Basic {pair}")))
        }
        AuthKind::Bearer => Ok(Some(format!("Bearer {}", secret(envs.token, lookup)?))),
    }
}

/// The secret in variable `name`, or in the file `name` + [`FILE_SUFFIX`]
/// names.
///
/// One trailing line ending is dropped from a file, the one an editor or an
/// `echo` leaves; anything else in it is the secret.
fn secret(name: &str, lookup: &dyn Fn(&str) -> Option<String>) -> Result<String, Error> {
    let file_var = format!("{name}{FILE_SUFFIX}");
    match (lookup(name), lookup(&file_var)) {
        (Some(value), None) => Ok(value),
        (None, Some(path)) => {
            let text = std::fs::read_to_string(&path).map_err(|e| {
                Error::Credential(format!(
                    "{file_var} names `{path}`, which cannot be read: {e}"
                ))
            })?;
            let text = text.strip_suffix('\n').unwrap_or(&text);
            Ok(text.strip_suffix('\r').unwrap_or(text).to_owned())
        }
        (Some(_), Some(_)) => Err(Error::Credential(format!(
            "both {name} and {file_var} are set; set one"
        ))),
        (None, None) => Err(Error::Credential(format!(
            "the credential is read from {name} or from the file {file_var} names, and neither is set"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arazzo::results::Explanation;
    use assert_fs::prelude::*;
    use std::collections::BTreeMap;

    fn lookup(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let vars: BTreeMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| vars.get(name).cloned()
    }

    #[test]
    fn a_secret_comes_from_its_variable_or_the_file_its_file_variable_names() {
        let dir = assert_fs::TempDir::new().unwrap();
        let file = dir.child("password");
        file.write_str("s3cret\r\n").unwrap();
        let path = file.path().to_str().unwrap();
        assert_eq!(secret("P", &lookup(&[("P", "plain")])).unwrap(), "plain");
        assert_eq!(secret("P", &lookup(&[("P_FILE", path)])).unwrap(), "s3cret");
        assert!(matches!(
            secret("P", &lookup(&[("P", "x"), ("P_FILE", path)])),
            Err(Error::Credential(_))
        ));
        assert!(matches!(
            secret("P", &lookup(&[])),
            Err(Error::Credential(_))
        ));
        assert!(matches!(
            secret("P", &lookup(&[("P_FILE", "/nonexistent/secret")])),
            Err(Error::Credential(_))
        ));
    }

    #[test]
    fn each_principal_reads_its_own_variables() {
        let vars = lookup(&[
            ("VEREDICTUM_SUT_USER", "alice"),
            ("VEREDICTUM_SUT_PASSWORD", "a"),
            ("VEREDICTUM_ADMIN_TOKEN", "t"),
        ]);
        assert_eq!(
            authorization(AuthKind::Basic, &SUT_ENVS, &vars).unwrap(),
            Some("Basic YWxpY2U6YQ==".to_owned())
        );
        assert_eq!(
            authorization(AuthKind::Bearer, &ADMIN_ENVS, &vars).unwrap(),
            Some("Bearer t".to_owned())
        );
        assert_eq!(
            authorization(AuthKind::None, &ADMIN_ENVS, &vars).unwrap(),
            None
        );
        assert!(authorization(AuthKind::Basic, &ADMIN_ENVS, &vars).is_err());
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
