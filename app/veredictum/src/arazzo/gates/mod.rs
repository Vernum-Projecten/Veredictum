// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `validate` gates for the Arazzo catalogue under a catalogue root.
//!
//! The documents are `cases/<capability>/<topic>.arazzo.yaml`, each rows
//! file sits beside the document whose workflow it feeds as
//! `<workflowId>.rows.yaml`, and the surface file is `cases/surface.yaml`.
//! Every file is read once, every finding names its [`Gate`], and the output
//! is sorted, so two runs over one tree print the same bytes.

pub mod aql;
pub mod chapters;
pub mod cycles;
pub mod development;
pub mod folders;
pub mod profile;
pub mod published;
pub mod rows;
pub mod schema;
pub mod surface;

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::arazzo::capabilities::{CAPABILITIES_FILE, Capabilities};
use crate::arazzo::cite::{QuoteChecker, check_level};
use crate::arazzo::document::{ArazzoDocument, source_finding};
use crate::arazzo::spec_profile::SpecProfile;

/// The suffix every Arazzo document carries.
pub const DOCUMENT_SUFFIX: &str = ".arazzo.yaml";

/// The folder under a catalogue root that holds the capability folders.
pub const CASES_DIR: &str = "cases";

/// The gates, one per kind of refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Gate {
    /// A file that does not read, parse, or carry a unique id.
    Load,
    /// The official Arazzo 1.1 JSON Schema, the model's own validation, and
    /// the `workflowId` alphabet.
    Schema,
    /// A key Arazzo does not define, or an extension outside the catalogue's
    /// own six or outside the place each is allowed.
    Keys,
    /// A source, an `operationId` or a parameter that does not resolve.
    Source,
    /// A criterion with no citation, or a citation that does not hold.
    Citation,
    /// A citation whose `level` disagrees with the keywords of its quote.
    Level,
    /// A condition or expression outside the condition profile.
    Profile,
    /// A rows file that does not validate or bind its workflow's inputs.
    Rows,
    /// A scoped surface element with neither a covering step nor a gap.
    Surface,
    /// A capabilities file naming a document that does not exist, or one
    /// document under two arms of an option.
    Identity,
    /// An AQL query that does not parse outside a step expecting `400`.
    Aql,
    /// A capability folder without a workflow-count floor, below its floor,
    /// or a floor naming no folder.
    Floor,
    /// A capability folder whose `extension-only` marking disagrees with the
    /// sources its steps call.
    Realization,
    /// A document, folder, section, option or report the reading structure
    /// does not place, or places twice.
    Structure,
    /// A workflow resting on released text that depends, at any depth, on a
    /// development-only workflow.
    DevelopmentSetup,
    /// A workflow whose `dependsOn` leads back to itself.
    DependsOnCycle,
    /// An extension source naming no extension of the vocabulary, an
    /// extension operation called outside a shared setup or by another name
    /// than its key, or a verdict-bearing judgment on an extension step.
    Extension,
}

impl Gate {
    /// The stable token a finding prints.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Load => "arazzo-load",
            Self::Schema => "arazzo-schema",
            Self::Keys => "arazzo-keys",
            Self::Source => "arazzo-source",
            Self::Citation => "arazzo-citation",
            Self::Level => "arazzo-level",
            Self::Profile => "arazzo-profile",
            Self::Rows => "arazzo-rows",
            Self::Surface => "arazzo-surface",
            Self::Identity => "arazzo-identity",
            Self::Aql => "arazzo-aql",
            Self::Floor => "arazzo-floor",
            Self::Realization => "arazzo-realization-scope",
            Self::Structure => "arazzo-structure",
            Self::DevelopmentSetup => "arazzo-development-setup",
            Self::DependsOnCycle => "arazzo-depends-on-cycle",
            Self::Extension => "arazzo-extension",
        }
    }
}

/// One finding over the Arazzo catalogue.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct GateFinding {
    /// The gate that refuses.
    pub gate: Gate,
    /// The file the finding is about.
    pub file: PathBuf,
    /// Where inside the file.
    pub location: String,
    /// What is wrong.
    pub message: String,
}

impl GateFinding {
    pub(crate) fn new(
        gate: Gate,
        file: &Path,
        location: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            gate,
            file: file.to_owned(),
            location: location.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for GateFinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {}: {}: {}",
            self.gate.token(),
            self.file.display(),
            self.location,
            self.message
        )
    }
}

/// One loaded document, keyed by its document id in the catalogue map.
struct Located {
    document: ArazzoDocument,
}

