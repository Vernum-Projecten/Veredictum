// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The reading structure every view of a result follows: chapters, the
//! capability folders inside each, the BASE sections of each folder, and the
//! theme and tier of each option.
//!
//! `catalogue/chapters.yaml` declares the chapters, folders, sections and
//! option themes; a document names its section with
//! [`SECTION_KEY`] on its `info`. Whether a document is BASE or OPTION is
//! derived from `capabilities.yaml` and never declared a second time: a
//! document an arm names is an option document, every other one is BASE.
//! `catalogue/reports.yaml` is the committed snapshot of the upstream
//! reports the citations name, so a view names a report's title and state
//! without asking the tracker. No openEHR spec governs the structure — our
//! own design.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use roas_arazzo::v1_1::Workflow;
use serde::{Deserialize, Serialize};

use crate::arazzo::capabilities::Capabilities;
use crate::arazzo::cite::{Citation, citation_of};
use crate::arazzo::document::ArazzoDocument;
use crate::arazzo::gates::published::{chapters_schema, reports_schema};
use crate::arazzo::maturity::{Maturities, MaturityError};
use crate::arazzo::source::split_qualified;

/// The structure file under a catalogue root.
pub const CHAPTERS_FILE: &str = "chapters.yaml";

/// The report snapshot under a catalogue root.
pub const REPORTS_FILE: &str = "reports.yaml";

/// The `info` extension naming a BASE document's section.
pub const SECTION_KEY: &str = "x-veredictum-section";

/// The tag every view puts on a case resting on released text an upstream
/// report records as unclear or contradictory, and on an option that exists
/// only because of such text.
pub const SPEC_DIVERGE_TAG: &str = "@SpecDiverge";

/// The ITS-REST operation that uploads an ADL 1.4 operational template.
pub const ADL14_UPLOAD: &str = "definition_template_adl1.4_upload";

/// The ITS-REST operation that uploads an ADL 2 operational template.
pub const ADL2_UPLOAD: &str = "definition_template_adl2_upload";

/// What an option's arms are about, the block a view files it under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    /// The canonical XML format.
    CanonicalXml,
    /// The Simplified Formats and the Web Template.
    SimplifiedFormats,
    /// Behaviour the release leaves to the implementation.
    LeftOpen,
}

impl Theme {
    /// All variants, in display order (schema emission derives from this).
    pub const ALL: &[Self] = &[Self::CanonicalXml, Self::SimplifiedFormats, Self::LeftOpen];

    /// The heading a view prints.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::CanonicalXml => "Canonical XML",
            Self::SimplifiedFormats => "Simplified Formats and Web Template",
            Self::LeftOpen => "Behaviour the release leaves open",
        }
    }
}

/// How firmly the release asks for what an option decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    /// Support is a SHOULD: declining it is recorded as a warning.
    Recommended,
    /// A MAY, or behaviour the release leaves to the implementation.
    Options,
    /// An option only because the released text is defective; its citation
    /// names the upstream report.
    SpecBoundary,
}

impl Tier {
    /// All variants, in display order (schema emission derives from this).
    pub const ALL: &[Self] = &[Self::Recommended, Self::Options, Self::SpecBoundary];

    /// The label a view prints beside an option of this tier.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Recommended => "Recommended (SHOULD)",
            Self::Options => "Optional (MAY)",
            Self::SpecBoundary => SPEC_DIVERGE_TAG,
        }
    }
}

/// The template formalism a semantic validation case checks constraints of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Formalism {
    /// An ADL 1.4 operational template.
    #[serde(rename = "adl1.4")]
    Adl14,
    /// An ADL 2 operational template.
    #[serde(rename = "adl2")]
    Adl2,
}

impl Formalism {
    /// All variants (schema emission derives from this).
    pub const ALL: &[Self] = &[Self::Adl14, Self::Adl2];

