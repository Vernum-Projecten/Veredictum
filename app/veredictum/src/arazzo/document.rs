// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! One Arazzo 1.1.0 document: loaded with `roas-arazzo`, its sources indexed,
//! and checked by the catalogue gate.
//!
//! The gate refuses what the evaluator would otherwise have to guess about:
//! a key Arazzo does not define, an extension outside the two the catalogue
//! allows, a criterion with no citation, a citation whose quote is not in its
//! source, a condition outside the profile, an operation that does not
//! resolve, and a construct the evaluator does not drive yet.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use enumset::EnumSet;
use roas_arazzo::common::reusable::ReusableOr;
use roas_arazzo::v1_1::{
    Criterion, CriterionKind, CriterionType, Description, ParameterLocation, SourceType, Step,
    ValueOrSelector, Workflow,
};
use roas_arazzo::validation::Validate;
use serde_json::Value;

use crate::arazzo::cite::{CITE_KEY, Citation, CiteError, QuoteChecker, citation_of};
use crate::arazzo::expr::{Expr, ParseError, RegexCriterion, SimpleCondition};
use crate::arazzo::source::{Operation, OperationIndex, SourceError, split_qualified};

/// The specification-extension key of the catalogue's own judges.
pub const CHECK_KEY: &str = "x-veredictum-check";