/// Runs every Arazzo gate over the catalogue under `catalogue_root`, whose
/// [`CASES_DIR`] folder holds the documents; a root without one yields
/// nothing.
///
/// `spec_root` is the vendored openEHR tree (`<repo>/specs/openehr`). With
/// it, every quote is looked up in the files under its repository root
/// (`<spec_root>/../..`) and the scoped surface is enumerated from the
/// decomposed ITS-REST OAS; without it, those two checks do not run, the way
/// every other spec-resolving gate of `validate` does not.
#[must_use]
pub fn check_catalogue(catalogue_root: &Path, spec_root: Option<&Path>) -> Vec<GateFinding> {
    let mut findings = Vec::new();
    let repo_root = spec_root.map(|specs| specs.join("../.."));
    let workflows = catalogue_root.join(CASES_DIR);
    let mut documents: BTreeMap<String, Located> = BTreeMap::new();
    let mut rows_files = Vec::new();
    let surface_file = workflows.join(surface::SURFACE_FILE);
    for path in files_under(&workflows, &mut findings) {
        if path == surface_file {
            continue;
        }
        let id = document_id(&path);
        let is_rows = rows::workflow_id_of(&path).is_some();
        if id.is_none() && !is_rows {
            findings.push(GateFinding::new(
                Gate::Load,
                &path,
                "#",
                format!(
                    "{CASES_DIR}/ holds `*{DOCUMENT_SUFFIX}` documents, `*{}` rows files and `{}` only",
                    rows::ROWS_SUFFIX,
                    surface::SURFACE_FILE
                ),
            ));
            continue;
        }
        if !in_capability_folder(&workflows, &path) {
            findings.push(GateFinding::new(
                Gate::Load,
                &path,
                "#",
                "a document or rows file sits in `cases/<capability>/`, never at the top",
            ));
            continue;
        }
        let Some(id) = id else {
            rows_files.push(path);
            continue;
        };
        let Some(document) = load_document(&path, repo_root.as_deref(), &mut findings) else {
            continue;
        };
        if let Some(earlier) = documents.get(&id) {
            findings.push(GateFinding::new(
                Gate::Load,
                &path,
                "#",
                format!(
                    "document id `{id}` is already taken by `{}`",
                    earlier.document.path().display()
                ),
            ));
            continue;
        }
        documents.insert(id, Located { document });
    }
    let rows = check_rows_files(&rows_files, &documents, repo_root.as_deref(), &mut findings);
    check_aql(&documents, &rows, &mut findings);
    let capabilities = check_capabilities(
        catalogue_root,
        &documents,
        repo_root.as_deref(),
        &mut findings,
    );
    let loaded: BTreeMap<&str, &ArazzoDocument> = documents
        .iter()
        .map(|(id, located)| (id.as_str(), &located.document))
        .collect();
    findings.extend(chapters::check(
        catalogue_root,
        &loaded,
        capabilities.as_ref(),
        repo_root.as_deref(),
    ));
    findings.extend(development::check(&loaded));
    findings.extend(cycles::check(&loaded));
    if let Some(specs) = spec_root {
        let released = capabilities
            .as_ref()
            .map(|c| c.in_profile(SpecProfile::Released));
        check_surface(
            &workflows,
            specs,
            &documents,
            released.as_ref(),
            &mut findings,
        );
    }
    findings.sort();
    findings.dedup();
    findings
}

/// Loads one document and runs its own gates; `None` when it cannot be
/// modelled at all.
fn load_document(
    path: &Path,
    repo_root: Option<&Path>,
    findings: &mut Vec<GateFinding>,
) -> Option<ArazzoDocument> {
    let raw = match crate::load::yaml_file_to_value(path) {
        Ok(raw) => raw,
        Err(error) => {
            findings.push(GateFinding::new(Gate::Load, path, "#", error.detail()));
            return None;
        }
    };
    let (document, problems) = match ArazzoDocument::from_value(path, raw.clone()) {
        Ok(loaded) => loaded,
        Err(error) => {
            findings.push(GateFinding::new(Gate::Load, path, "#", error.to_string()));
            match schema::violations(&raw) {
                Ok(violations) => findings.extend(
                    violations
                        .into_iter()
                        .map(|v| GateFinding::new(Gate::Schema, path, "#", v)),
                ),
                Err(error) => {
                    findings.push(GateFinding::new(Gate::Schema, path, "#", error.to_string()));
                }
            }
            return None;
        }
    };
    for problem in &problems {
        let finding = source_finding(problem);
        findings.push(GateFinding::new(
            finding.kind.gate(),
            path,
            finding.location,
            finding.kind.to_string(),
        ));
    }
    for finding in document.gate(repo_root) {
        findings.push(GateFinding::new(
            finding.kind.gate(),
            path,
            finding.location,
            finding.kind.to_string(),
        ));
    }
    Some(document)
}