    /// The badge a view prints.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Adl14 => "ADL 1.4",
            Self::Adl2 => "ADL 2",
        }
    }

    /// The formalism of the template the operation `operation` uploads.
    #[must_use]
    pub fn of_upload(operation: &str) -> Option<Self> {
        match operation {
            ADL14_UPLOAD => Some(Self::Adl14),
            ADL2_UPLOAD => Some(Self::Adl2),
            _ => None,
        }
    }
}

impl fmt::Display for Formalism {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Where an upstream report stands on the tracker, as the snapshot records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReportState {
    /// Open, not yet re-verified.
    Open,
    /// Open and re-verified first-hand.
    Confirmed,
    /// Closed as the standing outbound record.
    Closed,
}

impl ReportState {
    /// All variants (schema emission derives from this).
    pub const ALL: &[Self] = &[Self::Open, Self::Confirmed, Self::Closed];
}

impl fmt::Display for ReportState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Open => "open",
            Self::Confirmed => "confirmed",
            Self::Closed => "closed, standing record",
        })
    }
}

/// One BASE section of a capability folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionDecl {
    /// The key a document's [`SECTION_KEY`] names.
    pub section: String,
    /// The heading a view prints.
    pub title: String,
}

/// One capability folder: a band of its chapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FolderDecl {
    /// The folder name under `cases/`.
    pub folder: String,
    /// The heading a view prints.
    pub title: String,
    /// Whether the views badge each case with the template formalism its
    /// setups upload.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub formalism: bool,
    /// The BASE sections, in reading order.
    #[serde(default)]
    pub sections: Vec<SectionDecl>,
}

/// One chapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChapterDecl {
    /// The chapter key.
    pub chapter: String,
    /// The heading a view prints.
    pub title: String,
    /// The capability folders, in reading order.
    pub folders: Vec<FolderDecl>,
}

/// The theme and tier of one option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionPlacement {
    /// What the option's arms are about.
    pub theme: Theme,
    /// How firmly the release asks for it.
    pub tier: Tier,
}

/// `catalogue/chapters.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Chapters {
    /// The chapters, in reading order.
    pub chapters: Vec<ChapterDecl>,
    /// The placement of every option of `capabilities.yaml`, by name.
    #[serde(default)]
    pub options: BTreeMap<String, OptionPlacement>,
}

/// One upstream report, as the snapshot records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportEntry {
    /// The issue title.
    pub title: String,
    /// Where it stands.
    pub state: ReportState,
}

/// `catalogue/reports.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reports {
    /// The reports, by issue number.
    #[serde(default)]
    pub reports: BTreeMap<NonZeroU64, ReportEntry>,
}

