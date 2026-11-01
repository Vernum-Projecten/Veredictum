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
//! `options` select the arm of every option in `capabilities.yaml`; its
//! `spec_versions` and `tech_profiles` skip a document whose `applies` range
//! it is outside of as `version-out-of-range`, and one needing a format it
//! does not claim as `format-not-claimed`; its IXIT and
//! statement answer a rows file's `declared` inputs, and a party that
//! supplies no administrative principal has every workflow that needs one
//! skipped as `not-declared`, as does one that supplies no SMART Platform
//! base URL for every workflow with a step sent to it. The documents share
//! one recording, so an entry index in the results names the same exchange
//! the recording holds, and a re-judgement replays the recording through the
//! documents in the same order. No openEHR spec governs this — our own design.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use reqwest::StatusCode;
use serde_json::Value;

use crate::arazzo::capabilities::{
    CAPABILITIES_FILE, Capabilities, CapabilitiesError, SpecVersion,
};
use crate::arazzo::cite::{CiteError, QuoteChecker};
use crate::arazzo::control;
use crate::arazzo::document::{ArazzoDocument, DocumentError};
use crate::arazzo::evaluate::{
    Evaluator, Plan, RunConfig, RunError, recorded_inputs, rejudge_config, replayed_whole,
};
use crate::arazzo::gates::{CASES_DIR, DOCUMENT_SUFFIX};
use crate::arazzo::har::{Har, HarTransport};
use crate::arazzo::platform;
use crate::arazzo::principal::{self, Principal};
use crate::arazzo::results::{Engine, Results, ServerIdentity, Skip, SkipReason, Why};
use crate::arazzo::rows::{self, RowsError, rows_path};
use crate::arazzo::spec_profile::SpecProfile;
use crate::arazzo::structure::{
    self, CHAPTERS_FILE, Chapters, Outline, Place, Placer, REPORTS_FILE, Reports, StructureError,
};
use crate::exec::transport::Transport;
use crate::identity;
use crate::party::TechProfile;
use crate::vocab::{FormatName, ItsName, SpecComponent};

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
pub const CLAIMS_DESCRIPTION: &str = "The capabilities the party claims, at least one: each is a capability folder under `catalogue/cases/`, whose name is the folder's. Every workflow of an Arazzo document in a capability folder the statement does not claim is skipped not-claimed.";

/// The section of [`STATEMENT_SCHEMA`] a `format-not-claimed` skip cites.
pub const FORMATS_SECTION: &str = "#/properties/tech_profiles";

/// The rule a `format-not-claimed` skip quotes, verbatim from
/// [`FORMATS_DESCRIPTION`].
pub const FORMATS_RULE: &str = "Every workflow of an Arazzo document that needs a wire format the statement's its-rest profile does not list is skipped format-not-claimed.";

