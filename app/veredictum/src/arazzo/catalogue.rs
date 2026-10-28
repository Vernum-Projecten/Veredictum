// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Running every document under a catalogue root as one run, with one
//! results document and one HAR recording.
//!
//! The documents are `cases/<capability>/<id>.arazzo.yaml`, run in path
//! order. The shared setups under [`SETUP_DIR`] are not cases: they run only
//! where a case names them in `dependsOn`. Each document runs under its own
//! [`Plan`], built from what the party declares: its statement's `options`
//! select the arm of every option in `capabilities.yaml`, its IXIT and
//! statement answer a rows file's `declared` inputs, and a party that
//! supplies no administrative principal has every workflow that needs one
//! skipped as `not-declared`. The documents share one recording, so an entry
//! index in the results names the same exchange the recording holds, and a
//! re-judgement replays the recording through the documents in the same
//! order. No openEHR spec governs this — our own design.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::arazzo::capabilities::{CAPABILITIES_FILE, Capabilities, CapabilitiesError};
use crate::arazzo::document::{ArazzoDocument, DocumentError};
use crate::arazzo::evaluate::{
    Evaluator, Plan, RunConfig, RunError, recorded_inputs, rejudge_config, replayed_whole,
};
use crate::arazzo::gates::{CASES_DIR, DOCUMENT_SUFFIX};
use crate::arazzo::har::{Har, HarTransport};
use crate::arazzo::principal::{self, Principal};
use crate::arazzo::results::{Engine, Results};
use crate::arazzo::rows::{self, RowsError, rows_path};
use crate::exec::transport::Transport;

/// The capability folder holding the shared setups, which run only through
/// a case's `dependsOn`.
pub const SETUP_DIR: &str = "_setup";

/// The `document` a catalogue run's results carry: every case id in it
/// carries its own document's name.
pub const RUN_NAME: &str = "catalogue";