/// Why a structure file does not load.
#[derive(Debug, thiserror::Error)]
pub enum StructureError {
    /// The file does not read or parse.
    #[error(transparent)]
    Load(#[from] crate::load::LoadError),
    /// The file does not hold against its published schema.
    #[error("`{path}` does not hold: {}", reasons.join("; "))]
    Shape {
        /// The file.
        path: PathBuf,
        /// Every violation.
        reasons: Vec<String>,
    },
    /// A document of the catalogue cannot be placed.
    #[error("`{document}` cannot be placed: {reason}")]
    Unplaced {
        /// The document id.
        document: String,
        /// Why.
        reason: String,
    },
    /// A cited file has no maturity.
    #[error(transparent)]
    Maturity(#[from] MaturityError),
}

static CHAPTERS_VALIDATOR: LazyLock<Result<jsonschema::Validator, String>> =
    LazyLock::new(|| jsonschema::validator_for(&chapters_schema()).map_err(|e| e.to_string()));

static REPORTS_VALIDATOR: LazyLock<Result<jsonschema::Validator, String>> =
    LazyLock::new(|| jsonschema::validator_for(&reports_schema()).map_err(|e| e.to_string()));

/// Loads a YAML file through `validator` into `T`.
fn load_through<T: serde::de::DeserializeOwned>(
    path: &Path,
    validator: &Result<jsonschema::Validator, String>,
) -> Result<T, StructureError> {
    let value = crate::load::yaml_file_to_value(path)?;
    let shape = |reasons| StructureError::Shape {
        path: path.to_owned(),
        reasons,
    };
    let validator = validator
        .as_ref()
        .map_err(|error| shape(vec![format!("the schema does not compile: {error}")]))?;
    let violations: Vec<String> = validator
        .iter_errors(&value)
        .map(|error| format!("#{}: {error}", error.instance_path()))
        .collect();
    if !violations.is_empty() {
        return Err(shape(violations));
    }
    serde_json::from_value(value).map_err(|error| shape(vec![error.to_string()]))
}

impl Chapters {
    /// Loads the structure file at `path` through the published schema.
    ///
    /// # Errors
    /// [`StructureError`] when the file does not load or does not hold.
    pub fn load(path: &Path) -> Result<Self, StructureError> {
        load_through(path, &CHAPTERS_VALIDATOR)
    }

    /// The chapter and folder declaration of the capability folder
    /// `folder`, the first that lists it.
    #[must_use]
    pub fn folder(&self, folder: &str) -> Option<(&ChapterDecl, &FolderDecl)> {
        self.chapters.iter().find_map(|chapter| {
            chapter
                .folders
                .iter()
                .find(|decl| decl.folder == folder)
                .map(|decl| (chapter, decl))
        })
    }
}

impl Reports {
    /// Loads the report snapshot at `path` through the published schema.
    ///
    /// # Errors
    /// [`StructureError`] when the file does not load or does not hold.
    pub fn load(path: &Path) -> Result<Self, StructureError> {
        load_through(path, &REPORTS_VALIDATOR)
    }
}

/// One option arm a document sits on.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ArmRef {
    /// The option.
    pub option: String,
    /// The arm.
    pub arm: String,
}

/// Whether a case is BASE or OPTION.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Standing {
    /// Every claimant of the capability runs it, under one section.
    Base {
        /// The section key.
        section: String,
    },
    /// It runs only when the statement selects one of these arms.
    Option {
        /// The arms naming its document, in option order.
        arms: Vec<ArmRef>,
    },
}

/// Where one case sits in the reading structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Place {
    /// The chapter key.
    pub chapter: String,
    /// The capability folder.
    pub folder: String,
    /// BASE with its section, or OPTION with its arms.
    pub standing: Standing,
    /// Whether a citation of the case rests on TRIAL text.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub trial: bool,
    /// The upstream reports the case's citations name, ascending.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reports: Vec<NonZeroU64>,
    /// The template formalism its setups upload, on a folder that badges it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formalism: Option<Formalism>,
}

impl Place {
    /// Whether the case is BASE.
    #[must_use]
    pub fn is_base(&self) -> bool {
        matches!(self.standing, Standing::Base { .. })
    }
}

/// One arm of an option, as a result records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineArm {
    /// The arm token.
    pub arm: String,
    /// Whether the statement selected it.
    pub selected: bool,
    /// The documents it names; empty when the arm runs nothing.
    pub documents: Vec<String>,
}

/// One option, as a result records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineOption {
    /// The option name.
    pub option: String,
    /// What it decides.
    pub description: String,
    /// Its theme.
    pub theme: Theme,
    /// Its tier.
    pub tier: Tier,
    /// The upstream report its citation names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<NonZeroU64>,
    /// The capability folders its documents sit in, in reading order.
    pub folders: Vec<String>,
    /// Its arms, in name order.
    pub arms: Vec<OutlineArm>,
}

/// One upstream report, as a result records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineReport {
    /// The issue number.
    pub report: NonZeroU64,
    /// The issue title.
    pub title: String,
    /// Where it stands.
    pub state: ReportState,
}