/// Why a document cannot be loaded at all.
#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    /// The file does not read or parse as YAML.
    #[error(transparent)]
    Load(#[from] crate::load::LoadError),
    /// The YAML does not deserialize into the Arazzo 1.1.0 model.
    #[error("`{path}` is not an Arazzo 1.1.0 document: {reason}")]
    Model {
        /// The document.
        path: PathBuf,
        /// The verbatim deserialization failure.
        reason: String,
    },
    /// A source description is not an explicit `openapi` source.
    #[error("source `{0}` must declare `type: openapi`")]
    SourceType(String),
    /// A source description does not index.
    #[error(transparent)]
    Source(#[from] SourceError),
}

/// A loaded Arazzo document with its sources indexed.
#[derive(Debug)]
pub struct ArazzoDocument {
    path: PathBuf,
    raw: Value,
    description: Description,
    sources: BTreeMap<String, OperationIndex>,
}

impl ArazzoDocument {
    /// Loads the document at `path` and indexes every source it names,
    /// resolving each source `url` against the document's directory.
    ///
    /// # Errors
    /// [`DocumentError`] when the file, the model or a source does not load.
    pub fn load(path: &Path) -> Result<Self, DocumentError> {
        let raw = crate::load::yaml_file_to_value(path)?;
        let description: Description =
            serde_json::from_value(raw.clone()).map_err(|error| DocumentError::Model {
                path: path.to_owned(),
                reason: error.to_string(),
            })?;
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        let mut sources = BTreeMap::new();
        for source in &description.source_descriptions {
            if source.type_ != Some(SourceType::Openapi) {
                return Err(DocumentError::SourceType(source.name.clone()));
            }
            if source.url.contains("://") || Path::new(&source.url).is_absolute() {
                return Err(SourceError::NotLocal(source.url.clone()).into());
            }
            sources.insert(
                source.name.clone(),
                OperationIndex::load(&base.join(&source.url))?,
            );
        }
        Ok(Self {
            path: path.to_owned(),
            raw,
            description,
            sources,
        })
    }

    /// The document's file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The parsed Arazzo model.
    #[must_use]
    pub fn description(&self) -> &Description {
        &self.description
    }

    /// The workflow named `workflow_id`.
    #[must_use]
    pub fn workflow(&self, workflow_id: &str) -> Option<&Workflow> {
        self.description
            .workflows
            .iter()
            .find(|w| w.workflow_id == workflow_id)
    }

    /// Resolves a qualified `$sourceDescriptions.<name>.<operationId>`.
    #[must_use]
    pub fn operation(&self, qualified: &str) -> Option<&Operation> {
        let (source, operation_id) = split_qualified(qualified)?;
        self.sources.get(source)?.get(operation_id)
    }

    /// Runs the catalogue gate over the document; an empty list is the only
    /// passing result.
    ///
    /// `repo_root` is the directory every citation `source` is relative to.
    #[must_use]
    pub fn check(&self, repo_root: &Path) -> Vec<Finding> {
        let mut findings = Vec::new();
        if let Err(error) = self.description.validate(EnumSet::empty()) {
            for e in error.errors {
                findings.push(Finding::new(e.path, FindingKind::Model(e.message)));
            }
        }
        match serde_json::to_value(&self.description) {
            Ok(back) => unknown_keys(&self.raw, &back, "#", &mut findings),
            Err(error) => findings.push(Finding::new("#", FindingKind::Model(error.to_string()))),
        }
        extension_keys(&self.raw, &mut Vec::new(), &mut findings);
        let mut quotes = QuoteChecker::new(repo_root);
        for workflow in &self.description.workflows {
            self.check_workflow(workflow, &mut quotes, &mut findings);
        }
        findings
    }

    fn check_workflow(
        &self,
        workflow: &Workflow,
        quotes: &mut QuoteChecker<'_>,
        findings: &mut Vec<Finding>,
    ) {
        let at = workflow.workflow_id.clone();
        let workflow_cite = cite_at(workflow.extensions.as_ref(), &at, quotes, findings);
        if !workflow.depends_on.is_empty()
            || !workflow.success_actions.is_empty()
            || !workflow.failure_actions.is_empty()
            || !workflow.outputs.is_empty()
            || !workflow.parameters.is_empty()
        {
            findings.push(Finding::new(
                &at,
                FindingKind::Unsupported(
                    "dependsOn, actions, outputs and parameters on a workflow".to_owned(),
                ),
            ));
        }
        for step in &workflow.steps {
            let at = format!("{at}/{}", step.step_id);
            let step_cite = cite_at(step.extensions.as_ref(), &at, quotes, findings);
            self.check_step(step, &at, findings);
            for (index, criterion) in step.success_criteria.iter().enumerate() {
                let at = format!("{at}/successCriteria[{index}]");
                let own = cite_at(criterion.extensions.as_ref(), &at, quotes, findings);
                if own
                    .or(step_cite.clone())
                    .or(workflow_cite.clone())
                    .is_none()
                {
                    findings.push(Finding::new(&at, FindingKind::MissingCitation));
                }
                if let Err(kind) = CriterionPlan::of(criterion) {
                    findings.push(Finding::new(&at, kind));
                }
            }
        }
    }

    fn check_step(&self, step: &Step, at: &str, findings: &mut Vec<Finding>) {
        let mut push = |kind| findings.push(Finding::new(at, kind));
        if step.operation_path.is_some()
            || step.workflow_id.is_some()
            || step.channel_path.is_some()
            || !step.depends_on.is_empty()
            || !step.on_success.is_empty()
            || !step.on_failure.is_empty()
        {
            push(FindingKind::Unsupported(
                "operationPath, workflowId, channelPath, dependsOn and step actions".to_owned(),
            ));
        }
        let Some(qualified) = &step.operation_id else {
            push(FindingKind::UnresolvedOperation(
                "no operationId".to_owned(),
            ));
            return;
        };
        let Some(operation) = self.operation(qualified) else {
            push(FindingKind::UnresolvedOperation(qualified.clone()));
            return;
        };
        let mut path_parameters = Vec::new();
        for parameter in &step.parameters {
            let ReusableOr::Item(parameter) = parameter else {
                push(FindingKind::Unsupported("a reusable parameter".to_owned()));
                continue;
            };
            match parameter.in_ {
                Some(ParameterLocation::Path) => path_parameters.push(parameter.name.clone()),
                Some(ParameterLocation::Query | ParameterLocation::Header) => {}
                other => push(FindingKind::Unsupported(format!(
                    "parameter `{}` in {other:?}",
                    parameter.name
                ))),
            }
            if let Err(kind) = value_expression(&parameter.value) {
                push(kind);
            }
        }
        for placeholder in placeholders(&operation.path) {
            if !path_parameters.contains(&placeholder) {
                push(FindingKind::UnresolvedParameter(placeholder));
            }
        }
        for name in path_parameters {
            if !placeholders(&operation.path).contains(&name) {
                push(FindingKind::UnresolvedParameter(name));
            }
        }
        if let Some(body) = &step.request_body {
            for replacement in &body.replacements {
                if !replacement.target.starts_with('/')
                    || replacement.target_selector_type.is_some()
                {
                    push(FindingKind::Unsupported(format!(
                        "replacement target `{}` (a JSON Pointer is required)",
                        replacement.target
                    )));
                }
                if let Err(kind) = value_expression(&replacement.value) {
                    push(kind);
                }
            }
        }
        for value in step.outputs.values() {
            match value {
                ValueOrSelector::Literal(Value::String(text)) => {
                    if let Err(error) = Expr::parse(text) {
                        push(FindingKind::Profile(error));
                    }
                }
                _ => push(FindingKind::Unsupported(
                    "an output that is not a runtime expression".to_owned(),
                )),
            }
        }
    }
}

/// Reads and checks the citation at one location; returns it when it is
/// well-formed.
fn cite_at(
    extensions: Option<&BTreeMap<String, Value>>,
    at: &str,
    quotes: &mut QuoteChecker<'_>,
    findings: &mut Vec<Finding>,
) -> Option<Citation> {
    match citation_of(extensions) {
        Ok(Some(citation)) => {
            if let Err(error) = quotes.check(&citation) {
                findings.push(Finding::new(at, FindingKind::Citation(error)));
            }
            Some(citation)
        }
        Ok(None) => None,
        Err(error) => {
            findings.push(Finding::new(at, FindingKind::Citation(error)));
            None
        }
    }
}

/// Checks a parameter or replacement value: a literal, or a runtime
/// expression that parses.
fn value_expression(value: &ValueOrSelector) -> Result<(), FindingKind> {
    match value {
        ValueOrSelector::Literal(Value::String(text)) if text.starts_with('$') => {
            Expr::parse(text).map(drop).map_err(FindingKind::Profile)
        }
        ValueOrSelector::Literal(_) => Ok(()),
        ValueOrSelector::Selector(_) => {
            Err(FindingKind::Unsupported("a selector value".to_owned()))
        }
    }
}

/// The `{name}` placeholders of a path template.
fn placeholders(template: &str) -> Vec<String> {
    template
        .split('{')
        .skip(1)
        .filter_map(|part| part.split_once('}').map(|(name, _)| name.to_owned()))
        .collect()
}

/// A criterion as the evaluator judges it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CriterionPlan {
    /// A `simple` condition.
    Simple(SimpleCondition),
    /// A `regex` condition over a context.
    Regex(RegexCriterion),
}