/// The rows gate over the rows files beside the documents, plus the
/// workflows whose required inputs no rows file binds. Returns every rows
/// file that parsed, keyed by its `(document, workflowId)`.
fn check_rows_files(
    rows_files: &[PathBuf],
    documents: &BTreeMap<String, Located>,
    repo_root: Option<&Path>,
    findings: &mut Vec<GateFinding>,
) -> BTreeMap<(String, String), rows::RowsFile> {
    let mut bound: BTreeMap<(String, String), rows::RowsFile> = BTreeMap::new();
    let mut quotes = repo_root.map(QuoteChecker::new);
    for path in rows_files {
        let (found, binds) = check_rows_file(path, documents, quotes.as_mut());
        findings.extend(found);
        if let Some((key, file)) = binds {
            bound.insert(key, file);
        }
    }
    for (id, located) in documents {
        for workflow in &located.document.description().workflows {
            let unbound = rows::InputShape::of(workflow)
                .unbound_without_rows()
                .join(", ");
            if !unbound.is_empty()
                && !bound.contains_key(&(id.clone(), workflow.workflow_id.clone()))
            {
                findings.push(GateFinding::new(
                    Gate::Rows,
                    located.document.path(),
                    workflow.workflow_id.clone(),
                    format!(
                        "required inputs `{unbound}` are bound by no rows file `{}{}` beside the document",
                        workflow.workflow_id,
                        rows::ROWS_SUFFIX
                    ),
                ));
            }
        }
    }
    bound
}

/// The workflow a parsed rows file feeds, `(document, workflowId)`, with
/// the file.
type BoundRows = ((String, String), rows::RowsFile);

/// Checks one rows file; returns its findings and, when its name matches a
/// workflow of a document in its folder and it parses, that workflow's
/// `(document, workflowId)` with the file.
fn check_rows_file(
    path: &Path,
    documents: &BTreeMap<String, Located>,
    mut quotes: Option<&mut QuoteChecker<'_>>,
) -> (Vec<GateFinding>, Option<BoundRows>) {
    let refuse = |message: String| GateFinding::new(Gate::Rows, path, "#", message);
    let Some(workflow_id) = rows::workflow_id_of(path) else {
        return (Vec::new(), None);
    };
    let folder = path.parent();
    let owners: Vec<(&String, &Located)> = documents
        .iter()
        .filter(|(_, located)| {
            located.document.path().parent() == folder
                && located.document.workflow(workflow_id).is_some()
        })
        .collect();
    let (document_id, located) = match owners.as_slice() {
        [owner] => *owner,
        [] => {
            let message = format!("`{workflow_id}` names no workflow of a document in this folder");
            return (vec![refuse(message)], None);
        }
        _ => {
            let message = format!(
                "`{workflow_id}` names a workflow of more than one document in this folder"
            );
            return (vec![refuse(message)], None);
        }
    };
    let value = match crate::load::yaml_file_to_value(path) {
        Ok(value) => value,
        Err(error) => {
            return (
                vec![GateFinding::new(Gate::Load, path, "#", error.detail())],
                None,
            );
        }
    };
    let file = match rows::parse(&value) {
        Ok(file) => file,
        Err(messages) => return (messages.into_iter().map(refuse).collect(), None),
    };
    let mut out = Vec::new();
    let case_id = format!("{document_id}/{workflow_id}");
    if file.workflow != case_id {
        out.push(refuse(format!(
            "`workflow` is `{}`, while the file name and folder name `{case_id}`",
            file.workflow
        )));
    }
    if let Some(workflow) = located.document.workflow(workflow_id) {
        out.extend(rows::check_rows(&file, workflow).into_iter().map(refuse));
        if let Some(key) = crate::arazzo::extension::called_by(&located.document, workflow) {
            out.push(GateFinding::new(
                Gate::Extension,
                path,
                "#",
                crate::arazzo::extension::ExtensionError::Rows(key).to_string(),
            ));
        }
    }
    for (index, aggregate) in file.aggregate.iter().enumerate() {
        let at = format!("#/aggregate/{index}");
        if let Err(error) = check_level(&aggregate.cite) {
            out.push(GateFinding::new(Gate::Level, path, &at, error.to_string()));
        }
        if let Some(quotes) = quotes.as_deref_mut()
            && let Err(error) = quotes.check(&aggregate.cite)
        {
            out.push(GateFinding::new(
                Gate::Citation,
                path,
                &at,
                error.to_string(),
            ));
        }
    }
    (
        out,
        Some(((document_id.clone(), workflow_id.to_owned()), file)),
    )
}