/// The reading structure a result carries, so every view groups its rows
/// without reading the catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Outline {
    /// The chapters, folders and sections, in reading order.
    pub chapters: Vec<ChapterDecl>,
    /// Every option, in name order.
    pub options: Vec<OutlineOption>,
    /// Every upstream report a citation names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reports: Vec<OutlineReport>,
    /// Where each case sits, by case id.
    pub cases: BTreeMap<String, Place>,
}

impl Outline {
    /// Where the case `case` sits.
    #[must_use]
    pub fn place(&self, case: &str) -> Option<&Place> {
        self.cases.get(case)
    }

    /// The option named `option`.
    #[must_use]
    pub fn option(&self, option: &str) -> Option<&OutlineOption> {
        self.options.iter().find(|o| o.option == option)
    }

    /// The report numbered `report`.
    #[must_use]
    pub fn report(&self, report: NonZeroU64) -> Option<&OutlineReport> {
        self.reports.iter().find(|r| r.report == report)
    }

    /// The chapter and folder declaration of `folder`.
    #[must_use]
    pub fn folder(&self, folder: &str) -> Option<(&ChapterDecl, &FolderDecl)> {
        self.chapters.iter().find_map(|chapter| {
            chapter
                .folders
                .iter()
                .find(|decl| decl.folder == folder)
                .map(|decl| (chapter, decl))
        })
    }

    /// The title of the section `section` of `folder`.
    #[must_use]
    pub fn section_title(&self, folder: &str, section: &str) -> Option<&str> {
        self.folder(folder).and_then(|(_, decl)| {
            decl.sections
                .iter()
                .find(|s| s.section == section)
                .map(|s| s.title.as_str())
        })
    }
}

/// The section `document`'s `info` names, when it names one as a string.
#[must_use]
pub fn section_of(document: &ArazzoDocument) -> Option<&str> {
    document
        .description()
        .info
        .extensions
        .as_ref()?
        .get(SECTION_KEY)?
        .as_str()
}

/// Every citation `workflow` carries: its own, its steps' and its criteria'.
#[must_use]
pub fn workflow_citations(workflow: &Workflow) -> Vec<Citation> {
    let mut out = Vec::new();
    let mut take = |extensions| {
        // NOTE: no openEHR spec governs this — our own design; a malformed citation is the
        // citation gate's finding, so it carries nothing to read here.
        if let Ok(Some(citation)) = citation_of(extensions) {
            out.push(citation);
        }
    };
    take(workflow.extensions.as_ref());
    for step in &workflow.steps {
        take(step.extensions.as_ref());
        for criterion in &step.success_criteria {
            take(criterion.extensions.as_ref());
        }
    }
    out
}

