// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Running every document under a catalogue root as one run, with one
//! results document and one HAR recording.
//!
//! The documents are `cases/<capability>/<id>.arazzo.yaml`, run in path
//! order, in two phases ([`Phase`]): every workflow whose rows file says
//! `server: exclusive` runs after every other workflow of the run. The
//! shared setups under [`SETUP_DIR`] are not cases: they run only where a
//! case names them in `dependsOn`. Each document runs under its own
//! [`Plan`], built from what the party declares: its statement's
//! `claims.capabilities` names the capability folders it claims, and every
//! workflow of a document in another folder is skipped as `not-claimed`; its
//! `options` select the arm of every option in `capabilities.yaml`; its IXIT and
//! statement answer a rows file's `declared` inputs, and a party that
//! supplies no administrative principal has every workflow that needs one
//! skipped as `not-declared`, as does one that supplies no SMART Platform
//! base URL for every workflow with a step sent to it. The documents share
//! one recording, so an entry index in the results names the same exchange
//! the recording holds, and a re-judgement replays the recording through the
//! documents in the same order. No openEHR spec governs this — our own design.

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
use crate::arazzo::platform;
use crate::arazzo::principal::{self, Principal};
use crate::arazzo::results::{Engine, Results, Skip, SkipReason, Why};
use crate::arazzo::rows::{self, RowsError, rows_path};
use crate::exec::transport::Transport;

/// The capability folder holding the shared setups, which run only through
/// a case's `dependsOn`.
pub const SETUP_DIR: &str = "_setup";

/// The `document` a catalogue run's results carry: every case id in it
/// carries its own document's name.
pub const RUN_NAME: &str = "catalogue";

/// The published statement schema, whose `claims.capabilities` clause a
/// `not-claimed` skip cites.
pub const STATEMENT_SCHEMA: &str = "schemas/statement.schema.json";

/// The section of [`STATEMENT_SCHEMA`] a `not-claimed` skip cites.
pub const CLAIMS_SECTION: &str = "#/properties/claims/properties/capabilities";

/// The rule a `not-claimed` skip quotes, verbatim from
/// [`CLAIMS_DESCRIPTION`].
pub const CLAIMS_RULE: &str = "Every workflow of an Arazzo document in a capability folder the statement does not claim is skipped not-claimed.";

/// The description [`STATEMENT_SCHEMA`] publishes for `claims.capabilities`,
/// which carries [`CLAIMS_RULE`]. No openEHR spec governs the claim
/// mechanism — our own design.
pub const CLAIMS_DESCRIPTION: &str = "The capabilities the party claims. For the case-core catalogue each is a capability of the capability matrix; for the Arazzo catalogue each is a capability folder under `catalogue/cases/`, whose name is the folder's. Every workflow of an Arazzo document in a capability folder the statement does not claim is skipped not-claimed.";

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
    /// The statement's `claims.capabilities` is absent, empty, or not a list
    /// of strings.
    #[error(
        "the statement must claim at least one capability folder in `claims.capabilities`, \
         a list of folder names under cases/: {0}"
    )]
    ClaimsShape(String),
    /// The statement claims a capability the catalogue has no folder for.
    #[error("the statement claims `{0}`, which is not a capability folder of the catalogue")]
    UnknownCapability(String),
}

/// What the party declares, which every document's plan reads.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Declarations {
    /// The party's statement; its `options` select the option arms.
    pub statement: Option<Value>,
    /// The party's IXIT, which a rows file's `declared` inputs read.
    pub ixit: Option<Value>,
    /// The environment the party's IXIT declares, which a rows file's
    /// `server` reads.
    pub environment: Option<crate::ixit::DeclaredEnvironment>,
    /// Whether the party supplies the administrative principal.
    pub administrative: bool,
    /// Whether the party supplies the SMART Platform base URL.
    pub platform: bool,
}

/// One document of the run, with the plan it runs under.
#[derive(Debug)]
pub struct Planned {
    /// The document id, the prefix of its case ids.
    pub name: String,
    /// The capability folder the document sits in.
    pub capability: String,
    /// The document.
    pub document: ArazzoDocument,
    /// What the declarations decide about its workflows.
    pub plan: Plan,
}

/// One document of the catalogue, found by its path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentPath {
    /// The document id: the file name without [`DOCUMENT_SUFFIX`].
    pub id: String,
    /// The capability folder the document sits in.
    pub capability: String,
    /// The document file.
    pub path: PathBuf,
}