/// The AQL gate over every workflow, each with its rows file when it has one.
fn check_aql(
    documents: &BTreeMap<String, Located>,
    rows: &BTreeMap<(String, String), rows::RowsFile>,
    findings: &mut Vec<GateFinding>,
) {
    for (id, located) in documents {
        for workflow in &located.document.description().workflows {
            let case_id = format!("{id}/{}", workflow.workflow_id);
            let file = rows.get(&(id.clone(), workflow.workflow_id.clone()));
            for finding in aql::check_workflow(&case_id, workflow, file) {
                findings.push(GateFinding::new(
                    Gate::Aql,
                    located.document.path(),
                    finding.location,
                    finding.message,
                ));
            }
        }
    }
}

/// The identity gate over `capabilities.yaml`: every document an arm names
/// exists, no document sits under two arms of one option, and each option's
/// citation holds. Returns the file when it loads.
fn check_capabilities(
    catalogue_root: &Path,
    documents: &BTreeMap<String, Located>,
    repo_root: Option<&Path>,
    findings: &mut Vec<GateFinding>,
) -> Option<Capabilities> {
    let path = catalogue_root.join(CAPABILITIES_FILE);
    if !path.is_file() {
        return None;
    }
    let capabilities = match Capabilities::load(&path) {
        Ok(capabilities) => capabilities,
        Err(error) => {
            findings.push(GateFinding::new(Gate::Load, &path, "#", error.to_string()));
            return None;
        }
    };
    let mut arms: BTreeMap<(&str, &str), &str> = BTreeMap::new();
    for (option, arm, document) in capabilities.arm_documents() {
        let at = format!("#/options/{option}/arms/{arm}");
        if !documents.contains_key(document) {
            findings.push(GateFinding::new(
                Gate::Identity,
                &path,
                &at,
                format!("`{document}` is not a document of the catalogue"),
            ));
        }
        if let Some(other) = arms.insert((option, document), arm) {
            findings.push(GateFinding::new(
                Gate::Identity,
                &path,
                &at,
                format!("`{document}` is already under the arm `{other}`"),
            ));
        }
    }
    let loaded: BTreeMap<&str, &ArazzoDocument> = documents
        .iter()
        .map(|(id, located)| (id.as_str(), &located.document))
        .collect();
    findings.extend(folders::check_needs(&path, &capabilities, &loaded));
    let counts = folders::workflow_counts(loaded.values().copied());
    findings.extend(folders::check_floors(&path, &capabilities, &counts));
    findings.extend(folders::check_realization(
        &path,
        &capabilities,
        loaded.values().copied(),
    ));
    let mut quotes = repo_root.map(QuoteChecker::new);
    findings.extend(folders::check_range_cites(
        &path,
        &capabilities,
        quotes.as_mut(),
    ));
    for (name, option) in &capabilities.options {
        let at = format!("#/options/{name}");
        let cites = std::iter::once((at.clone(), &option.cite)).chain(
            option
                .decides
                .iter()
                .enumerate()
                .map(|(i, d)| (format!("{at}/decides/{i}"), &d.cite)),
        );
        for (at, cite) in cites {
            if let Err(error) = check_level(cite) {
                findings.push(GateFinding::new(Gate::Level, &path, &at, error.to_string()));
            }
            if let Some(quotes) = quotes.as_mut()
                && let Err(error) = quotes.check(cite)
            {
                findings.push(GateFinding::new(
                    Gate::Citation,
                    &path,
                    &at,
                    error.to_string(),
                ));
            }
        }
        let arms: Vec<(&str, std::collections::BTreeSet<surface::Element>)> = option
            .arms
            .iter()
            .map(|(arm, decl)| (arm.as_str(), arm_covered(&decl.documents, documents)))
            .collect();
        for error in surface::check_decides(&arms, &option.decided()) {
            findings.push(GateFinding::new(
                Gate::Identity,
                &path,
                format!("{at}/decides"),
                error.to_string(),
            ));
        }
    }
    Some(capabilities)
}

/// The elements the documents of one arm cover.
fn arm_covered(
    arm: &[String],
    documents: &BTreeMap<String, Located>,
) -> std::collections::BTreeSet<surface::Element> {
    surface::covered(
        arm.iter()
            .filter_map(|d| documents.get(d))
            .map(|located| &located.document),
    )
}