/// Why a catalogue cannot be run.
#[derive(Debug, thiserror::Error)]
pub enum CatalogueError {
    /// A folder of the catalogue does not list.
    #[error("cannot list {path}: {source}")]
    List {
        /// The folder.
        path: PathBuf,
        /// The filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// The catalogue holds no document to run.
    #[error("{0} holds no `*{DOCUMENT_SUFFIX}` document to run")]
    Empty(PathBuf),
    /// A document named for the run is not in the catalogue.
    #[error("the catalogue holds no document `{0}`")]
    UnknownDocument(String),
    /// A document does not load.
    #[error(transparent)]
    Document(#[from] DocumentError),
    /// A rows file does not load.
    #[error(transparent)]
    Rows(#[from] RowsError),
    /// The capabilities file does not load.
    #[error(transparent)]
    Capabilities(#[from] CapabilitiesError),
    /// The statement's `options` is not a list of option arms.
    #[error("the statement's `options` must be a list of strings: {0}")]
    OptionsShape(String),
    /// The statement selects an arm no option of the catalogue has.
    #[error(
        "the statement selects `{0}`, which is not an arm of any option in {CAPABILITIES_FILE}"
    )]
    UnknownArm(String),
    /// The statement selects two arms of one option.
    #[error("the statement selects both `{first}` and `{second}` of option `{option}`")]
    TwoArms {
        /// The option.
        option: String,
        /// The arm selected first.
        first: String,
        /// The arm selected second.
        second: String,
    },
}

/// What the party declares, which every document's plan reads.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Declarations {
    /// The party's statement; its `options` select the option arms.
    pub statement: Option<Value>,
    /// The party's IXIT, which a rows file's `declared` inputs read.
    pub ixit: Option<Value>,
    /// Whether the party supplies the administrative principal.
    pub administrative: bool,
}

/// One document of the run, with the plan it runs under.
#[derive(Debug)]
pub struct Planned {
    /// The document id, the prefix of its case ids.
    pub name: String,
    /// The document.
    pub document: ArazzoDocument,
    /// What the declarations decide about its workflows.
    pub plan: Plan,
}

/// The documents of the catalogue under `root`, as `(id, path)` in path
/// order, the shared setups left out.
///
/// # Errors
/// [`CatalogueError::List`] when a folder does not list.
pub fn document_paths(root: &Path) -> Result<Vec<(String, PathBuf)>, CatalogueError> {
    let cases = root.join(CASES_DIR);
    let mut found = Vec::new();
    for folder in sorted_entries(&cases)? {
        if !folder.is_dir() || folder.file_name().is_some_and(|name| name == SETUP_DIR) {
            continue;
        }
        for path in sorted_entries(&folder)? {
            let id = path
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .and_then(|name| name.strip_suffix(DOCUMENT_SUFFIX));
            if let Some(id) = id {
                found.push((id.to_owned(), path.clone()));
            }
        }
    }
    Ok(found)
}

fn sorted_entries(folder: &Path) -> Result<Vec<PathBuf>, CatalogueError> {
    let list = |source| CatalogueError::List {
        path: folder.to_owned(),
        source,
    };
    let mut paths = std::fs::read_dir(folder)
        .map_err(list)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(list)?;
    paths.sort();
    Ok(paths)
}

/// The option arms the statement selects, each checked against the options
/// of the catalogue.
///
/// # Errors
/// [`CatalogueError::OptionsShape`] when `options` is not a list of strings,
/// [`CatalogueError::UnknownArm`] for an arm no option has, and
/// [`CatalogueError::TwoArms`] for two arms of one option.
pub fn selected_arms(
    statement: Option<&Value>,
    capabilities: Option<&Capabilities>,
) -> Result<Vec<String>, CatalogueError> {
    let Some(options) = statement.and_then(|statement| statement.get("options")) else {
        return Ok(Vec::new());
    };
    let Value::Array(options) = options else {
        return Err(CatalogueError::OptionsShape(options.to_string()));
    };
    let mut option_of: BTreeMap<&str, &str> = BTreeMap::new();
    for (option, declaration) in capabilities.map(|c| &c.options).into_iter().flatten() {
        for arm in declaration.arms.keys() {
            option_of.insert(arm, option);
        }
    }
    let mut chosen: BTreeMap<&str, &str> = BTreeMap::new();
    let mut selected = Vec::new();
    for entry in options {
        let Value::String(arm) = entry else {
            return Err(CatalogueError::OptionsShape(entry.to_string()));
        };
        let option = option_of
            .get(arm.as_str())
            .ok_or_else(|| CatalogueError::UnknownArm(arm.clone()))?;
        if let Some(first) = chosen.insert(option, arm)
            && first != arm
        {
            return Err(CatalogueError::TwoArms {
                option: (*option).to_owned(),
                first: first.to_owned(),
                second: arm.clone(),
            });
        }
        selected.push(arm.clone());
    }
    Ok(selected)
}

/// The plan `document`, whose id is `name`, runs under.
///
/// It skips the documents of an arm the statement does not select and the
/// workflows of an administrative principal the party does not supply, and
/// carries the declarations and every rows file beside the document.
///
/// # Errors
/// [`CatalogueError::Rows`] when a rows file beside the document does not
/// load.
pub fn plan(
    document: &ArazzoDocument,
    name: &str,
    declarations: &Declarations,
    options: (Option<&Capabilities>, &[String]),
    repo_root: &Path,
) -> Result<Plan, CatalogueError> {
    let (capabilities, selected) = options;
    let mut plan = Plan {
        ixit: declarations.ixit.clone(),
        statement: declarations.statement.clone(),
        ..Plan::default()
    };
    if let Some(skip) = capabilities.and_then(|c| c.skip_for(name, selected, repo_root)) {
        for workflow in &document.description().workflows {
            plan.skips
                .insert(workflow.workflow_id.clone(), skip.clone());
        }
    }
    if !declarations.administrative {
        for (workflow, skip) in
            principal::undeclared_skips(document, Principal::Administrative, repo_root)
        {
            plan.skips.entry(workflow).or_insert(skip);
        }
    }
    for workflow in &document.description().workflows {
        let path = rows_path(document.path(), &workflow.workflow_id);
        if path.is_file() {
            let case_id = format!("{name}/{}", workflow.workflow_id);
            let rows = rows::load(&path, &case_id, workflow)?;
            plan.rows.insert(workflow.workflow_id.clone(), rows);
        }
    }
    Ok(plan)
}

/// The documents a run drives and the option arms it selects, read before
/// any document is loaded.
#[derive(Debug)]
pub struct Selection {
    paths: Vec<(String, PathBuf)>,
    capabilities: Option<Capabilities>,
    selected: Vec<String>,
}

impl Selection {
    /// Selects every document of the catalogue under `root`, or only the
    /// documents `only` names when it names any, and the arms `statement`
    /// selects.
    ///
    /// # Errors
    /// [`CatalogueError`] when a folder does not list, `only` names a
    /// document the catalogue does not hold, nothing is left to run, the
    /// capabilities file does not load, or the statement's options do not
    /// resolve.
    pub fn read(
        root: &Path,
        statement: Option<&Value>,
        only: &[String],
    ) -> Result<Self, CatalogueError> {
        let mut paths = document_paths(root)?;
        let known: BTreeSet<&str> = paths.iter().map(|(id, _)| id.as_str()).collect();
        if let Some(unknown) = only.iter().find(|id| !known.contains(id.as_str())) {
            return Err(CatalogueError::UnknownDocument(unknown.clone()));
        }
        if !only.is_empty() {
            paths.retain(|(id, _)| only.contains(id));
        }
        if paths.is_empty() {
            return Err(CatalogueError::Empty(root.join(CASES_DIR)));
        }
        let file = root.join(CAPABILITIES_FILE);
        let capabilities = if file.is_file() {
            Some(Capabilities::load(&file)?)
        } else {
            None
        };
        let selected = selected_arms(statement, capabilities.as_ref())?;
        Ok(Self {
            paths,
            capabilities,
            selected,
        })
    }