/// The capability folders under `root`'s `cases/`, the shared setups left
/// out.
///
/// # Errors
/// [`CatalogueError::List`] when `cases/` does not list.
pub fn capability_folders(root: &Path) -> Result<Vec<(String, PathBuf)>, CatalogueError> {
    Ok(sorted_entries(&root.join(CASES_DIR))?
        .into_iter()
        .filter(|folder| folder.is_dir())
        .filter_map(|folder| {
            let name = folder.file_name()?.to_str()?.to_owned();
            (name != SETUP_DIR).then_some((name, folder))
        })
        .collect())
}

/// The documents of the catalogue under `root`, in path order, the shared
/// setups left out.
///
/// # Errors
/// [`CatalogueError::List`] when a folder does not list.
pub fn document_paths(root: &Path) -> Result<Vec<DocumentPath>, CatalogueError> {
    let mut found = Vec::new();
    for (capability, folder) in capability_folders(root)? {
        for path in sorted_entries(&folder)? {
            let id = path
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .and_then(|name| name.strip_suffix(DOCUMENT_SUFFIX));
            if let Some(id) = id {
                found.push(DocumentPath {
                    id: id.to_owned(),
                    capability: capability.clone(),
                    path: path.clone(),
                });
            }
        }
    }
    Ok(found)
}

/// The capability folders the statement claims in `claims.capabilities`,
/// each checked against the folders of the catalogue; `None` without a
/// statement, which claims nothing away.
///
/// # Errors
/// [`CatalogueError::ClaimsShape`] when the statement's
/// `claims.capabilities` is absent, empty or not a list of strings, and
/// [`CatalogueError::UnknownCapability`] for a name no folder has.
pub fn claimed_capabilities(
    statement: Option<&Value>,
    folders: &BTreeSet<String>,
) -> Result<Option<BTreeSet<String>>, CatalogueError> {
    let Some(statement) = statement else {
        return Ok(None);
    };
    let Some(Value::Array(names)) = statement.pointer("/claims/capabilities") else {
        let found = statement
            .pointer("/claims/capabilities")
            .map_or_else(|| "it is absent".to_owned(), Value::to_string);
        return Err(CatalogueError::ClaimsShape(found));
    };
    if names.is_empty() {
        return Err(CatalogueError::ClaimsShape("it is empty".to_owned()));
    }
    let mut claimed = BTreeSet::new();
    for name in names {
        let Value::String(name) = name else {
            return Err(CatalogueError::ClaimsShape(name.to_string()));
        };
        if !folders.contains(name) {
            return Err(CatalogueError::UnknownCapability(name.clone()));
        }
        claimed.insert(name.clone());
    }
    Ok(Some(claimed))
}

/// The skip every workflow of a document in an unclaimed capability folder
/// carries, citing the statement schema's claims clause.
#[must_use]
pub fn not_claimed() -> Skip {
    Skip {
        reason: SkipReason::NotClaimed,
        why: Why {
            source: STATEMENT_SCHEMA.to_owned(),
            section: CLAIMS_SECTION.to_owned(),
            quote: CLAIMS_RULE.to_owned(),
            link: None,
            report: None,
        },
    }
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

/// What the statement decides about one document before it is planned.
#[derive(Debug, Clone, Copy)]
pub struct Choice<'a> {
    /// The catalogue's options, when it has a capabilities file.
    pub capabilities: Option<&'a Capabilities>,
    /// The option arms the statement selects.
    pub arms: &'a [String],
    /// Whether the statement claims the document's capability folder.
    pub claimed: bool,
}