/// The surface gate, over the scope `cases/surface.yaml` declares, reading
/// the options of the released profile in `capabilities`.
///
/// A statement runs one arm of an option alone, so the documents under an
/// option's arms cover what [`surface::option_covered`] credits them with:
/// an element every arm covers, and a response status the option's
/// `decides` lists, when one arm asserts it and every arm calls its
/// operation.
fn check_surface(
    workflows: &Path,
    spec_root: &Path,
    documents: &BTreeMap<String, Located>,
    capabilities: Option<&Capabilities>,
    findings: &mut Vec<GateFinding>,
) {
    let path = workflows.join(surface::SURFACE_FILE);
    if !path.is_file() {
        if !documents.is_empty() {
            findings.push(GateFinding::new(
                Gate::Surface,
                &path,
                "#",
                "Arazzo documents exist but no surface file declares their scope",
            ));
        }
        return;
    }
    let value = match crate::load::yaml_file_to_value(&path) {
        Ok(value) => value,
        Err(error) => {
            findings.push(GateFinding::new(Gate::Load, &path, "#", error.detail()));
            return;
        }
    };
    let file = match surface::parse(&value) {
        Ok(file) => file,
        Err(messages) => {
            for message in messages {
                findings.push(GateFinding::new(Gate::Surface, &path, "#", message));
            }
            return;
        }
    };
    let its_rest = spec_root.join("ITS-REST/specifications");
    let (elements, errors) = surface::enumerate(&its_rest, &file.scope);
    for error in errors {
        findings.push(GateFinding::new(
            Gate::Surface,
            &path,
            "#/scope",
            error.to_string(),
        ));
    }
    let in_option: Vec<&str> = capabilities
        .map(|c| c.arm_documents().map(|(_, _, d)| d).collect())
        .unwrap_or_default();
    let mut covered = surface::covered(
        documents
            .iter()
            .filter(|(id, _)| !in_option.contains(&id.as_str()))
            .map(|(_, located)| &located.document),
    );
    for option in capabilities.iter().flat_map(|c| c.options.values()) {
        let arms: Vec<_> = option
            .arms
            .values()
            .map(|arm| arm_covered(&arm.documents, documents))
            .collect();
        covered.extend(surface::option_covered(&arms, &option.decided()));
    }
    for message in surface::judge(&elements, &covered, &file.gaps) {
        findings.push(GateFinding::new(Gate::Surface, &path, "#", message));
    }
}

/// The document id of an Arazzo document: its file name without
/// [`DOCUMENT_SUFFIX`]; `None` for any other file.
fn document_id(path: &Path) -> Option<String> {
    path.file_name()?
        .to_str()?
        .strip_suffix(DOCUMENT_SUFFIX)
        .map(str::to_owned)
}

/// `<document>/<workflowId>`, the way a case id names a workflow; a
/// document whose file name is not `<id>.arazzo.yaml` is named by its path.
pub(crate) fn workflow_label(
    document: &ArazzoDocument,
    workflow: &roas_arazzo::v1_1::Workflow,
) -> String {
    let path = document.path();
    let id = document_id(path).unwrap_or_else(|| path.display().to_string());
    format!("{id}/{}", workflow.workflow_id)
}

/// Whether `path` sits exactly one capability folder below `root`, as
/// `<root>/<capability>/<file>`.
fn in_capability_folder(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root)
        .is_ok_and(|relative| relative.iter().count() == 2)
}