impl CriterionPlan {
    /// Plans one criterion under the profile.
    ///
    /// # Errors
    /// The [`FindingKind`] that refuses it.
    pub fn of(criterion: &Criterion) -> Result<Self, FindingKind> {
        if criterion
            .extensions
            .as_ref()
            .is_some_and(|ext| ext.contains_key(CHECK_KEY))
        {
            // TODO(#600): drive the `result_set`, `equivalent` and `signature` judges.
            return Err(FindingKind::Unsupported(format!("`{CHECK_KEY}`")));
        }
        match &criterion.type_ {
            None | Some(CriterionType::Simple(CriterionKind::Simple)) => {
                SimpleCondition::parse(&criterion.condition)
                    .map(Self::Simple)
                    .map_err(FindingKind::Profile)
            }
            Some(CriterionType::Simple(CriterionKind::Regex)) => {
                let Some(context) = &criterion.context else {
                    return Err(FindingKind::Model(
                        "a regex criterion needs a context".to_owned(),
                    ));
                };
                RegexCriterion::parse(context, &criterion.condition)
                    .map(Self::Regex)
                    .map_err(FindingKind::Profile)
            }
            // TODO(#600): evaluate `jsonpath` (RFC 9535) and `xpath` criteria.
            Some(other) => Err(FindingKind::Unsupported(format!(
                "criterion type {other:?}"
            ))),
        }
    }
}

/// One gate finding: where, and what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `workflowId/stepId/successCriteria[n]`, or a JSON Pointer-style path.
    pub location: String,
    /// What is wrong.
    pub kind: FindingKind,
}

impl Finding {
    fn new(location: impl Into<String>, kind: FindingKind) -> Self {
        Self {
            location: location.into(),
            kind,
        }
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.location, self.kind)
    }
}