    /// Loads and plans every selected document; `repo_root` is the root
    /// citations are relative to.
    ///
    /// # Errors
    /// [`CatalogueError`] when a document or a rows file does not load.
    pub fn load(
        &self,
        repo_root: &Path,
        declarations: &Declarations,
    ) -> Result<Vec<Planned>, CatalogueError> {
        let mut planned = Vec::new();
        for (name, path) in &self.paths {
            let document = ArazzoDocument::load(path)?;
            let plan = plan(
                &document,
                name,
                declarations,
                (self.capabilities.as_ref(), &self.selected),
                repo_root,
            )?;
            planned.push(Planned {
                name: name.clone(),
                document,
                plan,
            });
        }
        Ok(planned)
    }
}

fn results(base_url: String, rows: Vec<crate::arazzo::results::ResultRow>) -> Results {
    Results {
        document: RUN_NAME.to_owned(),
        engine: Engine {
            name: "veredictum".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        },
        instrument_status: crate::party::InstrumentStatus::CURRENT,
        base_url,
        rows,
    }
}

/// Runs every planned document live, in order, into one recording.
///
/// # Errors
/// [`RunError`] when a document cannot be run at all.
pub fn run(
    planned: &[Planned],
    config: &RunConfig,
    repo_root: &Path,
    transport: &mut dyn Transport,
) -> Result<(Results, Har), RunError> {
    let mut recording = Har::new(&config.base_url);
    recording.log.veredictum.administrative = config.administrative.is_some();
    let mut rows = Vec::new();
    for document in planned {
        let mut evaluator = Evaluator::new(
            &document.document,
            &document.name,
            config.clone(),
            repo_root,
        )?;
        evaluator.continue_recording(recording);
        rows.extend(evaluator.run(&document.plan, transport)?.rows);
        recording = evaluator.into_recording();
    }
    Ok((results(config.base_url.clone(), rows), recording))
}

/// Re-judges a recording of a catalogue run without a server, replaying it
/// through the planned documents in order.
///
/// The documents must be planned as the recorded run planned them: under
/// the same declarations, with [`Declarations::administrative`] read from
/// the recording.
///
/// # Errors
/// [`RunError::Replay`] when the replay does not line up with the
/// recording, and any other [`RunError`] a live run would return.
pub fn rejudge(
    planned: &[Planned],
    repo_root: &Path,
    recording: &Har,
) -> Result<Results, RunError> {
    let mut transport = HarTransport::new(recording);
    let mut inputs = recorded_inputs(recording);
    let mut replayed = Har::new(&recording.log.veredictum.base_url);
    let mut rows = Vec::new();
    for document in planned {
        let config = rejudge_config(recording, None);
        let mut evaluator = Evaluator::new(&document.document, &document.name, config, repo_root)?;
        evaluator.continue_recording(replayed);
        evaluator.replay_inputs(inputs);
        rows.extend(evaluator.run(&document.plan, &mut transport)?.rows);
        inputs = evaluator.unreplayed();
        replayed = evaluator.into_recording();
    }
    replayed_whole(&inputs, &transport)?;
    Ok(results(recording.log.veredictum.base_url.clone(), rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;
    use serde_json::json;

    fn capabilities() -> Capabilities {
        serde_json::from_value(json!({
            "options": {
                "xml-read": {
                    "description": "d",
                    "arms": {
                        "xml-supported": { "documents": ["xml"] },
                        "xml-unsupported": { "documents": ["xml_refused"] }
                    },
                    "x-veredictum-cite": { "source": "specs/openehr/x.md", "section": "s", "quote": "q" }
                },
                "xml-write": {
                    "description": "d",
                    "arms": {
                        "xml-write-accepted": {},
                        "xml-write-refused": { "documents": ["xml_write_refused"] }
                    },
                    "x-veredictum-cite": { "source": "specs/openehr/x.md", "section": "s", "quote": "q" }
                }
            }
        }))
        .unwrap()
    }

    #[test]
    fn the_statement_selects_known_arms_one_per_option() {
        let caps = capabilities();
        let pick =
            |options: Value| selected_arms(Some(&json!({ "options": options })), Some(&caps));
        assert_eq!(
            selected_arms(None, Some(&caps)).unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(
            pick(json!(["xml-supported", "xml-write-accepted"])).unwrap(),
            vec!["xml-supported".to_owned(), "xml-write-accepted".to_owned()]
        );
        assert!(matches!(
            pick(json!(["xml-support"])),
            Err(CatalogueError::UnknownArm(arm)) if arm == "xml-support"
        ));
        assert!(matches!(
            pick(json!(["xml-supported", "xml-unsupported"])),
            Err(CatalogueError::TwoArms { option, .. }) if option == "xml-read"
        ));
        assert!(matches!(
            pick(json!("xml-supported")),
            Err(CatalogueError::OptionsShape(_))
        ));
        assert!(matches!(
            pick(json!([1])),
            Err(CatalogueError::OptionsShape(_))
        ));
        assert!(matches!(
            selected_arms(Some(&json!({ "options": ["xml-supported"] })), None),
            Err(CatalogueError::UnknownArm(_))
        ));
    }

    #[test]
    fn the_documents_are_listed_in_path_order_without_the_setups() {
        let dir = assert_fs::TempDir::new().unwrap();
        for file in [
            "cases/ehr/b.arazzo.yaml",
            "cases/ehr/a.arazzo.yaml",
            "cases/ehr/a-w.rows.yaml",
            "cases/admin/z.arazzo.yaml",
            "cases/_setup/templates.arazzo.yaml",
            "cases/surface.yaml",
        ] {
            dir.child(file).write_str("").unwrap();
        }
        let ids: Vec<String> = document_paths(dir.path())
            .unwrap()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(ids, ["z", "a", "b"]);
        assert!(matches!(
            document_paths(&dir.path().join("absent")),
            Err(CatalogueError::List { .. })
        ));
    }
}