/// Every file under `root`, recursively, sorted. An absent `root` is a
/// catalogue without that directory and yields nothing; any other read
/// failure is a [`Gate::Load`] finding.
fn files_under(root: &Path, findings: &mut Vec<GateFinding>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !root.exists() {
        return out;
    }
    let mut stack = vec![root.to_owned()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) => {
                findings.push(GateFinding::new(Gate::Load, &dir, "#", error.to_string()));
                continue;
            }
        };
        for entry in entries {
            let path = match entry {
                Ok(entry) => entry.path(),
                Err(error) => {
                    findings.push(GateFinding::new(Gate::Load, &dir, "#", error.to_string()));
                    continue;
                }
            };
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;

    const OAS: &str = "openapi: 3.0.3\npaths:\n  /ehr/{ehr_id}:\n    put:\n      operationId: ehr_create_with_id\n      parameters:\n        - { name: ehr_id, in: path, required: true }\n";

    fn document(inputs: &str) -> String {
        format!(
            "arazzo: 1.1.0\ninfo: {{ title: t, version: '1' }}\nsourceDescriptions:\n  - {{ name: ehr, url: ../../../specs/rest-oas/ehr-validation.openapi.yaml, type: openapi }}\nworkflows:\n  - workflowId: w\n    x-veredictum-cite: {{ source: specs/rest-oas/ehr-validation.openapi.yaml, section: openapi, quote: x }}\n{inputs}    steps:\n      - stepId: create\n        operationId: $sourceDescriptions.ehr.ehr_create_with_id\n        parameters: [{{ name: ehr_id, in: path, value: $inputs.ehr_id }}]\n        successCriteria:\n          - condition: $statusCode == 201\n"
        )
    }

    const REQUIRED: &str = "    inputs:\n      type: object\n      required: [ehr_id, units]\n      properties:\n        ehr_id: { type: string, format: uuid }\n        units: { type: string }\n";

    fn world(inputs: &str) -> assert_fs::TempDir {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("specs/rest-oas/ehr-validation.openapi.yaml")
            .write_str(OAS)
            .unwrap();
        dir.child("catalogue/cases/ehr/ehr.arazzo.yaml")
            .write_str(&document(inputs))
            .unwrap();
        dir
    }

    fn gates(dir: &assert_fs::TempDir) -> Vec<(Gate, String)> {
        check_catalogue(&dir.path().join("catalogue"), None)
            .into_iter()
            .map(|f| (f.gate, f.message))
            .collect()
    }

    #[test]
    fn a_clean_world_has_no_findings_offline() {
        let dir = world("");
        assert_eq!(gates(&dir), Vec::new());
    }

    #[test]
    fn a_required_input_without_a_rows_file_is_refused_and_a_rows_file_binds_it() {
        let dir = world(REQUIRED);
        assert!(matches!(
            gates(&dir).as_slice(),
            [(Gate::Rows, message)] if message.contains("`units`")
        ));
        dir.child("catalogue/cases/ehr/w.rows.yaml")
            .write_str("workflow: ehr/w\niteration: reset_per_row\nrows:\n  - { units: cm }\n")
            .unwrap();
        assert_eq!(gates(&dir), Vec::new());
    }

    #[test]
    fn seeded_rows_files_are_refused() {
        let good = "workflow: ehr/w\niteration: reset_per_row\nrows:\n  - { units: cm }\n";
        for (path, body, fragment) in [
            (
                "catalogue/cases/ehr/w.rows.yaml",
                "workflow: ehr/w\niteration: reset_per_row\nrows:\n  - { unit: cm }\n",
                "`unit` is not an input",
            ),
            (
                "catalogue/cases/ehr/missing.rows.yaml",
                good,
                "names no workflow of a document in this folder",
            ),
            (
                "catalogue/cases/other/w.rows.yaml",
                good,
                "names no workflow of a document in this folder",
            ),
            (
                "catalogue/cases/ehr/w.rows.yaml",
                "workflow: ehr/x\niteration: reset_per_row\nrows:\n  - { units: cm }\n",
                "`workflow` is `ehr/x`",
            ),
            (
                "catalogue/cases/ehr/w.rows.yaml",
                "workflow: ehr/w\nrows:\n  - { units: cm }\n",
                "iteration",
            ),
        ] {
            let dir = world(REQUIRED);
            dir.child(path).write_str(body).unwrap();
            let found = gates(&dir);
            assert!(
                found
                    .iter()
                    .any(|(gate, message)| *gate == Gate::Rows && message.contains(fragment)),
                "{path}: {fragment}: {found:?}"
            );
        }
    }

    #[test]
    fn stray_misplaced_and_duplicate_documents_are_refused() {
        let dir = world("");
        dir.child("catalogue/cases/ehr/ehr.arazo.yaml")
            .write_str("x: 1\n")
            .unwrap();
        dir.child("catalogue/cases/top.arazzo.yaml")
            .write_str(&document(""))
            .unwrap();
        dir.child("catalogue/cases/other/ehr.arazzo.yaml")
            .write_str(&document(""))
            .unwrap();
        let found = gates(&dir);
        for fragment in [
            "holds `*.arazzo.yaml` documents",
            "never at the top",
            "already taken",
        ] {
            assert!(
                found
                    .iter()
                    .any(|(gate, message)| *gate == Gate::Load && message.contains(fragment)),
                "{fragment}: {found:?}"
            );
        }
    }

    #[test]
    fn an_unmodelled_document_reports_its_schema_violations() {
        let dir = world("");
        dir.child("catalogue/cases/ehr/broken.arazzo.yaml")
            .write_str("arazzo: 1.1.0\ninfo: { title: t, version: '1' }\nworkflows: 3\n")
            .unwrap();
        let found = gates(&dir);
        assert!(
            found.iter().any(|(gate, _)| *gate == Gate::Load),
            "{found:?}"
        );
        assert!(
            found.iter().any(|(gate, _)| *gate == Gate::Schema),
            "{found:?}"
        );
    }

    #[test]
    fn documents_without_a_surface_file_are_refused_once_specs_are_given() {
        let dir = world("");
        dir.child("specs/openehr/ITS-REST/specifications")
            .create_dir_all()
            .unwrap();
        let found: Vec<Gate> = check_catalogue(
            &dir.path().join("catalogue"),
            Some(&dir.path().join("specs/openehr")),
        )
        .into_iter()
        .map(|f| f.gate)
        .collect();
        assert!(found.contains(&Gate::Surface), "{found:?}");
    }

    #[test]
    fn a_capabilities_file_naming_an_unknown_or_doubled_document_is_refused() {
        let dir = world("");
        let caps = "options:\n  o:\n    description: d\n    arms:\n      a: { documents: [ehr, nowhere] }\n      b: { documents: [ehr] }\n    x-veredictum-cite: { source: specs/openehr/x.md, section: s, quote: q }\n";
        dir.child("catalogue/capabilities.yaml")
            .write_str(caps)
            .unwrap();
        let found = gates(&dir);
        for fragment in [
            "`nowhere` is not a document",
            "`ehr` is already under the arm",
        ] {
            assert!(
                found
                    .iter()
                    .any(|(gate, message)| *gate == Gate::Identity && message.contains(fragment)),
                "{fragment}: {found:?}"
            );
        }
        dir.child("catalogue/capabilities.yaml")
            .write_str("option: {}\n")
            .unwrap();
        assert!(gates(&dir).iter().any(|(gate, _)| *gate == Gate::Load));
    }

    #[test]
    fn a_seeded_decides_entry_no_arm_asserts_or_an_arm_does_not_call_is_refused() {
        let dir = world("");
        let caps = |decides: &str| {
            format!(
                "options:\n  o:\n    description: d\n    arms:\n      a: {{ documents: [ehr] }}\n      b: {{ documents: [] }}\n    x-veredictum-cite: {{ source: specs/openehr/x.md, section: s, quote: q }}\n    decides:\n      - api: ehr\n        operation: ehr_create_with_id\n        statuses: [{decides}]\n        x-veredictum-cite: {{ source: specs/openehr/x.md, section: s, quote: q }}\n"
            )
        };
        dir.child("catalogue/capabilities.yaml")
            .write_str(&caps("\"201\", \"409\""))
            .unwrap();
        let found = gates(&dir);
        for fragment in [
            "`ehr/ehr_create_with_id response 201`: the arm `b` does not call the operation",
            "`ehr/ehr_create_with_id response 409`: no arm asserts the status",
        ] {
            assert!(
                found
                    .iter()
                    .any(|(gate, message)| *gate == Gate::Identity && message == fragment),
                "{fragment}: {found:?}"
            );
        }
        dir.child("catalogue/capabilities.yaml")
            .write_str(&caps("\"201\""))
            .unwrap();
        assert!(
            gates(&dir).iter().any(|(gate, _)| *gate == Gate::Load),
            "one status decides nothing"
        );
    }

    #[test]
    fn seeded_aggregate_and_declared_defects_are_refused() {
        let dir = world(REQUIRED);
        let cite = "x-veredictum-cite: { source: specs/openehr/x.md, section: s, quote: q }";
        for (body, fragment) in [
            (
                format!("workflow: ehr/w\niteration: reset_per_row\naggregate:\n  - distinct: ehr_id\n    {cite}\nrows:\n  - {{ units: cm }}\n"),
                "`aggregate` needs `iteration: single_pass`",
            ),
            (
                format!("workflow: ehr/w\niteration: single_pass\naggregate:\n  - distinct: nothing\n    {cite}\nrows:\n  - {{ units: cm }}\n"),
                "not an output of the workflow",
            ),
            (
                "workflow: ehr/w\niteration: reset_per_row\ndeclared:\n  units: ixit.units\nrows:\n  - { units: cm }\n".to_owned(),
                "declared by the party and cannot be supplied",
            ),
            (
                "workflow: ehr/w\niteration: reset_per_row\ndeclared:\n  colour: ixit.colour\nrows:\n  - { units: cm }\n".to_owned(),
                "declared input `colour` is not an input",
            ),
            (
                "workflow: ehr/w\niteration: reset_per_row\ndeclared:\n  units: elsewhere.units\nrows:\n  - { units: cm }\n".to_owned(),
                "",
            ),
        ] {
            dir.child("catalogue/cases/ehr/w.rows.yaml")
                .write_str(&body)
                .unwrap();
            let found = gates(&dir);
            assert!(
                found
                    .iter()
                    .any(|(gate, message)| *gate == Gate::Rows && message.contains(fragment)),
                "{fragment}: {found:?}"
            );
        }
    }

    #[test]
    fn the_aql_gate_reads_each_row_of_a_query_workflow() {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("specs/rest-oas/query-validation.openapi.yaml")
            .write_str("openapi: 3.0.3\npaths:\n  /query/aql:\n    get:\n      operationId: query_execute_adhoc_query\n      parameters:\n        - { name: q, in: query, required: true }\n")
            .unwrap();
        dir.child("catalogue/cases/query/query.arazzo.yaml")
            .write_str("arazzo: 1.1.0\ninfo: { title: t, version: '1' }\nsourceDescriptions:\n  - { name: query, url: ../../../specs/rest-oas/query-validation.openapi.yaml, type: openapi }\nworkflows:\n  - workflowId: adhoc\n    x-veredictum-cite: { source: specs/rest-oas/query-validation.openapi.yaml, section: openapi, quote: x }\n    inputs:\n      type: object\n      required: [q]\n      properties:\n        q: { type: string }\n    steps:\n      - stepId: execute\n        operationId: $sourceDescriptions.query.query_execute_adhoc_query\n        parameters: [{ name: q, in: query, value: $inputs.q }]\n        successCriteria:\n          - condition: $statusCode == 200\n")
            .unwrap();
        dir.child("catalogue/cases/query/adhoc.rows.yaml")
            .write_str("workflow: query/adhoc\niteration: single_pass\nrows:\n  - { q: 'SELECT e/ehr_id/value FROM EHR e' }\n  - { q: 'SELECT e/ehr_id/value EHR e' }\n")
            .unwrap();
        let found = gates(&dir);
        assert!(
            matches!(found.as_slice(), [(Gate::Aql, message)] if message.contains("case `query/adhoc` row 1")),
            "{found:?}"
        );
    }

    #[test]
    fn a_permission_only_quote_needs_level_may_on_a_criterion_and_on_an_option() {
        let permits = "The server MAY answer 202.";
        let levels = |dir: &assert_fs::TempDir| -> Vec<String> {
            gates(dir)
                .into_iter()
                .filter(|(gate, _)| *gate == Gate::Level)
                .map(|(_, message)| message)
                .collect()
        };
        let needs_may = |found: &[String]| {
            found.len() == 1 && found.iter().all(|m| m.contains("needs `level: may`"))
        };
        let dir = world("");
        let criterion = |level: &str| {
            document("").replace(
                "section: openapi, quote: x }",
                &format!("section: openapi, quote: '{permits}'{level} }}"),
            )
        };
        let case = "catalogue/cases/ehr/ehr.arazzo.yaml";
        dir.child(case).write_str(&criterion("")).unwrap();
        let found = levels(&dir);
        assert!(needs_may(&found), "case criterion at `must`: {found:?}");
        dir.child(case)
            .write_str(&criterion(", level: may"))
            .unwrap();
        assert_eq!(
            levels(&dir),
            Vec::<String>::new(),
            "case criterion at `may`"
        );
        let option = |level: &str| {
            format!(
                "options:\n  o:\n    description: d\n    arms:\n      a: {{ documents: [ehr] }}\n      b: {{ documents: [] }}\n    x-veredictum-cite: {{ source: specs/openehr/x.md, section: s, quote: '{permits}'{level} }}\n"
            )
        };
        let caps = "catalogue/capabilities.yaml";
        dir.child(caps).write_str(&option("")).unwrap();
        let found = levels(&dir);
        assert!(needs_may(&found), "option citation at `must`: {found:?}");
        dir.child(caps).write_str(&option(", level: may")).unwrap();
        assert_eq!(
            levels(&dir),
            Vec::<String>::new(),
            "option citation at `may`"
        );
    }

    #[test]
    fn every_gate_token_is_distinct_and_prefixed() {
        let all = [
            Gate::Load,
            Gate::Schema,
            Gate::Keys,
            Gate::Source,
            Gate::Citation,
            Gate::Level,
            Gate::Profile,
            Gate::Rows,
            Gate::Surface,
            Gate::Identity,
            Gate::Aql,
        ];
        let mut tokens: Vec<&str> = all.iter().map(|g| g.token()).collect();
        assert!(tokens.iter().all(|t| t.starts_with("arazzo-")));
        tokens.sort_unstable();
        tokens.dedup();
        assert_eq!(tokens.len(), all.len());
    }
}