/// What a gate finding refuses.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FindingKind {
    /// The Arazzo model's own validation refuses it.
    #[error("Arazzo 1.1.0: {0}")]
    Model(String),
    /// A key Arazzo 1.1.0 does not define, which the model would drop.
    #[error("`{0}` is not a field Arazzo 1.1.0 defines here")]
    UnknownKey(String),
    /// An extension other than the two the catalogue allows, or one in the
    /// wrong place.
    #[error("extension `{0}` is not allowed here")]
    ExtensionNotAllowed(String),
    /// A criterion that resolves to no citation through inheritance.
    #[error("the criterion resolves to no `{CITE_KEY}`")]
    MissingCitation,
    /// A citation that does not hold.
    #[error(transparent)]
    Citation(CiteError),
    /// An expression or condition outside the profile.
    #[error(transparent)]
    Profile(ParseError),
    /// An `operationId` that does not resolve in its named source.
    #[error("operation `{0}` does not resolve in its source")]
    UnresolvedOperation(String),
    /// A path parameter that does not match the operation's path template.
    #[error("path parameter `{0}` does not match the operation's path template")]
    UnresolvedParameter(String),
    /// A construct the evaluator does not drive.
    #[error("not driven by the evaluator: {0}")]
    Unsupported(String),
}

/// Reports every key present in `raw` that the model's round trip dropped.
fn unknown_keys(raw: &Value, back: &Value, at: &str, findings: &mut Vec<Finding>) {
    match (raw, back) {
        (Value::Object(raw), Value::Object(back)) => {
            for (key, value) in raw {
                if key.starts_with("x-") {
                    continue;
                }
                let at = format!("{at}/{key}");
                match back.get(key) {
                    Some(kept) => unknown_keys(value, kept, &at, findings),
                    None => findings.push(Finding::new(&at, FindingKind::UnknownKey(key.clone()))),
                }
            }
        }
        (Value::Array(raw), Value::Array(back)) => {
            for (index, (value, kept)) in raw.iter().zip(back).enumerate() {
                unknown_keys(value, kept, &format!("{at}/{index}"), findings);
            }
        }
        _ => {}
    }
}

/// Reports every `x-` key that is not one of the catalogue's two extensions
/// at a place it is defined for. Request payloads are data and are skipped.
fn extension_keys(value: &Value, at: &mut Vec<String>, findings: &mut Vec<Finding>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if key.starts_with("x-") {
                    let allowed = match key.as_str() {
                        CITE_KEY => is_workflow(at) || is_step(at) || is_criterion(at),
                        CHECK_KEY => is_criterion(at),
                        _ => false,
                    };
                    if !allowed {
                        findings.push(Finding::new(
                            format!("#/{}", at.join("/")),
                            FindingKind::ExtensionNotAllowed(key.clone()),
                        ));
                    }
                    continue;
                }
                if key == "payload" {
                    continue;
                }
                at.push(key.clone());
                extension_keys(child, at, findings);
                at.pop();
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                at.push(index.to_string());
                extension_keys(child, at, findings);
                at.pop();
            }
        }
        _ => {}
    }
}

fn is_workflow(at: &[String]) -> bool {
    matches!(at, [w, _] if w == "workflows")
}

fn is_step(at: &[String]) -> bool {
    matches!(at, [w, _, s, _] if w == "workflows" && s == "steps")
}

