// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `validate` gates for the Arazzo catalogue under an artifact root.
//!
//! The documents are `workflows/<capability>/<topic>.arazzo.yaml`, each rows
//! file sits beside the document whose workflow it feeds as
//! `<workflowId>.rows.yaml`, and the surface file is `workflows/surface.yaml`. Every file is read once, every finding names its
//! [`Gate`], and the output is sorted, so two runs over one tree print the
//! same bytes.

pub mod aql;
pub mod profile;
pub mod published;
pub mod rows;
pub mod schema;
pub mod surface;

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::arazzo::document::{ArazzoDocument, source_finding};

/// The suffix every Arazzo document carries.
pub const DOCUMENT_SUFFIX: &str = ".arazzo.yaml";

/// The gates, one per kind of refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Gate {
    /// A file that does not read, parse, or carry a unique id.
    Load,
    /// The official Arazzo 1.1 JSON Schema, the model's own validation, and
    /// the `workflowId` alphabet.
    Schema,
    /// A key Arazzo does not define, or an extension outside the two.
    Keys,
    /// A source, an `operationId` or a parameter that does not resolve.
    Source,
    /// A criterion with no citation, or a citation that does not hold.
    Citation,
    /// A condition or expression outside the condition profile.
    Profile,
    /// A rows file that does not validate or bind its workflow's inputs.
    Rows,
    /// A scoped surface element with neither a covering step nor a gap.
    Surface,
    /// An AQL query that does not parse outside a step expecting `400`.
    Aql,
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
            Self::Profile => "arazzo-profile",
            Self::Rows => "arazzo-rows",
            Self::Surface => "arazzo-surface",
            Self::Aql => "arazzo-aql",
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
    fn new(
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

/// Runs every Arazzo gate over the catalogue under `artifact_root`.
///
/// `spec_root` is the vendored openEHR tree (`<repo>/specs/openehr`). With
/// it, every quote is looked up in the files under its repository root
/// (`<spec_root>/../..`) and the scoped surface is enumerated from the
/// decomposed ITS-REST OAS; without it, those two checks do not run, the way
/// every other spec-resolving gate of `validate` does not.
#[must_use]
pub fn check_catalogue(artifact_root: &Path, spec_root: Option<&Path>) -> Vec<GateFinding> {
    let mut findings = Vec::new();
    let repo_root = spec_root.map(|specs| specs.join("../.."));
    let workflows = artifact_root.join("workflows");
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
                    "workflows/ holds `*{DOCUMENT_SUFFIX}` documents, `*{}` rows files and `{}` only",
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
                "a document or rows file sits in `workflows/<capability>/`, never at the top",
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
    let rows = check_rows_files(&rows_files, &documents, &mut findings);
    check_aql(&documents, &rows, &mut findings);
    if let Some(specs) = spec_root {
        check_surface(&workflows, specs, &documents, &mut findings);
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
    findings: &mut Vec<GateFinding>,
) -> BTreeMap<(String, String), rows::RowsFile> {
    let mut bound: BTreeMap<(String, String), rows::RowsFile> = BTreeMap::new();
    for path in rows_files {
        let (found, binds) = check_rows_file(path, documents);
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

/// The surface gate, over the scope `workflows/surface.yaml` declares.
fn check_surface(
    workflows: &Path,
    spec_root: &Path,
    documents: &BTreeMap<String, Located>,
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
    let covered = surface::covered(documents.values().map(|located| &located.document));
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
        dir.child("artifacts/workflows/ehr/ehr.arazzo.yaml")
            .write_str(&document(inputs))
            .unwrap();
        dir
    }

    fn gates(dir: &assert_fs::TempDir) -> Vec<(Gate, String)> {
        check_catalogue(&dir.path().join("artifacts"), None)
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
        dir.child("artifacts/workflows/ehr/w.rows.yaml")
            .write_str("workflow: ehr/w\niteration: reset_per_row\nrows:\n  - { units: cm }\n")
            .unwrap();
        assert_eq!(gates(&dir), Vec::new());
    }

    #[test]
    fn seeded_rows_files_are_refused() {
        let good = "workflow: ehr/w\niteration: reset_per_row\nrows:\n  - { units: cm }\n";
        for (path, body, fragment) in [
            (
                "artifacts/workflows/ehr/w.rows.yaml",
                "workflow: ehr/w\niteration: reset_per_row\nrows:\n  - { unit: cm }\n",
                "`unit` is not an input",
            ),
            (
                "artifacts/workflows/ehr/missing.rows.yaml",
                good,
                "names no workflow of a document in this folder",
            ),
            (
                "artifacts/workflows/other/w.rows.yaml",
                good,
                "names no workflow of a document in this folder",
            ),
            (
                "artifacts/workflows/ehr/w.rows.yaml",
                "workflow: ehr/x\niteration: reset_per_row\nrows:\n  - { units: cm }\n",
                "`workflow` is `ehr/x`",
            ),
            (
                "artifacts/workflows/ehr/w.rows.yaml",
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
        dir.child("artifacts/workflows/ehr/ehr.arazo.yaml")
            .write_str("x: 1\n")
            .unwrap();
        dir.child("artifacts/workflows/top.arazzo.yaml")
            .write_str(&document(""))
            .unwrap();
        dir.child("artifacts/workflows/other/ehr.arazzo.yaml")
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
        dir.child("artifacts/workflows/ehr/broken.arazzo.yaml")
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
            &dir.path().join("artifacts"),
            Some(&dir.path().join("specs/openehr")),
        )
        .into_iter()
        .map(|f| f.gate)
        .collect();
        assert!(found.contains(&Gate::Surface), "{found:?}");
    }

    #[test]
    fn the_aql_gate_reads_each_row_of_a_query_workflow() {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("specs/rest-oas/query-validation.openapi.yaml")
            .write_str("openapi: 3.0.3\npaths:\n  /query/aql:\n    get:\n      operationId: query_execute_adhoc_query\n      parameters:\n        - { name: q, in: query, required: true }\n")
            .unwrap();
        dir.child("artifacts/workflows/query/query.arazzo.yaml")
            .write_str("arazzo: 1.1.0\ninfo: { title: t, version: '1' }\nsourceDescriptions:\n  - { name: query, url: ../../../specs/rest-oas/query-validation.openapi.yaml, type: openapi }\nworkflows:\n  - workflowId: adhoc\n    x-veredictum-cite: { source: specs/rest-oas/query-validation.openapi.yaml, section: openapi, quote: x }\n    inputs:\n      type: object\n      required: [q]\n      properties:\n        q: { type: string }\n    steps:\n      - stepId: execute\n        operationId: $sourceDescriptions.query.query_execute_adhoc_query\n        parameters: [{ name: q, in: query, value: $inputs.q }]\n        successCriteria:\n          - condition: $statusCode == 200\n")
            .unwrap();
        dir.child("artifacts/workflows/query/adhoc.rows.yaml")
            .write_str("workflow: query/adhoc\niteration: single_pass\nrows:\n  - { q: 'SELECT e/ehr_id/value FROM EHR e' }\n  - { q: 'SELECT e/ehr_id/value EHR e' }\n")
            .unwrap();
        let found = gates(&dir);
        assert!(
            matches!(found.as_slice(), [(Gate::Aql, message)] if message.contains("case `query/adhoc` row 1")),
            "{found:?}"
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
            Gate::Profile,
            Gate::Rows,
            Gate::Surface,
            Gate::Aql,
        ];
        let mut tokens: Vec<&str> = all.iter().map(|g| g.token()).collect();
        assert!(tokens.iter().all(|t| t.starts_with("arazzo-")));
        tokens.sort_unstable();
        tokens.dedup();
        assert_eq!(tokens.len(), all.len());
    }
}