/// The description [`STATEMENT_SCHEMA`] publishes for `tech_profiles`.
///
/// It carries [`FORMATS_RULE`]. ITS-REST `docs/overview/Resources.md` §Data
/// representation requires one of the canonical formats and leaves the rest
/// to the service, so which formats a party serves is its own claim.
pub const FORMATS_DESCRIPTION: &str = "The technology profiles the party claims, each an ITS with the wire formats the party serves on it. A statement without an its-rest profile claims every format. Every workflow of an Arazzo document that needs a wire format the statement's its-rest profile does not list is skipped format-not-claimed.";

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
    /// The statement declares a version of a component an `applies` range
    /// bounds, and the version is not `MAJOR.MINOR.PATCH`.
    #[error(
        "the statement's `spec_versions.{component}` must be a release version \
         MAJOR.MINOR.PATCH, since {CAPABILITIES_FILE} bounds documents by it: {found}"
    )]
    VersionShape {
        /// The component, as `spec_versions` keys it.
        component: &'static str,
        /// What the statement declares.
        found: String,
    },
    /// The statement's `tech_profiles` is not a list of technology profiles.
    #[error("the statement's `tech_profiles` is not a list of technology profiles: {0}")]
    TechProfilesShape(String),
    /// The reading structure does not load, or does not place a document.
    #[error(transparent)]
    Structure(#[from] StructureError),
    /// A citation of a case is refused under the run's profile for a reason
    /// other than the absence of its text.
    #[error("`{case}`: {error}")]
    Citation {
        /// The case id.
        case: String,
        /// The refusal.
        #[source]
        error: CiteError,
    },
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
    /// The specification profile whose text the run's citations are read
    /// against; a workflow citing text the profile lacks is skipped as
    /// `cited-text-absent`, under the released profile a workflow
    /// resting on development text alone is skipped as `development-only`,
    /// and under the development profile a workflow whose cited text the
    /// snapshot contests is skipped as `contested-in-development`.
    pub profile: SpecProfile,
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
    /// Where each of its workflows sits in the reading structure, by
    /// workflow id; empty when the catalogue declares no structure.
    pub places: BTreeMap<String, Place>,
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
        detail: None,
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

/// The skip every workflow of a document needing a format the statement does
/// not claim carries, citing the statement schema's `tech_profiles` clause.
#[must_use]
pub fn format_not_claimed() -> Skip {
    Skip {
        detail: None,
        reason: SkipReason::FormatNotClaimed,
        why: Why {
            source: STATEMENT_SCHEMA.to_owned(),
            section: FORMATS_SECTION.to_owned(),
            quote: FORMATS_RULE.to_owned(),
            link: None,
            report: None,
        },
    }
}

/// The versions the statement's `spec_versions` declares of `components`;
/// a component it does not declare is left out, and so is every component
/// without a statement.
///
/// # Errors
/// [`CatalogueError::VersionShape`] when a declared version of one of
/// `components` is not `MAJOR.MINOR.PATCH`.
pub fn declared_versions(
    statement: Option<&Value>,
    components: &BTreeSet<SpecComponent>,
) -> Result<BTreeMap<SpecComponent, SpecVersion>, CatalogueError> {
    let mut declared = BTreeMap::new();
    let Some(versions) = statement.and_then(|s| s.get("spec_versions")) else {
        return Ok(declared);
    };
    for component in components {
        let Some(found) = versions.get(component.token()) else {
            continue;
        };
        let shape = || CatalogueError::VersionShape {
            component: component.token(),
            found: found.to_string(),
        };
        let version = found
            .as_str()
            .ok_or_else(shape)?
            .parse::<SpecVersion>()
            .map_err(|_not_a_version| shape())?;
        declared.insert(*component, version);
    }
    Ok(declared)
}

/// The wire formats the statement's its-rest profiles claim; `None` without
/// a statement, or with one that declares no its-rest profile, which claims
/// every format.
///
/// # Errors
/// [`CatalogueError::TechProfilesShape`] when `tech_profiles` is not a list
/// of technology profiles.
pub fn claimed_formats(
    statement: Option<&Value>,
) -> Result<Option<BTreeSet<FormatName>>, CatalogueError> {
    let Some(profiles) = statement.and_then(|s| s.get("tech_profiles")) else {
        return Ok(None);
    };
    let profiles: Vec<TechProfile> = serde_json::from_value(profiles.clone())
        .map_err(|error| CatalogueError::TechProfilesShape(error.to_string()))?;
    let mut rest = profiles
        .into_iter()
        .filter(|profile| profile.its == ItsName::ItsRest)
        .peekable();
    // NOTE: a statement naming no its-rest profile records every format as
    // `defaulted` in the results too; no openEHR spec governs this — our own design.
    if rest.peek().is_none() {
        return Ok(None);
    }
    Ok(Some(rest.flat_map(|profile| profile.formats).collect()))
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
    /// The versions the statement declares of every component an `applies`
    /// range bounds.
    pub versions: &'a BTreeMap<SpecComponent, SpecVersion>,
    /// The formats the statement claims; `None` claims every format.
    pub formats: Option<&'a BTreeSet<FormatName>>,
}

impl Choice<'_> {
    /// The skip every workflow of the document `name` carries, from what the
    /// statement alone decides: an unclaimed capability, then an unselected
    /// arm, then a declared version outside the document's range, then a
    /// format the statement does not claim; `None` when the document runs.
    #[must_use]
    pub fn skip(&self, name: &str, repo_root: &Path) -> Option<Skip> {
        if !self.claimed {
            return Some(not_claimed());
        }
        let capabilities = self.capabilities?;
        if let Some(skip) = capabilities.skip_for(name, self.arms, repo_root) {
            return Some(skip);
        }
        if let Some(skip) = capabilities.version_skip(name, self.versions, repo_root) {
            return Some(skip);
        }
        let unclaimed = self
            .formats
            .and_then(|formats| capabilities.unclaimed_format(name, formats));
        unclaimed.map(|_| format_not_claimed())
    }
}