fn is_criterion(at: &[String]) -> bool {
    matches!(at, [w, _, s, _, c, _] if w == "workflows" && s == "steps" && c == "successCriteria")
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;

    const OAS: &str =
        "openapi: 3.0.3\npaths:\n  /ehr/{ehr_id}:\n    get:\n      operationId: ehr_get_by_id\n";
    const RESPONSE: &str =
        "description: |\n  `404 Not Found` is returned when an EHR with `ehr_id` does not exist.\n";

    fn fixture(workflow: &str) -> (assert_fs::TempDir, ArazzoDocument) {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("specs/rest-oas/ehr.yaml").write_str(OAS).unwrap();
        dir.child("specs/openehr/ITS-REST/specifications/responses/404.yaml")
            .write_str(RESPONSE)
            .unwrap();
        let doc = format!(
            "arazzo: 1.1.0\ninfo: {{ title: t, version: '1' }}\nsourceDescriptions:\n  - {{ name: ehr, url: ../../specs/rest-oas/ehr.yaml, type: openapi }}\nworkflows:\n{workflow}"
        );
        let file = dir.child("artifacts/workflows/ehr.arazzo.yaml");
        file.write_str(&doc).unwrap();
        let loaded = ArazzoDocument::load(file.path()).unwrap();
        (dir, loaded)
    }

    const CITE: &str =
        "source: specs/openehr/ITS-REST/specifications/responses/404.yaml, section: description";

    fn workflow(workflow_ext: &str, criterion_ext: &str, condition: &str) -> String {
        format!(
            "  - workflowId: w\n{workflow_ext}    steps:\n      - stepId: get\n        operationId: $sourceDescriptions.ehr.ehr_get_by_id\n        parameters: [{{ name: ehr_id, in: path, value: $inputs.ehr_id }}]\n        successCriteria:\n          - condition: {condition}\n{criterion_ext}"
        )
    }

    #[test]
    fn a_criterion_inherits_its_workflow_citation() {
        let ext = format!(
            "    x-veredictum-cite: {{ {CITE}, quote: '`404 Not Found` is returned when an EHR with `ehr_id` does not exist.' }}\n"
        );
        let (dir, doc) = fixture(&workflow(&ext, "", "$statusCode == 404"));
        assert_eq!(doc.check(dir.path()), Vec::new());
    }

    #[test]
    fn a_criterion_with_no_citation_anywhere_is_refused() {
        let (dir, doc) = fixture(&workflow("", "", "$statusCode == 404"));
        assert_eq!(
            doc.check(dir.path()),
            vec![Finding::new(
                "w/get/successCriteria[0]",
                FindingKind::MissingCitation
            )]
        );
    }

    #[test]
    fn a_seeded_quote_that_does_not_occur_is_refused() {
        let ext = format!(
            "            x-veredictum-cite: {{ {CITE}, quote: '`404 Not Found` is returned when the EHR was deleted.' }}\n"
        );
        let (dir, doc) = fixture(&workflow("", &ext, "$statusCode == 404"));
        let findings = doc.check(dir.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(matches!(
            findings.first().map(|f| &f.kind),
            Some(FindingKind::Citation(CiteError::QuoteNotFound { .. }))
        ));
    }

    #[test]
    fn unknown_keys_foreign_extensions_and_profile_breaches_are_refused() {
        let ext = format!(
            "    x-veredictum-cite: {{ {CITE}, quote: '`404 Not Found` is returned when an EHR with `ehr_id` does not exist.' }}\n    x-veredictum-outcome: failed\n    succesCriteria: []\n"
        );
        let (dir, doc) = fixture(&workflow(&ext, "", "\"$response.body#/a == 'x'\""));
        let kinds: Vec<FindingKind> = doc.check(dir.path()).into_iter().map(|f| f.kind).collect();
        assert!(
            kinds.contains(&FindingKind::UnknownKey("succesCriteria".to_owned())),
            "{kinds:?}"
        );
        assert!(
            kinds.contains(&FindingKind::ExtensionNotAllowed(
                "x-veredictum-outcome".to_owned()
            )),
            "{kinds:?}"
        );
        assert!(
            kinds
                .iter()
                .any(|k| matches!(k, FindingKind::Profile(ParseError::StringComparison(_)))),
            "{kinds:?}"
        );
    }

    #[test]
    fn an_unresolved_operation_is_refused() {
        let ext = format!(
            "    x-veredictum-cite: {{ {CITE}, quote: '`404 Not Found` is returned when an EHR with `ehr_id` does not exist.' }}\n"
        );
        let text =
            workflow(&ext, "", "$statusCode == 404").replace("ehr_get_by_id", "ehr_get_by_idx");
        let (dir, doc) = fixture(&text);
        assert_eq!(
            doc.check(dir.path()),
            vec![Finding::new(
                "w/get",
                FindingKind::UnresolvedOperation(
                    "$sourceDescriptions.ehr.ehr_get_by_idx".to_owned()
                )
            )]
        );
    }
}