/// The plan `document`, whose id is `name`, runs under.
///
/// It skips the documents of a capability the statement does not claim,
/// then those of an arm it does not select, then the workflows of an
/// administrative principal or a Platform base URL the party does not
/// supply, and carries the declarations and every rows file beside the
/// document. Only the document's own workflows are skipped: a setup it
/// names in `dependsOn` runs whichever folder holds it.
///
/// # Errors
/// [`CatalogueError::Rows`] when a rows file beside the document does not
/// load.
pub fn plan(
    document: &ArazzoDocument,
    name: &str,
    declarations: &Declarations,
    choice: Choice<'_>,
    repo_root: &Path,
) -> Result<Plan, CatalogueError> {
    let mut plan = Plan {
        ixit: declarations.ixit.clone(),
        environment: declarations.environment.clone(),
        statement: declarations.statement.clone(),
        ..Plan::default()
    };
    let unclaimed = (!choice.claimed).then(not_claimed);
    let unselected = choice
        .capabilities
        .and_then(|c| c.skip_for(name, choice.arms, repo_root));
    if let Some(skip) = unclaimed.or(unselected) {
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
    if !declarations.platform {
        for (workflow, skip) in platform::undeclared_skips(document, repo_root) {
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

/// The documents a run drives, the capability folders the statement claims
/// and the option arms it selects, read before any document is loaded.
#[derive(Debug)]
pub struct Selection {
    paths: Vec<DocumentPath>,
    capabilities: Option<Capabilities>,
    selected: Vec<String>,
    claimed: Option<BTreeSet<String>>,
}

impl Selection {
    /// Selects every document of the catalogue under `root`, or only the
    /// documents `only` names when it names any, the capability folders
    /// `statement` claims and the arms it selects.
    ///
    /// # Errors
    /// [`CatalogueError`] when a folder does not list, `only` names a
    /// document the catalogue does not hold, nothing is left to run, the
    /// capabilities file does not load, or the statement's claims or options
    /// do not resolve.
    pub fn read(
        root: &Path,
        statement: Option<&Value>,
        only: &[String],
    ) -> Result<Self, CatalogueError> {
        let mut paths = document_paths(root)?;
        let known: BTreeSet<&str> = paths.iter().map(|found| found.id.as_str()).collect();
        if let Some(unknown) = only.iter().find(|id| !known.contains(id.as_str())) {
            return Err(CatalogueError::UnknownDocument(unknown.clone()));
        }
        if !only.is_empty() {
            paths.retain(|found| only.contains(&found.id));
        }
        if paths.is_empty() {
            return Err(CatalogueError::Empty(root.join(CASES_DIR)));
        }
        let folders: BTreeSet<String> = capability_folders(root)?
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        let claimed = claimed_capabilities(statement, &folders)?;
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
            claimed,
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
        for found in &self.paths {
            let document = ArazzoDocument::load(&found.path)?;
            let choice = Choice {
                capabilities: self.capabilities.as_ref(),
                arms: &self.selected,
                claimed: self
                    .claimed
                    .as_ref()
                    .is_none_or(|claimed| claimed.contains(&found.capability)),
            };
            let plan = plan(&document, &found.id, declarations, choice, repo_root)?;
            planned.push(Planned {
                name: found.id.clone(),
                capability: found.capability.clone(),
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

/// The phase of a catalogue run a workflow runs in.
///
/// A workflow whose rows file says `server: exclusive` changes state it did
/// not create, such as a delete of every EHR, so it runs in
/// [`Phase::Exclusive`], which starts once every [`Phase::Shared`] workflow
/// of every document has finished. Nothing set up is carried across
/// workflows: each workflow runs its own `dependsOn` setups, so no setup
/// result is reused after an exclusive workflow has run. No openEHR spec
/// governs this — our own design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    /// Every workflow that leaves the state it did not create alone.
    Shared,
    /// Every workflow whose rows file says `server: exclusive`.
    Exclusive,
}

impl Phase {
    /// Every phase, in the order a run drives them.
    pub const ORDER: [Self; 2] = [Self::Shared, Self::Exclusive];

    /// The phase `workflow_id` of a document planned as `plan` runs in.
    #[must_use]
    pub fn of(plan: &Plan, workflow_id: &str) -> Self {
        if plan.is_exclusive(workflow_id) {
            Self::Exclusive
        } else {
            Self::Shared
        }
    }
}

/// The order a run drives the planned documents in: phase by phase, each
/// document in path order, and only the documents with a workflow in the
/// phase. A live run and its re-judgement both follow it, so the
/// re-judgement replays the recording in the order it was recorded.
fn drive_order(planned: &[Planned]) -> Vec<(Phase, &Planned)> {
    Phase::ORDER
        .into_iter()
        .flat_map(|phase| {
            planned
                .iter()
                .filter(move |document| {
                    document
                        .document
                        .description()
                        .workflows
                        .iter()
                        .any(|w| Phase::of(&document.plan, &w.workflow_id) == phase)
                })
                .map(move |document| (phase, document))
        })
        .collect()
}

/// Runs every planned document live, in [`Phase`] order, into one
/// recording.
///
/// # Errors
/// [`RunError`] when a document cannot be run at all.
pub fn run(
    planned: &[Planned],
    config: &RunConfig,
    repo_root: &Path,
    transport: &mut dyn Transport,
) -> Result<(Results, Har), RunError> {
    let mut recording = config.recording();
    let mut rows = Vec::new();
    for (phase, document) in drive_order(planned) {
        let mut evaluator = Evaluator::new(
            &document.document,
            &document.name,
            config.clone(),
            repo_root,
        )?;
        evaluator.continue_recording(recording);
        let plan = &document.plan;
        let results = evaluator.run_where(plan, transport, |w| {
            Phase::of(plan, &w.workflow_id) == phase
        })?;
        rows.extend(results.rows);
        recording = evaluator.into_recording();
    }
    Ok((results(config.base_url.clone(), rows), recording))
}

/// Re-judges a recording of a catalogue run without a server, replaying it
/// through the planned documents in the [`Phase`] order the run drove.
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
    for (phase, document) in drive_order(planned) {
        let config = rejudge_config(recording, None);
        let mut evaluator = Evaluator::new(&document.document, &document.name, config, repo_root)?;
        evaluator.continue_recording(replayed);
        evaluator.replay_inputs(inputs);
        let plan = &document.plan;
        let results = evaluator.run_where(plan, &mut transport, |w| {
            Phase::of(plan, &w.workflow_id) == phase
        })?;
        rows.extend(results.rows);
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
        let found: Vec<(String, String)> = document_paths(dir.path())
            .unwrap()
            .into_iter()
            .map(|found| (found.capability, found.id))
            .collect();
        let pairs = [("admin", "z"), ("ehr", "a"), ("ehr", "b")]
            .map(|(capability, id)| (capability.to_owned(), id.to_owned()));
        assert_eq!(found, pairs);
        assert!(matches!(
            document_paths(&dir.path().join("absent")),
            Err(CatalogueError::List { .. })
        ));
    }

    #[test]
    fn the_statement_claims_known_capability_folders() {
        let folders: BTreeSet<String> = ["ehr", "demographic"].map(str::to_owned).into();
        let claim =
            |claims: Value| claimed_capabilities(Some(&json!({ "claims": claims })), &folders);
        assert_eq!(claimed_capabilities(None, &folders).unwrap(), None);
        assert_eq!(
            claim(json!({ "capabilities": ["ehr"] })).unwrap(),
            Some(["ehr".to_owned()].into())
        );
        assert!(matches!(
            claim(json!({ "capabilities": ["ehr", "EhrOperations"] })),
            Err(CatalogueError::UnknownCapability(name)) if name == "EhrOperations"
        ));
        assert!(matches!(
            claim(json!({ "capabilities": ["_setup"] })),
            Err(CatalogueError::UnknownCapability(_))
        ));
        for refused in [
            json!({}),
            json!({ "capabilities": [] }),
            json!({ "capabilities": "ehr" }),
            json!({ "capabilities": [1] }),
        ] {
            assert!(
                matches!(claim(refused.clone()), Err(CatalogueError::ClaimsShape(_))),
                "{refused}"
            );
        }
        assert!(matches!(
            claimed_capabilities(Some(&json!({ "options": [] })), &folders),
            Err(CatalogueError::ClaimsShape(_))
        ));
    }

    #[test]
    fn only_a_rows_file_saying_exclusive_puts_a_workflow_in_the_last_phase() {
        let file = |server: Value| {
            serde_json::from_value::<crate::arazzo::gates::rows::RowsFile>(json!({
                "workflow": "d/w", "iteration": "reset_per_row", "server": server, "rows": [{}]
            }))
            .unwrap()
        };
        let mut plan = Plan::default();
        plan.rows
            .insert("exclusive".to_owned(), file(json!("exclusive")));
        plan.rows.insert("shared".to_owned(), file(Value::Null));
        assert_eq!(Phase::of(&plan, "exclusive"), Phase::Exclusive);
        assert_eq!(Phase::of(&plan, "shared"), Phase::Shared);
        assert_eq!(Phase::of(&plan, "no_rows_file"), Phase::Shared);
        assert_eq!(Phase::ORDER, [Phase::Shared, Phase::Exclusive]);
    }

    #[test]
    fn the_not_claimed_skip_quotes_the_published_statement_schema() {
        assert!(CLAIMS_DESCRIPTION.contains(CLAIMS_RULE));
        let skip = not_claimed();
        assert_eq!(skip.reason, SkipReason::NotClaimed);
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let schema: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join(&skip.why.source)).unwrap())
                .unwrap();
        let described = schema
            .pointer(skip.why.section.trim_start_matches('#'))
            .and_then(|clause| clause.get("description"))
            .and_then(Value::as_str)
            .unwrap();
        assert!(described.contains(&skip.why.quote), "{described}");
    }
}