/// The arms of `capabilities` that name `document`, in option order.
#[must_use]
pub fn arms_of(capabilities: Option<&Capabilities>, document: &str) -> Vec<ArmRef> {
    capabilities
        .map(|c| {
            c.arm_documents()
                .filter(|(_, _, named)| *named == document)
                .map(|(option, arm, _)| ArmRef {
                    option: option.to_owned(),
                    arm: arm.to_owned(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Reads the setups a case names and the template formalism they upload,
/// loading each setup document once.
#[derive(Debug, Default)]
pub struct Formalisms {
    loaded: BTreeMap<PathBuf, Option<ArazzoDocument>>,
}

impl Formalisms {
    /// The formalisms of the templates `workflow` of `document` uploads,
    /// itself or through the setups it names in `dependsOn`, transitively.
    pub fn of(&mut self, document: &ArazzoDocument, workflow: &Workflow) -> BTreeSet<Formalism> {
        let mut found = BTreeSet::new();
        let mut seen = BTreeSet::new();
        self.walk(document.path(), document, workflow, &mut seen, &mut found);
        found
    }

    fn walk(
        &mut self,
        path: &Path,
        document: &ArazzoDocument,
        workflow: &Workflow,
        seen: &mut BTreeSet<(PathBuf, String)>,
        found: &mut BTreeSet<Formalism>,
    ) {
        if !seen.insert((path.to_owned(), workflow.workflow_id.clone())) {
            return;
        }
        for step in &workflow.steps {
            let operation = step.operation_id.as_deref().and_then(split_qualified);
            if let Some(formalism) = operation.and_then(|(_, id)| Formalism::of_upload(id)) {
                found.insert(formalism);
            }
        }
        for id in &workflow.depends_on {
            let Some((source, workflow_id)) = split_qualified(id) else {
                if let Some(local) = document.workflow(id) {
                    self.walk(path, document, local, seen, found);
                }
                continue;
            };
            let Some(source_path) = document.workflow_sources().get(source).cloned() else {
                continue;
            };
            // NOTE: no openEHR spec governs this — our own design; a setup that does not load is
            // the load gate's finding, and it uploads nothing to read here.
            let setup = self
                .loaded
                .entry(source_path.clone())
                .or_insert_with(|| ArazzoDocument::load(&source_path).ok())
                .take();
            if let Some(setup) = setup {
                if let Some(next) = setup.workflow(workflow_id) {
                    self.walk(&source_path, &setup, next, seen, found);
                }
                self.loaded.insert(source_path, Some(setup));
            }
        }
    }
}

/// What placing the cases of a catalogue reads.
#[derive(Debug)]
pub struct Placer<'a> {
    chapters: &'a Chapters,
    capabilities: Option<&'a Capabilities>,
    maturities: Maturities,
    formalisms: Formalisms,
}

impl<'a> Placer<'a> {
    /// A placer over `chapters` and `capabilities`, reading maturities from
    /// the release vendored under `repo_root`.
    #[must_use]
    pub fn new(
        chapters: &'a Chapters,
        capabilities: Option<&'a Capabilities>,
        repo_root: &Path,
    ) -> Self {
        Self {
            chapters,
            capabilities,
            maturities: Maturities::new(repo_root),
            formalisms: Formalisms::default(),
        }
    }

    /// Where every workflow of `document`, whose id is `name` and which sits
    /// in the capability folder `folder`, is placed, by workflow id.
    ///
    /// # Errors
    /// [`StructureError::Unplaced`] when the folder is in no chapter, a BASE
    /// document names no section of its folder, or a case of a formalism
    /// folder uploads no template; [`StructureError::Maturity`] when a
    /// citation names a file with no release status.
    pub fn place(
        &mut self,
        document: &ArazzoDocument,
        name: &str,
        folder: &str,
    ) -> Result<BTreeMap<String, Place>, StructureError> {
        let unplaced = |reason: String| StructureError::Unplaced {
            document: name.to_owned(),
            reason,
        };
        let (chapter, decl) = self
            .chapters
            .folder(folder)
            .ok_or_else(|| unplaced(format!("the folder `{folder}` is in no chapter")))?;
        let arms = arms_of(self.capabilities, name);
        let option_reports: Vec<NonZeroU64> = arms
            .iter()
            .filter_map(|arm| {
                self.capabilities?
                    .options
                    .get(&arm.option)
                    .and_then(|o| o.cite.report)
            })
            .collect();
        let standing = if arms.is_empty() {
            let section = section_of(document)
                .filter(|s| decl.sections.iter().any(|d| d.section == *s))
                .ok_or_else(|| {
                    unplaced(format!(
                        "a BASE document names no section of `{folder}` in `{SECTION_KEY}`"
                    ))
                })?;
            Standing::Base {
                section: section.to_owned(),
            }
        } else {
            Standing::Option { arms }
        };
        let mut places = BTreeMap::new();
        for workflow in &document.description().workflows {
            let mut trial = false;
            let mut reports: BTreeSet<NonZeroU64> = option_reports.iter().copied().collect();
            for citation in workflow_citations(workflow) {
                trial |= self.maturities.of(&citation.source)?.is_trial();
                reports.extend(citation.report);
            }
            let formalism = if decl.formalism {
                let found = self.formalisms.of(document, workflow);
                let mut each = found.into_iter();
                match (each.next(), each.next()) {
                    (Some(one), None) => Some(one),
                    (None, _) => {
                        return Err(unplaced(format!(
                            "`{}` uploads no template, so it has no formalism",
                            workflow.workflow_id
                        )));
                    }
                    (Some(_), Some(_)) => {
                        return Err(unplaced(format!(
                            "`{}` uploads templates of both formalisms",
                            workflow.workflow_id
                        )));
                    }
                }
            } else {
                None
            };
            places.insert(
                workflow.workflow_id.clone(),
                Place {
                    chapter: chapter.chapter.clone(),
                    folder: folder.to_owned(),
                    standing: standing.clone(),
                    trial,
                    reports: reports.into_iter().collect(),
                    formalism,
                },
            );
        }
        Ok(places)
    }
}

/// The outline a result carries: the declared structure, every option with
/// the arms `selected` names, every report of `reports`, and `cases`.
///
/// `folder_of` maps each document id of the catalogue to its capability
/// folder, which places each option under the folders its documents sit in.
#[must_use]
pub fn outline(
    chapters: &Chapters,
    capabilities: Option<&Capabilities>,
    reports: Option<&Reports>,
    folder_of: &BTreeMap<String, String>,
    selected: &[String],
    cases: BTreeMap<String, Place>,
) -> Outline {
    let order: Vec<&str> = chapters
        .chapters
        .iter()
        .flat_map(|c| c.folders.iter().map(|f| f.folder.as_str()))
        .collect();
    let options = capabilities
        .map(|c| {
            c.options
                .iter()
                .filter_map(|(name, option)| {
                    let placement = chapters.options.get(name)?;
                    let touched: BTreeSet<&str> = option
                        .arms
                        .values()
                        .flat_map(|arm| arm.documents.iter())
                        .filter_map(|d| folder_of.get(d).map(String::as_str))
                        .collect();
                    Some(OutlineOption {
                        option: name.clone(),
                        description: option.description.clone(),
                        theme: placement.theme,
                        tier: placement.tier,
                        report: option.cite.report,
                        folders: order
                            .iter()
                            .filter(|f| touched.contains(*f))
                            .map(|f| (*f).to_owned())
                            .collect(),
                        arms: option
                            .arms
                            .iter()
                            .map(|(arm, decl)| OutlineArm {
                                arm: arm.clone(),
                                selected: selected.contains(arm),
                                documents: decl.documents.clone(),
                            })
                            .collect(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Outline {
        chapters: chapters.chapters.clone(),
        options,
        reports: reports
            .map(|r| {
                r.reports
                    .iter()
                    .map(|(number, entry)| OutlineReport {
                        report: *number,
                        title: entry.title.clone(),
                        state: entry.state,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        cases,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;

    const CITE: &str = "x-veredictum-cite: { source: specs/rest-oas/ehr-validation.openapi.yaml, section: openapi, quote: x }";

    fn capabilities(documents: &str) -> Capabilities {
        let value = serde_json::json!({
            "options": {
                "o": {
                    "description": "d",
                    "arms": { "a": { "documents": serde_json::from_str::<serde_json::Value>(documents).unwrap() }, "b": { "documents": [] } },
                    "x-veredictum-cite": { "source": "specs/openehr/x.md", "section": "s", "quote": "q" }
                }
            }
        });
        serde_json::from_value(value).unwrap()
    }

    fn chapters() -> Chapters {
        Chapters {
            chapters: vec![ChapterDecl {
                chapter: "ehr".to_owned(),
                title: "EHR".to_owned(),
                folders: vec![FolderDecl {
                    folder: "ehr".to_owned(),
                    title: "EHR".to_owned(),
                    formalism: false,
                    sections: vec![SectionDecl {
                        section: "create".to_owned(),
                        title: "Create".to_owned(),
                    }],
                }],
            }],
            options: BTreeMap::from([(
                "o".to_owned(),
                OptionPlacement {
                    theme: Theme::LeftOpen,
                    tier: Tier::Options,
                },
            )]),
        }
    }

    #[test]
    fn naming_a_document_in_an_arm_moves_it_from_base_to_option() {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("specs/rest-oas/ehr.yaml")
            .write_str(
                "openapi: 3.0.3\npaths:\n  /ehr:\n    post:\n      operationId: ehr_create\n",
            )
            .unwrap();
        dir.child("specs/openehr/ITS-REST/manifest.json")
            .write_str(
                r#"{ "specifications": [{ "id": "REST_OAS_files", "spec_status": "STABLE" }] }"#,
            )
            .unwrap();
        let file = dir.child("cases/ehr/doc.arazzo.yaml");
        file.write_str(&format!(
            "arazzo: 1.1.0\ninfo: {{ title: t, version: '1', {SECTION_KEY}: create }}\nsourceDescriptions:\n  - {{ name: ehr, url: ../../specs/rest-oas/ehr.yaml, type: openapi }}\nworkflows:\n  - workflowId: w\n    {CITE}\n    steps:\n      - stepId: s\n        operationId: $sourceDescriptions.ehr.ehr_create\n        successCriteria:\n          - condition: $statusCode == 201\n"
        ))
        .unwrap();
        let document = ArazzoDocument::load(file.path()).unwrap();
        let chapters = chapters();
        let unarmed = capabilities("[]");
        let base = Placer::new(&chapters, Some(&unarmed), dir.path())
            .place(&document, "doc", "ehr")
            .unwrap();
        assert_eq!(
            base.get("w").map(|p| &p.standing),
            Some(&Standing::Base {
                section: "create".to_owned()
            })
        );
        let armed = capabilities(r#"["doc"]"#);
        let option = Placer::new(&chapters, Some(&armed), dir.path())
            .place(&document, "doc", "ehr")
            .unwrap();
        assert_eq!(
            option.get("w").map(|p| &p.standing),
            Some(&Standing::Option {
                arms: vec![ArmRef {
                    option: "o".to_owned(),
                    arm: "a".to_owned()
                }]
            })
        );
        assert!(
            Placer::new(&chapters, Some(&unarmed), dir.path())
                .place(&document, "doc", "nowhere")
                .is_err(),
            "a folder in no chapter cannot be placed"
        );
    }

    #[test]
    fn the_committed_catalogue_places_every_case_with_its_trial_and_formalism() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let catalogue = root.join("catalogue");
        let chapters = Chapters::load(&catalogue.join(CHAPTERS_FILE)).unwrap();
        let capabilities =
            Capabilities::load(&catalogue.join(crate::arazzo::capabilities::CAPABILITIES_FILE))
                .unwrap();
        let mut placer = Placer::new(&chapters, Some(&capabilities), &root);
        let place = |placer: &mut Placer<'_>, folder: &str, id: &str| {
            let path = catalogue.join(format!("cases/{folder}/{id}.arazzo.yaml"));
            placer
                .place(&ArazzoDocument::load(&path).unwrap(), id, folder)
                .unwrap()
        };
        let stored = place(&mut placer, "query", "definition_query");
        assert!(
            stored.values().any(|p| p.trial),
            "the stored-query OAS is TRIAL"
        );
        let top = place(&mut placer, "query", "aqlc_top");
        assert!(top.values().all(Place::is_base), "TOP is BASE");
        let content = place(&mut placer, "content", "dv_count");
        assert!(
            content
                .values()
                .all(|p| p.formalism == Some(Formalism::Adl14)),
            "every content case uploads an ADL 1.4 template"
        );
        let unmatched = place(&mut placer, "content", "content_item_unmatched_refused");
        assert!(
            unmatched
                .values()
                .all(|p| p.reports.iter().any(|r| r.get() == 307)),
            "an option resting on a reported text carries its report"
        );
    }
}