/// The plan `document`, whose id is `name`, runs under.
///
/// It skips the documents of a capability the statement does not claim,
/// then those of an arm it does not select, a version range the statement
/// is outside of, or a format it does not claim, then the workflows of an
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
    if let Some(skip) = choice.skip(name, repo_root) {
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

/// The `cited-text-absent` skips of the workflows of `document` whose cited
/// text the profile of `quotes` lacks.
///
/// `name` is the document id. Each skip carries the workflow's first such
/// citation, with what the profile lacks.
///
/// A workflow's citations are its own, its steps' and its criteria's, the
/// ones its verdict rests on. A setup it names in `dependsOn` is judged by
/// its own citations wherever it runs as a case.
///
/// # Errors
/// [`CatalogueError::Citation`] when a citation is refused for a reason
/// other than the absence of its text, such as a source that does not parse
/// or a document the profile states no status for.
pub fn absent_text_skips(
    document: &ArazzoDocument,
    name: &str,
    quotes: &mut QuoteChecker<'_>,
    repo_root: &Path,
) -> Result<Vec<(String, Skip)>, CatalogueError> {
    let profile = quotes.profile();
    let mut skips = Vec::new();
    for workflow in &document.description().workflows {
        for citation in structure::workflow_citations(workflow) {
            match quotes.check(&citation) {
                Ok(()) => {}
                Err(error) if error.is_absence() => {
                    skips.push((
                        workflow.workflow_id.clone(),
                        Skip {
                            reason: SkipReason::CitedTextAbsent,
                            why: Why::of(&citation, repo_root),
                            detail: Some(format!("{profile} profile: {error}")),
                        },
                    ));
                    break;
                }
                Err(error) => {
                    return Err(CatalogueError::Citation {
                        case: format!("{name}/{}", workflow.workflow_id),
                        error,
                    });
                }
            }
        }
    }
    Ok(skips)
}

/// The `development-only` skips of the workflows of `document` that rest on
/// text only the development snapshot holds, which a released run never
/// runs.
///
/// Each skip carries the workflow's first development-only citation, and its
/// detail names it. No openEHR spec governs this — our own design: a
/// development result never counts toward a released one.
#[must_use]
pub fn development_only_skips(document: &ArazzoDocument, repo_root: &Path) -> Vec<(String, Skip)> {
    document
        .description()
        .workflows
        .iter()
        .filter_map(|workflow| {
            let citation = structure::development_citation(workflow)?;
            Some((
                workflow.workflow_id.clone(),
                Skip {
                    reason: SkipReason::DevelopmentOnly,
                    why: Why::of(&citation, repo_root),
                    detail: Some(format!(
                        "{} profile: the case rests on development text alone, `{}` ({}), and runs under the {} profile only",
                        SpecProfile::Released,
                        citation.source,
                        citation.section,
                        SpecProfile::Development
                    )),
                },
            ))
        })
        .collect()
}

/// The `contested-in-development` skips of the workflows of `document`
/// whose cited text the development snapshot keeps unchanged and contests,
/// which a development run never grades.
///
/// Each skip carries the workflow's first contested citation, and its detail
/// names the upstream report. No openEHR spec governs this — our own design:
/// a development run grades no server on one side of a contradiction between
/// two development texts.
#[must_use]
pub fn contested_skips(document: &ArazzoDocument, repo_root: &Path) -> Vec<(String, Skip)> {
    document
        .description()
        .workflows
        .iter()
        .filter_map(|workflow| {
            let citation = structure::contested_citation(workflow)?;
            let report = citation.contested()?;
            Some((
                workflow.workflow_id.clone(),
                Skip {
                    reason: SkipReason::ContestedInDevelopment,
                    why: Why::of(&citation, repo_root),
                    detail: Some(format!(
                        "{} profile: the snapshot keeps `{}` ({}) unchanged and contradicts it elsewhere, as upstream report #{report} records, and the case runs under the {} profile only",
                        SpecProfile::Development,
                        citation.source,
                        citation.section,
                        SpecProfile::Released
                    )),
                },
            ))
        })
        .collect()
}

/// The documents a run drives, the capability folders the statement claims
/// and the option arms it selects, read before any document is loaded.
#[derive(Debug)]
pub struct Selection {
    paths: Vec<DocumentPath>,
    capabilities: Option<Capabilities>,
    selected: Vec<String>,
    claimed: Option<BTreeSet<String>>,
    versions: BTreeMap<SpecComponent, SpecVersion>,
    formats: Option<BTreeSet<FormatName>>,
    chapters: Option<Chapters>,
    reports: Option<Reports>,
    folder_of: BTreeMap<String, String>,
}

impl Selection {
    /// The documents the run drives, in path order.
    #[must_use]
    pub fn paths(&self) -> &[DocumentPath] {
        &self.paths
    }

    /// The capability folders the statement claims; `None` without a
    /// statement.
    #[must_use]
    pub fn claimed(&self) -> Option<&BTreeSet<String>> {
        self.claimed.as_ref()
    }

    /// Every document of a selected option arm that needs a format the
    /// statement does not claim, as `(arm, document, format)`; empty when the
    /// statement claims every format.
    ///
    /// Selecting an arm declares the behaviour its documents test, so an arm
    /// whose documents the statement's own formats would skip declares a
    /// behaviour nothing verifies. No openEHR spec governs this — our own
    /// design.
    #[must_use]
    pub fn arms_needing_unclaimed_formats(&self) -> Vec<(&str, &str, FormatName)> {
        let (Some(capabilities), Some(claimed)) = (&self.capabilities, &self.formats) else {
            return Vec::new();
        };
        capabilities
            .arm_documents()
            .filter(|(_, arm, _)| self.selected.iter().any(|selected| selected == arm))
            .filter_map(|(_, arm, document)| {
                capabilities
                    .unclaimed_format(document, claimed)
                    .map(|format| (arm, document, format))
            })
            .collect()
    }

    /// What the statement decides about the document `found`.
    #[must_use]
    pub fn choice(&self, found: &DocumentPath) -> Choice<'_> {
        Choice {
            capabilities: self.capabilities.as_ref(),
            arms: &self.selected,
            claimed: self
                .claimed
                .as_ref()
                .is_none_or(|claimed| claimed.contains(&found.capability)),
            versions: &self.versions,
            formats: self.formats.as_ref(),
        }
    }

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
        let folder_of: BTreeMap<String, String> = paths
            .iter()
            .map(|found| (found.id.clone(), found.capability.clone()))
            .collect();
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
        let ranged = capabilities
            .as_ref()
            .map(Capabilities::ranged_components)
            .unwrap_or_default();
        let versions = declared_versions(statement, &ranged)?;
        let formats = claimed_formats(statement)?;
        let chapters_file = root.join(CHAPTERS_FILE);
        let chapters = if chapters_file.is_file() {
            Some(Chapters::load(&chapters_file)?)
        } else {
            None
        };
        let reports_file = root.join(REPORTS_FILE);
        let reports = if reports_file.is_file() {
            Some(Reports::load(&reports_file)?)
        } else {
            None
        };
        Ok(Self {
            paths,
            capabilities,
            selected,
            claimed,
            versions,
            formats,
            chapters,
            reports,
            folder_of,
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
        let mut structure = self.chapters.as_ref().map(|chapters| {
            Placer::in_profile(
                chapters,
                self.capabilities.as_ref(),
                repo_root,
                declarations.profile,
            )
        });
        let mut quotes = (declarations.profile != SpecProfile::Released)
            .then(|| QuoteChecker::in_profile(repo_root, declarations.profile));
        for found in &self.paths {
            let document = ArazzoDocument::load(&found.path)?;
            let mut plan = plan(
                &document,
                &found.id,
                declarations,
                self.choice(found),
                repo_root,
            )?;
            if declarations.profile == SpecProfile::Development {
                for (workflow, skip) in contested_skips(&document, repo_root) {
                    plan.skips.entry(workflow).or_insert(skip);
                }
            }
            if let Some(quotes) = quotes.as_mut() {
                for (workflow, skip) in absent_text_skips(&document, &found.id, quotes, repo_root)?
                {
                    plan.skips.entry(workflow).or_insert(skip);
                }
            } else {
                for (workflow, skip) in development_only_skips(&document, repo_root) {
                    plan.skips.entry(workflow).or_insert(skip);
                }
            }
            let places = match structure.as_mut() {
                Some(placer) => placer.place(&document, &found.id, &found.capability)?,
                None => BTreeMap::new(),
            };
            planned.push(Planned {
                name: found.id.clone(),
                capability: found.capability.clone(),
                document,
                plan,
                places,
            });
        }
        Ok(planned)
    }

    /// The reading structure the results of `planned` carry: the declared
    /// chapters, every option with the arms the statement selects, every
    /// recorded report, and where each planned case sits; `None` when the
    /// catalogue declares no structure.
    #[must_use]
    pub fn outline(&self, planned: &[Planned]) -> Option<Outline> {
        let chapters = self.chapters.as_ref()?;
        let cases = planned
            .iter()
            .flat_map(|document| {
                document.places.iter().map(|(workflow, place)| {
                    (format!("{}/{workflow}", document.name), place.clone())
                })
            })
            .collect();
        Some(structure::outline(
            chapters,
            self.capabilities.as_ref(),
            self.reports.as_ref(),
            &self.folder_of,
            &self.selected,
            cases,
        ))
    }
}

fn results(
    base_url: String,
    server: ServerIdentity,
    rows: Vec<crate::arazzo::results::ResultRow>,
) -> Results {
    Results {
        document: RUN_NAME.to_owned(),
        engine: Engine {
            name: "veredictum".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        },
        instrument_status: crate::party::InstrumentStatus::CURRENT,
        specification: crate::arazzo::spec_profile::Specification::Released,
        base_url,
        server: Some(server),
        review: Vec::new(),
        rows,
        outline: None,
    }
}

/// The System OPTIONS answer a recording carries: the first entry the run
/// exchanged with `OPTIONS` on the API root that received a response, else
/// the read the log carries.
///
/// A live run and its re-judgement read the same recording, so both record
/// the same identity.
#[must_use]
pub fn recorded_identity(recording: &Har) -> Option<ServerIdentity> {
    let root = format!(
        "{}{}",
        recording.log.veredictum.base_url.trim_end_matches('/'),
        identity::PATH
    );
    let exchanged = recording
        .log
        .entries
        .iter()
        .enumerate()
        .find_map(|(entry, exchange)| {
            if exchange.request.method != "OPTIONS" || exchange.request.url != root {
                return None;
            }
            // NOTE: no openEHR spec governs this — our own design; status 0
            // records a transport failure, which is no answer to classify.
            let status = StatusCode::from_u16(exchange.response.status).ok()?;
            Some(ServerIdentity::Exchange {
                entry,
                answer: identity::classify(status, &exchange.response.content.text),
            })
        });
    exchanged.or_else(|| recording.log.veredictum.identity.clone())
}

/// Reads the server's identity after a live run that exchanged no System
/// OPTIONS, with the ordinary principal's credential when the run has one,
/// and records the read in the recording's log so a re-judgement reads it
/// back.
fn read_identity(
    config: &RunConfig,
    transport: &mut dyn Transport,
    recording: &mut Har,
) -> ServerIdentity {
    let read = match identity::read(transport, &config.base_url, config.authorization.as_deref()) {
        Ok(answered) => ServerIdentity::Read {
            answer: answered.read,
        },
        Err(error) => ServerIdentity::Unanswered {
            error: error.to_string(),
        },
    };
    recording.log.veredictum.identity = Some(read.clone());
    read
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
/// The results record the server's identity from the recording
/// ([`recorded_identity`]). When no workflow exchanged System OPTIONS, the
/// run reads it once after the catalogue, through `transport` with the
/// ordinary principal's credential, and keeps that read in the recording's
/// log.
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
    control::apply(&mut rows, &run_controls(planned));
    let server = match recorded_identity(&recording) {
        Some(server) => server,
        None => read_identity(config, transport, &mut recording),
    };
    Ok((results(config.base_url.clone(), server, rows), recording))
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
    control::apply(&mut rows, &run_controls(planned));
    let server = recorded_identity(recording).unwrap_or(ServerIdentity::NotRecorded);
    Ok(results(
        recording.log.veredictum.base_url.clone(),
        server,
        rows,
    ))
}

/// The control case of every refusal case of the planned documents, so a
/// refusal row is judged against its control wherever in the run it ran.
fn run_controls(planned: &[Planned]) -> BTreeMap<String, control::Control> {
    planned
        .iter()
        .flat_map(|document| control::controls(&document.document, &document.name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;
    use serde_json::json;

    fn exchanged(har: &mut Har, method: &str, url: &str, outcome: Result<(u16, &str), &str>) {
        let raw = outcome
            .ok()
            .map(|(status, text)| crate::exec::transport::RawResponse {
                status: StatusCode::from_u16(status).unwrap(),
                headers: BTreeMap::new(),
                text: text.to_owned(),
            });
        let page = har.begin_page(crate::arazzo::har::PageContext {
            case: "system_options/options".to_owned(),
            row: 0,
            role: crate::arazzo::har::RunRole::Row,
            inputs: rows::Row::new(),
        });
        har.record(&crate::arazzo::har::Recorded {
            page: &page,
            step: "options",
            method,
            url,
            request_headers: &BTreeMap::new(),
            query: &BTreeMap::new(),
            body: None,
            outcome: raw
                .as_ref()
                .ok_or_else(|| outcome.err().unwrap_or_default()),
            millis: 1.0,
        });
    }

    #[test]
    fn the_first_answered_system_options_exchange_is_the_recorded_identity() {
        let mut har = Har::new("http://sut/v1/");
        assert_eq!(recorded_identity(&har), None);
        exchanged(&mut har, "OPTIONS", "http://sut/v1/", Err("refused"));
        exchanged(&mut har, "OPTIONS", "http://sut/v1/ehr", Ok((200, "{}")));
        exchanged(&mut har, "GET", "http://sut/v1/", Ok((200, "{}")));
        assert_eq!(recorded_identity(&har), None);
        har.log.veredictum.identity = Some(ServerIdentity::Unanswered {
            error: "refused".to_owned(),
        });
        assert_eq!(
            recorded_identity(&har),
            har.log.veredictum.identity.clone(),
            "without an exchange, the read the log carries is the identity"
        );
        let stated = r#"{"restapi_specs_version":"1.0.3"}"#;
        exchanged(&mut har, "OPTIONS", "http://sut/v1/", Ok((200, stated)));
        exchanged(&mut har, "OPTIONS", "http://sut/v1/", Ok((404, "")));
        let identity = recorded_identity(&har).unwrap();
        assert_eq!(
            identity,
            ServerIdentity::Exchange {
                entry: 3,
                answer: identity::classify(StatusCode::OK, stated),
            }
        );
        assert_eq!(identity.restapi_specs_version(), Some("1.0.3"));
    }

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

    #[test]
    fn the_format_not_claimed_skip_quotes_the_published_statement_schema() {
        assert!(FORMATS_DESCRIPTION.contains(FORMATS_RULE));
        let skip = format_not_claimed();
        assert_eq!(skip.reason, SkipReason::FormatNotClaimed);
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

    #[test]
    fn the_statement_declares_versions_only_of_ranged_components() {
        let ranged = BTreeSet::from([SpecComponent::ItsRest]);
        let statement = json!({ "spec_versions": { "its_rest": "1.0.2", "rm": "development" } });
        let declared = declared_versions(Some(&statement), &ranged).unwrap();
        assert_eq!(
            declared
                .get(&SpecComponent::ItsRest)
                .map(ToString::to_string),
            Some("1.0.2".to_owned())
        );
        assert_eq!(declared.len(), 1);
        assert!(declared_versions(None, &ranged).unwrap().is_empty());
        assert!(
            declared_versions(Some(&json!({})), &ranged)
                .unwrap()
                .is_empty()
        );
        for refused in [json!("1.1"), json!(1.1), json!("latest")] {
            assert!(
                matches!(
                    declared_versions(
                        Some(&json!({ "spec_versions": { "its_rest": refused } })),
                        &ranged
                    ),
                    Err(CatalogueError::VersionShape {
                        component: "its_rest",
                        ..
                    })
                ),
                "{refused}"
            );
        }
    }

    #[test]
    fn the_statement_claims_the_formats_of_its_its_rest_profile() {
        assert_eq!(claimed_formats(None).unwrap(), None);
        assert_eq!(claimed_formats(Some(&json!({}))).unwrap(), None);
        assert_eq!(
            claimed_formats(Some(&json!({ "tech_profiles": [] }))).unwrap(),
            None
        );
        assert_eq!(
            claimed_formats(Some(&json!({ "tech_profiles": [
                { "its": "its-rest", "formats": ["canonical-json", "wt-flat"] }
            ] })))
            .unwrap(),
            Some(BTreeSet::from([
                FormatName::CanonicalJson,
                FormatName::WtFlat
            ]))
        );
        assert_eq!(
            claimed_formats(Some(&json!({ "tech_profiles": [{ "its": "its-rest" }] }))).unwrap(),
            Some(BTreeSet::new())
        );
        for refused in [
            json!({ "tech_profiles": "its-rest" }),
            json!({ "tech_profiles": [{ "its": "its-rest", "formats": ["yaml"] }] }),
            json!({ "tech_profiles": [{ "its": "its-soap" }] }),
        ] {
            assert!(
                matches!(
                    claimed_formats(Some(&refused)),
                    Err(CatalogueError::TechProfilesShape(_))
                ),
                "{refused}"
            );
        }
    }

    /// A catalogue root holding the committed System document under a
    /// capabilities file that gives it `applies` and `formats`, with the
    /// vendored specs linked where the document's sources resolve.
    fn needs_world() -> assert_fs::TempDir {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dir = assert_fs::TempDir::new().unwrap();
        std::os::unix::fs::symlink(repo.join("specs"), dir.path().join("specs")).unwrap();
        dir.child("catalogue/cases/system/system_options.arazzo.yaml")
            .write_str(
                &std::fs::read_to_string(
                    repo.join("catalogue/cases/system/system_options.arazzo.yaml"),
                )
                .unwrap(),
            )
            .unwrap();
        dir.child("catalogue/capabilities.yaml")
            .write_str(
                "documents:\n  system_options:\n    applies:\n      its_rest:\n        min: 1.1.0\n        x-veredictum-cite: { source: specs/openehr/ITS-REST/specifications/docs/overview/Amendment_record.md, section: Release-1.1.0, quote: q }\n    formats: [canonical-json]\n",
            )
            .unwrap();
        dir
    }

    /// The skip of every workflow of the System document under `statement`.
    fn system_skips(statement: &Value) -> Vec<Option<Skip>> {
        let world = needs_world();
        let selection =
            Selection::read(&world.path().join("catalogue"), Some(statement), &[]).unwrap();
        let planned = selection
            .load(world.path(), &Declarations::default())
            .unwrap();
        assert_eq!(planned.len(), 1);
        let system = planned.first().unwrap();
        system
            .document
            .description()
            .workflows
            .iter()
            .map(|w| system.plan.skips.get(&w.workflow_id).cloned())
            .collect()
    }

    #[test]
    fn a_document_runs_inside_its_version_range_and_is_skipped_outside_it() {
        let claims = json!({ "capabilities": ["system"] });
        let inside =
            system_skips(&json!({ "claims": claims, "spec_versions": { "its_rest": "1.1.0" } }));
        assert_eq!(inside, vec![None, None, None]);
        let undeclared =
            system_skips(&json!({ "claims": claims, "spec_versions": { "rm": "1.0.0" } }));
        assert_eq!(undeclared, vec![None, None, None]);
        let outside =
            system_skips(&json!({ "claims": claims, "spec_versions": { "its_rest": "1.0.2" } }));
        assert_eq!(outside.len(), 3);
        for skip in outside {
            let skip = skip.unwrap();
            assert_eq!(skip.reason, SkipReason::VersionOutOfRange);
            assert!(skip.why.source.ends_with("Amendment_record.md"));
            assert_eq!(skip.why.section, "Release-1.1.0");
        }
    }

    #[test]
    fn a_document_runs_when_its_format_is_claimed_and_is_skipped_when_not() {
        let claims = json!({ "capabilities": ["system"] });
        let profile = |formats: Value| json!({ "claims": claims, "tech_profiles": [{ "its": "its-rest", "formats": formats }] });
        let claimed = system_skips(&profile(json!(["canonical-json"])));
        assert_eq!(claimed, vec![None, None, None]);
        let defaulted = system_skips(&json!({ "claims": claims }));
        assert_eq!(defaulted, vec![None, None, None]);
        let unclaimed = system_skips(&profile(json!(["canonical-xml"])));
        assert_eq!(unclaimed, vec![Some(format_not_claimed()); 3]);
    }

    /// A development-only citation the vendored snapshot holds.
    pub(crate) fn development_cite() -> Value {
        json!({
            "source": "specs/openehr-development/specifications-ITS-XML/components/AM/Release-1.4/Template.xsd",
            "section": "complexType OPERATIONAL_TEMPLATE",
            "quote": "Element 'annotations' added 3 May 2010"
        })
    }

    /// A catalogue root holding the committed System document with its
    /// first workflow resting on development text, the vendored specs
    /// linked beside it.
    fn development_only_world() -> Result<assert_fs::TempDir, Box<dyn std::error::Error>> {
        system_world(|_| development_cite())
    }

    /// A catalogue root holding the committed System document with the
    /// citation of its first workflow replaced by `cite` of it, the vendored
    /// specs linked beside it.
    fn system_world(
        cite: impl FnOnce(Value) -> Value,
    ) -> Result<assert_fs::TempDir, Box<dyn std::error::Error>> {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dir = assert_fs::TempDir::new()?;
        std::os::unix::fs::symlink(repo.join("specs"), dir.path().join("specs"))?;
        let path = repo.join("catalogue/cases/system/system_options.arazzo.yaml");
        let mut document = crate::load::yaml_str_to_value(&std::fs::read_to_string(&path)?, &path)?;
        let workflow = document
            .pointer_mut("/workflows/0")
            .and_then(Value::as_object_mut)
            .ok_or("the System document has a workflow")?;
        let current = workflow
            .get("x-veredictum-cite")
            .cloned()
            .unwrap_or(Value::Null);
        workflow.insert("x-veredictum-cite".to_owned(), cite(current));
        dir.child("catalogue/cases/system/system_options.arazzo.yaml")
            .write_str(&serde_json::to_string_pretty(&document)?)?;
        Ok(dir)
    }

    /// The skip of every workflow of the catalogue under `world`, planned
    /// under `profile`, in document order.
    fn world_skips(
        world: &assert_fs::TempDir,
        profile: SpecProfile,
    ) -> Result<Vec<Option<Skip>>, Box<dyn std::error::Error>> {
        let selection = Selection::read(&world.path().join("catalogue"), None, &[])?;
        let declarations = Declarations {
            profile,
            ..Declarations::default()
        };
        let planned = selection.load(world.path(), &declarations)?;
        Ok(planned
            .iter()
            .flat_map(|document| {
                document
                    .document
                    .description()
                    .workflows
                    .iter()
                    .map(|w| document.plan.skips.get(&w.workflow_id).cloned())
            })
            .collect())
    }

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    fn a_contested_case_is_skipped_under_development_and_judged_under_released()
    -> Result<(), Box<dyn std::error::Error>> {
        let world = system_world(|mut cite| {
            if let Some(cite) = cite.as_object_mut() {
                cite.insert("development".to_owned(), json!({ "contested": 1008 }));
            }
            cite
        })?;
        assert_eq!(
            world_skips(&world, SpecProfile::Released)?,
            vec![None, None, None],
            "the released profile judges the released quote"
        );
        let development = world_skips(&world, SpecProfile::Development)?;
        let skip = development
            .first()
            .cloned()
            .flatten()
            .ok_or("the contested case is skipped")?;
        assert_eq!(skip.reason, SkipReason::ContestedInDevelopment);
        assert_eq!(skip.reason.to_string(), "contested-in-development");
        assert_eq!(
            skip.why.source,
            "specs/openehr/ITS-REST/specifications/operations/options.yaml"
        );
        let detail = skip.detail.as_deref().ok_or("the skip names its report")?;
        assert!(
            detail.contains("upstream report #1008") && detail.contains("options.yaml"),
            "{detail}"
        );
        assert_eq!(development.get(1..), Some(&[None, None][..]));
        Ok(())
    }

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    fn a_development_only_case_is_skipped_under_released_and_runs_under_development()
    -> Result<(), Box<dyn std::error::Error>> {
        let world = development_only_world()?;
        let selection = Selection::read(&world.path().join("catalogue"), None, &[])?;
        let skips = |profile| -> Result<Vec<Option<Skip>>, CatalogueError> {
            let declarations = Declarations {
                profile,
                ..Declarations::default()
            };
            let planned = selection.load(world.path(), &declarations)?;
            Ok(planned
                .iter()
                .flat_map(|document| {
                    document
                        .document
                        .description()
                        .workflows
                        .iter()
                        .map(|w| document.plan.skips.get(&w.workflow_id).cloned())
                })
                .collect())
        };
        let released = skips(SpecProfile::Released)?;
        assert_eq!(released.len(), 3);
        let skip = released
            .first()
            .cloned()
            .flatten()
            .ok_or("the development-only case is skipped")?;
        assert_eq!(skip.reason, SkipReason::DevelopmentOnly);
        assert_eq!(skip.reason.to_string(), "development-only");
        assert!(skip.why.source.starts_with("specs/openehr-development/"));
        let detail = skip
            .detail
            .as_deref()
            .ok_or("the skip names its citation")?;
        assert!(
            detail.contains("specifications-ITS-XML/components/AM/Release-1.4/Template.xsd")
                && detail.contains("complexType OPERATIONAL_TEMPLATE"),
            "{detail}"
        );
        assert_eq!(released.get(1..), Some(&[None, None][..]));
        assert_eq!(
            skips(SpecProfile::Development)?,
            vec![None, None, None],
            "the development profile runs and judges it"
        );
        Ok(())
    }

    #[test]
    fn a_case_whose_cited_text_the_profile_lacks_is_skipped_with_what_is_missing() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let document =
            ArazzoDocument::load(&repo.join("catalogue/cases/system/system_options.arazzo.yaml"))
                .unwrap();
        let workflows = document.description().workflows.len();
        let mut held = QuoteChecker::in_profile(&repo, SpecProfile::Development);
        assert_eq!(
            absent_text_skips(&document, "system_options", &mut held, &repo).unwrap(),
            [],
            "the committed snapshot holds every text the document cites"
        );
        let empty = assert_fs::TempDir::new().unwrap();
        let mut lacking = QuoteChecker::in_profile(empty.path(), SpecProfile::Development);
        let skips =
            absent_text_skips(&document, "system_options", &mut lacking, empty.path()).unwrap();
        assert_eq!(skips.len(), workflows, "one skip per workflow");
        for (_, skip) in &skips {
            assert_eq!(skip.reason, SkipReason::CitedTextAbsent);
            let detail = skip.detail.as_deref().unwrap();
            assert!(
                detail.starts_with("development profile: the development specification profile has no file `specs/openehr-development/"),
                "{detail}"
            );
        }
        let mut released = QuoteChecker::new(&repo);
        assert_eq!(
            absent_text_skips(&document, "system_options", &mut released, &repo).unwrap(),
            [],
            "the release holds every text the catalogue cites"
        );
    }
}
