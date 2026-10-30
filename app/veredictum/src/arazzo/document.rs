// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! One Arazzo 1.1.0 document: loaded with `roas-arazzo`, its sources indexed,
//! and checked by the catalogue gate.
//!
//! The gate refuses what the evaluator would otherwise have to guess about:
//! a document the official Arazzo schema refuses, a key Arazzo does not
//! define, an extension outside the three the catalogue allows, a criterion
//! with no citation, a citation whose quote is not in its source, a condition
//! outside the profile, an operation or parameter that does not resolve, and
//! a construct the evaluator does not drive yet.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use enumset::EnumSet;
use roas_arazzo::common::reusable::ReusableOr;
use roas_arazzo::v1_1::{
    Criterion, CriterionKind, CriterionType, Description, ExpressionKind, ExpressionType,
    ParameterLocation, RequestBody, SourceType, Step, ValueOrSelector, Workflow,
};
use roas_arazzo::validation::Validate;
use serde_json::Value;

use crate::arazzo::check::Check;
use crate::arazzo::cite::{CITE_KEY, Citation, CiteError, QuoteChecker, check_level, citation_of};
use crate::arazzo::control::{CONTROL_KEY, Control, ControlError, control_of, is_refusal};
use crate::arazzo::derive::{DERIVE_KEY, DeriveError, derivations_of};
use crate::arazzo::expr::{Expr, JsonPathCriterion, ParseError, RegexCriterion, SimpleCondition};
use crate::arazzo::gates::Gate;
use crate::arazzo::gates::profile::{ProfileHeader, RegexBreach, lint_regex};
use crate::arazzo::principal::{PRINCIPAL_KEY, PrincipalError, principal_of};
use crate::arazzo::scope::{SCOPE_KEY, ScopeError, scope_of};
use crate::arazzo::structure::SECTION_KEY;
use crate::arazzo::source::{
    DeclaredParameter, Operation, OperationIndex, ParameterError, ParameterIn, ParameterIndex,
    SourceError, split_qualified,
};

/// Every workflow of one `type: arazzo` source, by id, with whether it is a
/// refusal workflow.
type Dependency = BTreeMap<String, bool>;

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
    /// A source description is neither an explicit `openapi` nor an
    /// explicit `arazzo` source.
    #[error("source `{0}` must declare `type: openapi` or `type: arazzo`")]
    SourceType(String),
    /// A source description does not index.
    #[error(transparent)]
    Source(#[from] SourceError),
    /// A source description's parameters do not resolve.
    #[error(transparent)]
    Parameters(#[from] ParameterError),
}

/// A loaded Arazzo document with its sources indexed.
#[derive(Debug)]
pub struct ArazzoDocument {
    path: PathBuf,
    raw: Value,
    description: Description,
    sources: BTreeMap<String, OperationIndex>,
    parameters: BTreeMap<String, ParameterIndex>,
    workflow_sources: BTreeMap<String, PathBuf>,
}

impl ArazzoDocument {
    /// Loads the document at `path` and indexes every source it names,
    /// resolving each source `url` against the document's directory.
    ///
    /// # Errors
    /// [`DocumentError`] when the file, the model or a source does not load.
    pub fn load(path: &Path) -> Result<Self, DocumentError> {
        let raw = crate::load::yaml_file_to_value(path)?;
        let (document, problems) = Self::from_value(path, raw)?;
        match problems.into_iter().next() {
            Some(problem) => Err(problem),
            None => Ok(document),
        }
    }

    /// Builds the document at `path` from its parsed YAML, indexing every
    /// source it can. A source that does not index is returned beside the
    /// document instead of failing it, so a gate can report every one.
    ///
    /// # Errors
    /// [`DocumentError::Model`] when `raw` is not an Arazzo 1.1.0 document.
    pub fn from_value(
        path: &Path,
        raw: Value,
    ) -> Result<(Self, Vec<DocumentError>), DocumentError> {
        let description: Description =
            serde_json::from_value(raw.clone()).map_err(|error| DocumentError::Model {
                path: path.to_owned(),
                reason: error.to_string(),
            })?;
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        let mut sources = BTreeMap::new();
        let mut parameters = BTreeMap::new();
        let mut workflow_sources = BTreeMap::new();
        let mut problems = Vec::new();
        for source in &description.source_descriptions {
            let arazzo = match source.type_ {
                Some(SourceType::Openapi) => false,
                Some(SourceType::Arazzo) => true,
                _ => {
                    problems.push(DocumentError::SourceType(source.name.clone()));
                    continue;
                }
            };
            if source.url.contains("://") || Path::new(&source.url).is_absolute() {
                problems.push(SourceError::NotLocal(source.url.clone()).into());
                continue;
            }
            let file = base.join(&source.url);
            if arazzo {
                workflow_sources.insert(source.name.clone(), file);
                continue;
            }
            match OperationIndex::load(&file) {
                Ok(index) => {
                    sources.insert(source.name.clone(), index);
                }
                Err(error) => {
                    problems.push(error.into());
                    continue;
                }
            }
            match ParameterIndex::load(&file) {
                Ok(index) => {
                    parameters.insert(source.name.clone(), index);
                }
                Err(error) => problems.push(error.into()),
            }
        }
        let document = Self {
            path: path.to_owned(),
            raw,
            description,
            sources,
            parameters,
            workflow_sources,
        };
        Ok((document, problems))
    }

    /// The document's file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The document as read from YAML, before the model saw it.
    #[must_use]
    pub fn raw(&self) -> &Value {
        &self.raw
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

    /// The file of every `type: arazzo` source, by source name.
    #[must_use]
    pub fn workflow_sources(&self) -> &BTreeMap<String, PathBuf> {
        &self.workflow_sources
    }

    /// The indexed OpenAPI source named `name`; `None` for a source that is
    /// not `type: openapi` or did not index.
    #[must_use]
    pub fn source(&self, name: &str) -> Option<&OperationIndex> {
        self.sources.get(name)
    }

    /// Resolves a qualified `$sourceDescriptions.<name>.<operationId>`.
    #[must_use]
    pub fn operation(&self, qualified: &str) -> Option<&Operation> {
        let (source, operation_id) = split_qualified(qualified)?;
        self.sources.get(source)?.get(operation_id)
    }

    /// The parameters a qualified `$sourceDescriptions.<name>.<operationId>`
    /// declares, after every `$ref` is followed.
    #[must_use]
    pub fn declared_parameters(&self, qualified: &str) -> Option<&[DeclaredParameter]> {
        let (source, operation_id) = split_qualified(qualified)?;
        self.parameters.get(source)?.get(operation_id)
    }

    /// Runs the catalogue gate over the document, checking every quote
    /// against the vendored files under `repo_root`; an empty list is the
    /// only passing result.
    #[must_use]
    pub fn check(&self, repo_root: &Path) -> Vec<Finding> {
        self.gate(Some(repo_root))
    }

    /// Runs the catalogue gate over the document. Without `repo_root` a
    /// citation is still read and inherited, but its quote is not looked up.
    #[must_use]
    pub fn gate(&self, repo_root: Option<&Path>) -> Vec<Finding> {
        let mut findings = Vec::new();
        match super::gates::schema::violations(&self.raw) {
            Ok(violations) => {
                for violation in violations {
                    findings.push(Finding::new("#", FindingKind::Schema(violation)));
                }
            }
            Err(error) => findings.push(Finding::new("#", FindingKind::Schema(error.to_string()))),
        }
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
        let mut quotes = repo_root.map(QuoteChecker::new);
        let dependencies = self.dependency_sources();
        for workflow in &self.description.workflows {
            self.check_workflow(workflow, quotes.as_mut(), &dependencies, &mut findings);
        }
        findings
    }

    /// Loads each `type: arazzo` source a `dependsOn` or an
    /// `x-veredictum-control` names once, keeping whether each of its
    /// workflows is a refusal workflow, by id, or the reason it did not load.
    fn dependency_sources(&self) -> BTreeMap<&str, Result<Dependency, String>> {
        let mut loaded = BTreeMap::new();
        for workflow in &self.description.workflows {
            // NOTE: no openEHR spec governs this — our own design; a malformed control is
            // `check_control`'s finding, so here it names no source to load.
            let controls = match control_of(workflow.extensions.as_ref()) {
                Ok(Some(Control::Workflows(named))) => named,
                _ => Vec::new(),
            };
            for id in workflow.depends_on.iter().chain(&controls) {
                let Some((source, _)) = split_qualified(id) else {
                    continue;
                };
                let Some((name, file)) = self.workflow_sources.get_key_value(source) else {
                    continue;
                };
                loaded.entry(name.as_str()).or_insert_with(|| {
                    Self::load(file)
                        .map(|other| {
                            other
                                .description
                                .workflows
                                .iter()
                                .map(|w| (w.workflow_id.clone(), is_refusal(w)))
                                .collect()
                        })
                        .map_err(|error| error.to_string())
                });
            }
        }
        loaded
    }

    fn check_workflow(
        &self,
        workflow: &Workflow,
        mut quotes: Option<&mut QuoteChecker<'_>>,
        dependencies: &BTreeMap<&str, Result<Dependency, String>>,
        findings: &mut Vec<Finding>,
    ) {
        let at = workflow.workflow_id.clone();
        if !is_workflow_id(&at) {
            findings.push(Finding::new(&at, FindingKind::WorkflowId(at.clone())));
        }
        let workflow_cite = cite_at(
            workflow.extensions.as_ref(),
            &at,
            quotes.as_deref_mut(),
            findings,
        );
        for id in &workflow.depends_on {
            if let Some(problem) = self.dependency_problem(id, dependencies) {
                findings.push(Finding::new(&at, FindingKind::Unsupported(problem)));
            }
        }
        for error in self.check_controls(workflow, dependencies) {
            findings.push(Finding::new(&at, FindingKind::Control(error)));
        }
        if !workflow.success_actions.is_empty()
            || !workflow.failure_actions.is_empty()
            || !workflow.parameters.is_empty()
        {
            findings.push(Finding::new(
                &at,
                FindingKind::Unsupported("actions and parameters on a workflow".to_owned()),
            ));
        }
        for step in &workflow.steps {
            let at = format!("{at}/{}", step.step_id);
            let step_cite = cite_at(
                step.extensions.as_ref(),
                &at,
                quotes.as_deref_mut(),
                findings,
            );
            self.check_step(step, &at, findings);
            for (index, criterion) in step.success_criteria.iter().enumerate() {
                let at = format!("{at}/successCriteria[{index}]");
                let own = cite_at(
                    criterion.extensions.as_ref(),
                    &at,
                    quotes.as_deref_mut(),
                    findings,
                );
                if own
                    .or(step_cite.clone())
                    .or(workflow_cite.clone())
                    .is_none()
                {
                    findings.push(Finding::new(&at, FindingKind::MissingCitation));
                }
                for kind in criterion_profile(criterion) {
                    findings.push(Finding::new(&at, kind));
                }
            }
        }
    }

    /// Why a `dependsOn` entry does not name a workflow: one of this
    /// document, or `$sourceDescriptions.<name>.<workflowId>` of a
    /// `type: arazzo` source (Arazzo 1.1.0 §Workflow Object `dependsOn`).
    fn dependency_problem(
        &self,
        id: &str,
        dependencies: &BTreeMap<&str, Result<Dependency, String>>,
    ) -> Option<String> {
        let Some((source, workflow_id)) = split_qualified(id) else {
            return self
                .workflow(id)
                .is_none()
                .then(|| format!("dependsOn `{id}`, which is not a workflow of this document"));
        };
        let Some(file) = self.workflow_sources.get(source) else {
            return Some(format!(
                "dependsOn `{id}`, whose source `{source}` is not a `type: arazzo` source"
            ));
        };
        match dependencies.get(source) {
            Some(Ok(workflows)) if workflows.contains_key(workflow_id) => None,
            Some(Ok(_)) => Some(format!(
                "dependsOn `{id}`, which is not a workflow of `{}`",
                file.display()
            )),
            Some(Err(error)) => Some(format!("dependsOn `{id}`: {error}")),
            None => Some(format!(
                "dependsOn `{id}`: `{}` was not loaded",
                file.display()
            )),
        }
    }

    /// Checks a workflow's `x-veredictum-control`: a refusal workflow names
    /// at least one control, any other names none, and each one it names is
    /// another workflow that exists and is not itself a refusal workflow.
    fn check_controls(
        &self,
        workflow: &Workflow,
        dependencies: &BTreeMap<&str, Result<Dependency, String>>,
    ) -> Vec<ControlError> {
        let control = match control_of(workflow.extensions.as_ref()) {
            Ok(control) => control,
            Err(error) => return vec![error],
        };
        match (is_refusal(workflow), control) {
            (true, None) => vec![ControlError::Missing],
            (false, Some(_)) => vec![ControlError::NotRefusal],
            (false, None) | (true, Some(Control::Pending(_))) => Vec::new(),
            (true, Some(Control::Workflows(controls))) => controls
                .into_iter()
                .filter_map(|control| self.check_control(workflow, control, dependencies).err())
                .collect(),
        }
    }

    /// Checks one control a refusal workflow names.
    fn check_control(
        &self,
        workflow: &Workflow,
        control: String,
        dependencies: &BTreeMap<&str, Result<Dependency, String>>,
    ) -> Result<(), ControlError> {
        if control == workflow.workflow_id {
            return Err(ControlError::Itself);
        }
        let refuses = match split_qualified(&control) {
            None => self.workflow(&control).map(is_refusal),
            Some((source, workflow_id)) => match dependencies.get(source) {
                Some(Ok(workflows)) => workflows.get(workflow_id).copied(),
                _ => None,
            },
        };
        match refuses {
            None => {
                let reason = if split_qualified(&control).is_some() {
                    self.dependency_problem(&control, dependencies)
                        .unwrap_or_else(|| "which is not a workflow of its source".to_owned())
                } else {
                    "which is not a workflow of this document".to_owned()
                };
                Err(ControlError::Unknown { control, reason })
            }
            Some(true) => Err(ControlError::ControlRefuses(control)),
            Some(false) => Ok(()),
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
        let declared = self.declared_parameters(qualified).unwrap_or_default();
        let mut path_parameters = Vec::new();
        for parameter in &step.parameters {
            let ReusableOr::Item(parameter) = parameter else {
                push(FindingKind::Unsupported("a reusable parameter".to_owned()));
                continue;
            };
            let location = match parameter.in_ {
                Some(ParameterLocation::Path) => {
                    path_parameters.push(parameter.name.clone());
                    Some(ParameterIn::Path)
                }
                Some(ParameterLocation::Query) => Some(ParameterIn::Query),
                Some(ParameterLocation::Header) => Some(ParameterIn::Header),
                other => {
                    push(FindingKind::Unsupported(format!(
                        "parameter `{}` in {other:?}",
                        parameter.name
                    )));
                    None
                }
            };
            if let Some(location) = location {
                let resolves = declared
                    .iter()
                    .any(|declared| declared.matches(location, &parameter.name));
                let allowed = location == ParameterIn::Header
                    && ProfileHeader::parse(&parameter.name).is_some();
                if !resolves && !allowed && location != ParameterIn::Path {
                    push(FindingKind::UndeclaredParameter {
                        name: parameter.name.clone(),
                        location: location.token(),
                    });
                }
            }
            for kind in value_findings(&parameter.name, &parameter.value, location) {
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
            body_findings(body, &mut push);
        }
        if let Err(error) = derivations_of(step.extensions.as_ref(), |name| {
            step.outputs.contains_key(name)
        }) {
            push(FindingKind::Derive(error));
        }
        if let Err(error) = principal_of(step.extensions.as_ref()) {
            push(FindingKind::Principal(error));
        }
        if let Err(error) = scope_of(step.extensions.as_ref()) {
            push(FindingKind::Scope(error));
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

/// Whether `id` matches `[A-Za-z0-9_\-]+`, the form Arazzo 1.1.0 §Workflow
/// Object recommends for `workflowId` and a case id is built from.
fn is_workflow_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// The profile findings of one criterion: its plan, its regex lint, and its
/// context expression.
fn criterion_profile(criterion: &Criterion) -> Vec<FindingKind> {
    let mut out = Vec::new();
    if matches!(
        criterion.type_,
        Some(CriterionType::Simple(CriterionKind::Regex))
    ) && let Err(breach) = lint_regex(&criterion.condition)
    {
        out.push(FindingKind::Regex(breach));
        return out;
    }
    match CriterionPlan::of(criterion) {
        Err(kind) => out.push(kind),
        Ok(plan) => plan_profile(&plan, criterion, &mut out),
    }
    out
}

/// The profile findings of one planned criterion; a checked criterion is
/// linted through the condition its judge runs behind.
fn plan_profile(plan: &CriterionPlan, criterion: &Criterion, out: &mut Vec<FindingKind>) {
    match plan {
        CriterionPlan::Regex(_) => {
            if let Err(breach) = lint_regex(&criterion.condition) {
                out.push(FindingKind::Regex(breach));
            }
        }
        CriterionPlan::Simple(_) => {
            if let Some(context) = &criterion.context
                && let Err(error) = Expr::parse(context)
            {
                out.push(FindingKind::Profile(error));
            }
        }
        // NOTE: Arazzo 1.1.0 §JSONPath Conditions; `JsonPathCriterion::parse` already refused a
        // query that is not RFC 9535 and a context that does not parse.
        CriterionPlan::JsonPath(_) => {}
        CriterionPlan::Checked(inner, _) => plan_profile(inner, criterion, out),
    }
}

/// Reads and, when a checker is supplied, checks the citation at one
/// location; returns it when it is well-formed.
fn cite_at(
    extensions: Option<&BTreeMap<String, Value>>,
    at: &str,
    quotes: Option<&mut QuoteChecker<'_>>,
    findings: &mut Vec<Finding>,
) -> Option<Citation> {
    match citation_of(extensions) {
        Ok(Some(citation)) => {
            if let Err(error) = check_level(&citation) {
                findings.push(Finding::new(at, FindingKind::Level(error)));
            }
            if let Some(quotes) = quotes
                && let Err(error) = quotes.check(&citation)
            {
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

/// Checks a request body: a payload that is a runtime expression parses, and
/// every replacement targets a JSON Pointer with a value that parses.
fn body_findings(body: &RequestBody, push: &mut impl FnMut(FindingKind)) {
    if let Some(payload @ Value::String(_)) = &body.payload
        && let Err(kind) = value_expression(&ValueOrSelector::Literal(payload.clone()))
    {
        push(kind);
    }
    for replacement in &body.replacements {
        if !replacement.target.starts_with('/') || replacement.target_selector_type.is_some() {
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

/// The findings on a parameter's value: an expression outside the profile,
/// and a list anywhere but in a query parameter.
fn value_findings(
    name: &str,
    value: &ValueOrSelector,
    location: Option<ParameterIn>,
) -> Vec<FindingKind> {
    let mut out: Vec<FindingKind> = value_expression(value).err().into_iter().collect();
    if matches!(value, ValueOrSelector::Literal(Value::Array(_)))
        && location != Some(ParameterIn::Query)
    {
        out.push(FindingKind::Unsupported(format!(
            "the list value of `{name}` outside a query parameter"
        )));
    }
    out
}

/// Checks a parameter or replacement value: a literal, a runtime expression
/// that parses, or a string whose embedded `{$…}` expressions all parse.
fn value_expression(value: &ValueOrSelector) -> Result<(), FindingKind> {
    match value {
        ValueOrSelector::Literal(Value::String(text)) if text.starts_with('$') => {
            Expr::parse(text).map(drop).map_err(FindingKind::Profile)
        }
        ValueOrSelector::Literal(Value::String(text)) => {
            embedded_expressions(text).map_err(FindingKind::Profile)
        }
        ValueOrSelector::Literal(Value::Array(items)) => items
            .iter()
            .try_for_each(|item| value_expression(&ValueOrSelector::Literal(item.clone()))),
        ValueOrSelector::Literal(_) => Ok(()),
        ValueOrSelector::Selector(_) => {
            Err(FindingKind::Unsupported("a selector value".to_owned()))
        }
    }
}

/// Parses every `{$…}` expression embedded in `text` (Arazzo 1.1.0 §Runtime
/// Expressions: "Runtime expressions … can be embedded into string values by
/// surrounding the expression with {} curly braces").
fn embedded_expressions(text: &str) -> Result<(), ParseError> {
    let mut rest = text;
    while let Some(start) = rest.find("{$") {
        let tail = rest.get(start.saturating_add(1)..).unwrap_or_default();
        let Some(end) = tail.find('}') else {
            return Err(ParseError::Unterminated(text.to_owned()));
        };
        Expr::parse(tail.get(..end).unwrap_or_default())?;
        rest = tail.get(end..).unwrap_or_default();
    }
    Ok(())
}

/// The `{name}` placeholders of a path template.
///
/// An RFC 6570 §3.2.8 form-style query expansion (`{?name*}`) or §3.2.9
/// continuation (`{&name}`) names a query parameter, not a path segment, so
/// it is no placeholder.
fn placeholders(template: &str) -> Vec<String> {
    template
        .split('{')
        .skip(1)
        .filter_map(|part| part.split_once('}').map(|(name, _)| name.to_owned()))
        .filter(|name| !name.starts_with(['?', '&']))
        .collect()
}

/// A criterion as the evaluator judges it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CriterionPlan {
    /// A `simple` condition.
    Simple(SimpleCondition),
    /// A `regex` condition over a context.
    Regex(RegexCriterion),
    /// An RFC 9535 `jsonpath` condition over a context.
    JsonPath(JsonPathCriterion),
    /// A condition that must hold for its `x-veredictum-check` judge to run;
    /// the criterion holds when both do.
    Checked(Box<CriterionPlan>, Check),
}

impl CriterionPlan {
    /// Plans one criterion under the profile.
    ///
    /// # Errors
    /// The [`FindingKind`] that refuses it.
    pub fn of(criterion: &Criterion) -> Result<Self, FindingKind> {
        let condition = Self::condition(criterion).map_err(FindingKind::Profile)?;
        match criterion.extensions.as_ref().and_then(|e| e.get(CHECK_KEY)) {
            None => Ok(condition),
            Some(value) => Check::parse(value)
                .map(|check| Self::Checked(Box::new(condition), check))
                .map_err(FindingKind::Profile),
        }
    }

    fn condition(criterion: &Criterion) -> Result<Self, ParseError> {
        let context = |kind: &str| {
            criterion
                .context
                .as_deref()
                .ok_or_else(|| ParseError::MissingContext(kind.to_owned()))
        };
        let jsonpath = || {
            JsonPathCriterion::parse(context("jsonpath")?, &criterion.condition).map(Self::JsonPath)
        };
        match &criterion.type_ {
            None | Some(CriterionType::Simple(CriterionKind::Simple)) => {
                SimpleCondition::parse(&criterion.condition).map(Self::Simple)
            }
            Some(CriterionType::Simple(CriterionKind::Regex)) => {
                RegexCriterion::parse(context("regex")?, &criterion.condition).map(Self::Regex)
            }
            Some(CriterionType::Simple(CriterionKind::Jsonpath)) => jsonpath(),
            Some(CriterionType::Expression(expression))
                if expression.type_ == ExpressionKind::Jsonpath
                    && expression.version == "rfc9535" =>
            {
                jsonpath()
            }
            // TODO(#637): judge `xpath` criteria once a vetted XPath 3.1 engine is chosen.
            Some(
                CriterionType::Simple(CriterionKind::Xpath)
                | CriterionType::Expression(ExpressionType {
                    type_: ExpressionKind::Xpath,
                    ..
                }),
            ) => Err(ParseError::XPath(criterion.condition.clone())),
            Some(CriterionType::Expression(expression)) => Err(ParseError::ExpressionType {
                kind: format!("{:?}", expression.type_).to_lowercase(),
                version: expression.version.clone(),
            }),
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
    /// The official Arazzo 1.1 JSON Schema refuses the document.
    #[error("Arazzo 1.1 schema: {0}")]
    Schema(String),
    /// The Arazzo model's own validation refuses it.
    #[error("Arazzo 1.1.0: {0}")]
    Model(String),
    /// A `workflowId` outside `[A-Za-z0-9_\-]+`, so no case id can be built
    /// from it.
    #[error("workflowId `{0}` does not match [A-Za-z0-9_\\-]+")]
    WorkflowId(String),
    /// A key Arazzo 1.1.0 does not define, which the model would drop.
    #[error("`{0}` is not a field Arazzo 1.1.0 defines here")]
    UnknownKey(String),
    /// An extension other than the three the catalogue allows, or one in the
    /// wrong place.
    #[error("extension `{0}` is not allowed here")]
    ExtensionNotAllowed(String),
    /// A criterion that resolves to no citation through inheritance.
    #[error("the criterion resolves to no `{CITE_KEY}`")]
    MissingCitation,
    /// A citation that does not hold.
    #[error(transparent)]
    Citation(CiteError),
    /// A citation whose `level` disagrees with its quote's keywords.
    #[error(transparent)]
    Level(CiteError),
    /// An expression or condition outside the profile.
    #[error(transparent)]
    Profile(ParseError),
    /// A `regex` condition outside the profile.
    #[error(transparent)]
    Regex(RegexBreach),
    /// A source description that is not an indexable local `openapi` source.
    #[error("source description: {0}")]
    Source(String),
    /// An `operationId` that does not resolve in its named source.
    #[error("operation `{0}` does not resolve in its source")]
    UnresolvedOperation(String),
    /// A path parameter that does not match the operation's path template.
    #[error("path parameter `{0}` does not match the operation's path template")]
    UnresolvedParameter(String),
    /// A parameter the operation does not declare, and that is not a header
    /// the profile allows.
    #[error(
        "parameter `{name}` in {location} is not declared by the operation and is not a header the profile allows"
    )]
    UndeclaredParameter {
        /// The parameter name.
        name: String,
        /// The OAS `in` token.
        location: &'static str,
    },
    /// A construct the evaluator does not drive.
    #[error("not driven by the evaluator: {0}")]
    Unsupported(String),
    /// An `x-veredictum-derive` that does not hold.
    #[error(transparent)]
    Derive(DeriveError),
    /// An `x-veredictum-principal` that does not hold.
    #[error(transparent)]
    Principal(PrincipalError),
    /// An `x-veredictum-scope` that does not hold.
    #[error(transparent)]
    Scope(ScopeError),
    /// A refusal workflow's `x-veredictum-control` that is absent or does
    /// not name a control, or a control on any other workflow.
    #[error(transparent)]
    Control(ControlError),
}

impl FindingKind {
    /// The `validate` gate this finding belongs to.
    #[must_use]
    pub fn gate(&self) -> Gate {
        match self {
            Self::Schema(_) | Self::Model(_) | Self::WorkflowId(_) => Gate::Schema,
            Self::UnknownKey(_) | Self::ExtensionNotAllowed(_) => Gate::Keys,
            Self::MissingCitation | Self::Citation(_) => Gate::Citation,
            Self::Level(_) => Gate::Level,
            Self::Profile(_)
            | Self::Regex(_)
            | Self::Unsupported(_)
            | Self::Derive(_)
            | Self::Principal(_)
            | Self::Scope(_) => Gate::Profile,
            Self::Source(_)
            | Self::UnresolvedOperation(_)
            | Self::UnresolvedParameter(_)
            | Self::UndeclaredParameter { .. } => Gate::Source,
            Self::Control(_) => Gate::Identity,
        }
    }
}

/// The finding a source description that did not index becomes.
#[must_use]
pub fn source_finding(problem: &DocumentError) -> Finding {
    Finding::new(
        "#/sourceDescriptions",
        FindingKind::Source(problem.to_string()),
    )
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

/// Reports every `x-` key that is not one of the catalogue's extensions
/// at a place it is defined for. Request payloads are data and are skipped.
fn extension_keys(value: &Value, at: &mut Vec<String>, findings: &mut Vec<Finding>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if key.starts_with("x-") {
                    let allowed = match key.as_str() {
                        CITE_KEY => is_workflow(at) || is_step(at) || is_criterion(at),
                        CONTROL_KEY => is_workflow(at),
                        CHECK_KEY => is_criterion(at),
                        DERIVE_KEY | PRINCIPAL_KEY | SCOPE_KEY => is_step(at),
                        SECTION_KEY => matches!(at.as_slice(), [i] if i == "info"),
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

    const OAS: &str = "openapi: 3.0.3\npaths:\n  /ehr/{ehr_id}:\n    get:\n      operationId: ehr_get_by_id\n      parameters:\n        - $ref: '#/components/parameters/ehr_id'\n        - { name: version_at_time, in: query }\ncomponents:\n  parameters:\n    ehr_id: { name: ehr_id, in: path, required: true }\n";
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
        let file = dir.child("catalogue/cases/ehr.arazzo.yaml");
        file.write_str(&doc).unwrap();
        let loaded = ArazzoDocument::load(file.path()).unwrap();
        (dir, loaded)
    }

    const CITE: &str =
        "source: specs/openehr/ITS-REST/specifications/responses/404.yaml, section: description";

    fn cited() -> String {
        format!(
            "    x-veredictum-cite: {{ {CITE}, quote: '`404 Not Found` is returned when an EHR with `ehr_id` does not exist.' }}\n"
        )
    }

    fn workflow(workflow_ext: &str, criterion_ext: &str, condition: &str) -> String {
        format!(
            "  - workflowId: w\n{workflow_ext}    steps:\n      - stepId: get\n        operationId: $sourceDescriptions.ehr.ehr_get_by_id\n        parameters: [{{ name: ehr_id, in: path, value: $inputs.ehr_id }}]\n        successCriteria:\n          - condition: {condition}\n{criterion_ext}"
        )
    }

    fn kinds(dir: &assert_fs::TempDir, doc: &ArazzoDocument) -> Vec<FindingKind> {
        doc.check(dir.path()).into_iter().map(|f| f.kind).collect()
    }

    #[test]
    fn a_criterion_inherits_its_workflow_citation() {
        let (dir, doc) = fixture(&workflow(&cited(), "", "$statusCode == 404"));
        assert_eq!(doc.check(dir.path()), Vec::new());
    }

    #[test]
    fn a_scope_set_is_gated_and_never_sits_beside_a_principal() {
        let with = |extensions: &str| {
            workflow(&cited(), "", "$statusCode == 404").replace(
                "        operationId: $sourceDescriptions.ehr.ehr_get_by_id\n",
                &format!(
                    "        operationId: $sourceDescriptions.ehr.ehr_get_by_id\n{extensions}"
                ),
            )
        };
        for clean in [
            "        x-veredictum-scope: 'system/template-*.r openid'\n",
            "        x-veredictum-scope: $inputs.scope\n",
            "        x-veredictum-scope: ''\n",
        ] {
            let (dir, doc) = fixture(&with(clean));
            assert_eq!(doc.check(dir.path()), Vec::new(), "{clean}");
        }
        let refused = [
            (
                "        x-veredictum-scope: 'a\"b'\n",
                ScopeError::Token("a\"b".to_owned()),
            ),
            ("        x-veredictum-scope: [a]\n", ScopeError::NotAString),
            (
                "        x-veredictum-scope: a\n        x-veredictum-principal: administrative\n",
                ScopeError::WithPrincipal,
            ),
        ];
        for (extensions, error) in refused {
            let (dir, doc) = fixture(&with(extensions));
            assert!(
                kinds(&dir, &doc).contains(&FindingKind::Scope(error.clone())),
                "{extensions}: {:?}",
                kinds(&dir, &doc)
            );
        }
    }

    #[test]
    fn a_principal_outside_the_vocabulary_and_a_list_path_value_are_refused() {
        let named = |principal: &str| {
            workflow(&cited(), "", "$statusCode == 404").replace(
                "        operationId: $sourceDescriptions.ehr.ehr_get_by_id\n",
                &format!(
                    "        operationId: $sourceDescriptions.ehr.ehr_get_by_id\n        x-veredictum-principal: {principal}\n"
                ),
            )
        };
        let (dir, doc) = fixture(&named("administrative"));
        assert_eq!(doc.check(dir.path()), Vec::new());
        let (dir, doc) = fixture(&named("root"));
        assert!(
            kinds(&dir, &doc).contains(&FindingKind::Principal(PrincipalError::Unknown(
                "root".to_owned()
            ))),
            "{:?}",
            kinds(&dir, &doc)
        );
        let listed = workflow(&cited(), "", "$statusCode == 404")
            .replace("value: $inputs.ehr_id", "value: [$inputs.ehr_id]");
        let (dir, doc) = fixture(&listed);
        assert!(
            kinds(&dir, &doc).iter().any(
                |k| matches!(k, FindingKind::Unsupported(text) if text.contains("list value"))
            ),
            "{:?}",
            kinds(&dir, &doc)
        );
    }

    #[test]
    fn a_query_expansion_in_a_path_template_is_no_placeholder() {
        assert_eq!(
            placeholders("/admin/ehr/all{?ehr_id*}"),
            Vec::<String>::new()
        );
        assert_eq!(placeholders("/ehr/{ehr_id}{&x}"), vec!["ehr_id".to_owned()]);
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
        assert!(
            doc.gate(None).is_empty(),
            "offline, the quote is not looked up"
        );
    }

    #[test]
    fn a_seeded_report_that_is_not_an_issue_number_is_refused() {
        let ext = format!(
            "            x-veredictum-cite: {{ {CITE}, quote: '`404 Not Found` is returned when an EHR with `ehr_id` does not exist.', report: 0 }}\n"
        );
        let (dir, doc) = fixture(&workflow("", &ext, "$statusCode == 404"));
        let kinds = kinds(&dir, &doc);
        assert!(
            kinds
                .iter()
                .any(|k| matches!(k, FindingKind::Citation(CiteError::Malformed(_)))),
            "{kinds:?}"
        );
    }

    #[test]
    fn unknown_keys_foreign_extensions_and_profile_breaches_are_refused() {
        let ext = format!(
            "{}    x-veredictum-outcome: failed\n    succesCriteria: []\n",
            cited()
        );
        let (dir, doc) = fixture(&workflow(&ext, "", "\"$response.body#/a == 'x'\""));
        let kinds = kinds(&dir, &doc);
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
        assert!(
            kinds.iter().any(|k| matches!(k, FindingKind::Schema(_))),
            "the official schema refuses the unknown key too: {kinds:?}"
        );
    }

    /// The step of `workflow` with `outputs` and an `x-veredictum-derive`
    /// whose entries are `derive`, written as YAML flow mappings.
    fn with_derive(derive: &str) -> String {
        workflow(&cited(), "", "$statusCode == 404").replace(
            "        successCriteria:\n",
            &format!(
                "        outputs: {{ version_uid: $response.body#/uid }}\n        x-veredictum-derive: {{ {derive} }}\n        successCriteria:\n"
            ),
        )
    }

    #[test]
    fn a_derive_over_a_declared_output_passes_the_gate() {
        let (dir, doc) = fixture(&with_derive(
            "vo_uid: { function: object_id, of: version_uid }, tree: { function: version_tree_id, of: version_uid }",
        ));
        assert_eq!(kinds(&dir, &doc), Vec::<FindingKind>::new());
    }

    #[test]
    fn the_gate_refuses_an_unknown_function_an_undeclared_of_and_a_colliding_name() {
        for (derive, refused) in [
            (
                "vo_uid: { function: is_branch, of: version_uid }",
                DeriveError::UnknownFunction("is_branch".to_owned()),
            ),
            (
                "vo_uid: { function: object_id, of: uid }",
                DeriveError::UndeclaredOutput {
                    name: "vo_uid".to_owned(),
                    of: "uid".to_owned(),
                },
            ),
            (
                "version_uid: { function: object_id, of: version_uid }",
                DeriveError::Collides("version_uid".to_owned()),
            ),
        ] {
            let (dir, doc) = fixture(&with_derive(derive));
            let kinds = kinds(&dir, &doc);
            assert!(
                kinds.contains(&FindingKind::Derive(refused)),
                "{derive}: {kinds:?}"
            );
        }
    }

    #[test]
    fn a_derive_off_a_step_is_not_allowed() {
        let ext = format!(
            "{}    x-veredictum-derive: {{ vo_uid: {{ function: object_id, of: version_uid }} }}\n",
            cited()
        );
        let (dir, doc) = fixture(&workflow(&ext, "", "$statusCode == 404"));
        assert!(
            kinds(&dir, &doc).contains(&FindingKind::ExtensionNotAllowed(DERIVE_KEY.to_owned()))
        );
    }

    #[test]
    fn a_seeded_lookaround_regex_is_refused() {
        let text = workflow(&cited(), "", "$statusCode == 404").replace(
            "          - condition: $statusCode == 404\n",
            "          - context: $response.body#/ehr_id/value\n            type: regex\n            condition: '^(?=a){$inputs.ehr_id}$'\n",
        );
        let (dir, doc) = fixture(&text);
        assert_eq!(
            kinds(&dir, &doc),
            vec![FindingKind::Regex(RegexBreach::Lookaround(
                "^(?=a){$inputs.ehr_id}$".to_owned()
            ))]
        );
    }

    fn with_criterion(criterion: &str) -> String {
        workflow(&cited(), "", "$statusCode == 404")
            .replace("          - condition: $statusCode == 404\n", criterion)
    }

    #[test]
    fn a_seeded_jsonpath_outside_rfc_9535_or_without_context_is_refused() {
        let good = with_criterion(
            "          - context: $response.body\n            type: jsonpath\n            condition: \"$.ehr_id[?@ == '{$inputs.ehr_id}']\"\n",
        );
        let (dir, doc) = fixture(&good);
        assert_eq!(kinds(&dir, &doc), Vec::new());
        let bad = with_criterion(
            "          - context: $response.body\n            type: jsonpath\n            condition: '$.a[?(@.b'\n",
        );
        let (dir, doc) = fixture(&bad);
        assert!(matches!(
            kinds(&dir, &doc).as_slice(),
            [FindingKind::Profile(ParseError::JsonPath { .. })]
        ));
        let bare = with_criterion("          - type: jsonpath\n            condition: $.a\n");
        let (dir, doc) = fixture(&bare);
        assert!(
            kinds(&dir, &doc)
                .iter()
                .any(|k| matches!(k, FindingKind::Profile(ParseError::MissingContext(_)))),
            "{:?}",
            kinds(&dir, &doc)
        );
    }

    #[test]
    fn a_seeded_xpath_criterion_is_refused() {
        let text = with_criterion(
            "          - context: $response.body\n            type: xpath\n            condition: /ehr\n",
        );
        let (dir, doc) = fixture(&text);
        assert_eq!(
            kinds(&dir, &doc),
            vec![FindingKind::Profile(ParseError::XPath("/ehr".to_owned()))]
        );
    }

    #[test]
    fn a_seeded_check_is_linted_through_its_condition_and_its_judge() {
        let checked = |extra: &str| {
            with_criterion(&format!(
                "          - condition: $statusCode == 200\n            x-veredictum-check: {{ judge: equivalent, context: $response.body, to: $inputs.ehr_id{extra} }}\n"
            ))
        };
        let (dir, doc) = fixture(&checked(""));
        assert_eq!(kinds(&dir, &doc), Vec::new());
        let (dir, doc) = fixture(&checked(", extra: 1"));
        assert!(
            kinds(&dir, &doc)
                .iter()
                .any(|k| matches!(k, FindingKind::Profile(ParseError::Check(_)))),
            "{:?}",
            kinds(&dir, &doc)
        );
    }

    #[test]
    fn a_seeded_schema_check_is_accepted_only_against_the_oas() {
        let checked = |against: &str| {
            with_criterion(&format!(
                "          - condition: $statusCode == 200\n            x-veredictum-check: {{ judge: schema, against: {against} }}\n"
            ))
        };
        let (dir, doc) = fixture(&checked("oas"));
        assert_eq!(kinds(&dir, &doc), Vec::new());
        let (dir, doc) = fixture(&checked("xsd"));
        assert!(
            kinds(&dir, &doc)
                .iter()
                .any(|k| matches!(k, FindingKind::Profile(ParseError::Check(_)))),
            "{:?}",
            kinds(&dir, &doc)
        );
    }

    #[test]
    fn seeded_unparsable_expressions_are_refused() {
        let text = workflow(&cited(), "", "$statusCode == 404")
            .replace("value: $inputs.ehr_id", "value: '$input.ehr_id'");
        let (dir, doc) = fixture(&text);
        assert_eq!(
            kinds(&dir, &doc),
            vec![FindingKind::Profile(ParseError::Expression(
                "$input.ehr_id".to_owned()
            ))]
        );
        let text = workflow(&cited(), "", "$statusCode == 404")
            .replace("value: $inputs.ehr_id", "value: 'ehr-{$inputs.ehr_id'");
        let (dir, doc) = fixture(&text);
        assert!(matches!(
            kinds(&dir, &doc).as_slice(),
            [FindingKind::Profile(ParseError::Unterminated(_))]
        ));
    }

    #[test]
    fn an_unresolved_operation_is_refused() {
        let text =
            workflow(&cited(), "", "$statusCode == 404").replace("ehr_get_by_id", "ehr_get_by_idx");
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

    #[test]
    fn declared_and_allowed_parameters_resolve_and_others_are_refused() {
        let params = "[{ name: ehr_id, in: path, value: $inputs.ehr_id }, { name: version_at_time, in: query, value: x }, { name: accept, in: header, value: application/json }, { name: If-Match, in: header, value: x }]";
        let text = workflow(&cited(), "", "$statusCode == 404").replace(
            "[{ name: ehr_id, in: path, value: $inputs.ehr_id }]",
            params,
        );
        let (dir, doc) = fixture(&text);
        assert_eq!(doc.check(dir.path()), Vec::new());
        for (seeded, name, location) in [
            (
                "{ name: versionAtTime, in: query, value: x }",
                "versionAtTime",
                "query",
            ),
            (
                "{ name: If-None-Match, in: header, value: x }",
                "If-None-Match",
                "header",
            ),
            (
                "{ name: version_at_time, in: header, value: x }",
                "version_at_time",
                "header",
            ),
        ] {
            let text = workflow(&cited(), "", "$statusCode == 404").replace(
                "[{ name: ehr_id, in: path, value: $inputs.ehr_id }]",
                &format!("[{{ name: ehr_id, in: path, value: $inputs.ehr_id }}, {seeded}]"),
            );
            let (dir, doc) = fixture(&text);
            assert_eq!(
                kinds(&dir, &doc),
                vec![FindingKind::UndeclaredParameter {
                    name: name.to_owned(),
                    location
                }],
                "{seeded}"
            );
        }
    }

    #[test]
    fn a_payload_that_is_a_runtime_expression_must_parse() {
        let with_payload = |payload: &str| {
            workflow(&cited(), "", "$statusCode == 404").replace(
                "        successCriteria:",
                &format!(
                    "        requestBody: {{ payload: '{payload}' }}\n        successCriteria:"
                ),
            )
        };
        for payload in [
            "$inputs.composition",
            "<template/>",
            "<id>t-{$inputs.composition}</id>",
        ] {
            let (dir, doc) = fixture(&with_payload(payload));
            assert_eq!(doc.check(dir.path()), Vec::new(), "{payload}");
        }
        for payload in ["$input.composition", "<id>t-{$input.composition}</id>"] {
            let (dir, doc) = fixture(&with_payload(payload));
            assert!(
                matches!(kinds(&dir, &doc).as_slice(), [FindingKind::Profile(_)]),
                "{payload}: {:?}",
                kinds(&dir, &doc)
            );
        }
    }

    #[test]
    fn a_seeded_source_without_type_openapi_is_reported_not_indexed() {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("specs/rest-oas/ehr.yaml").write_str(OAS).unwrap();
        let file = dir.child("catalogue/cases/ehr.arazzo.yaml");
        file.write_str(&format!(
            "arazzo: 1.1.0\ninfo: {{ title: t, version: '1' }}\nsourceDescriptions:\n  - {{ name: ehr, url: ../../specs/rest-oas/ehr.yaml }}\nworkflows:\n{}",
            workflow(&cited(), "", "$statusCode == 404")
        ))
        .unwrap();
        assert!(matches!(
            ArazzoDocument::load(file.path()),
            Err(DocumentError::SourceType(_))
        ));
        let raw = crate::load::yaml_file_to_value(file.path()).unwrap();
        let (doc, problems) = ArazzoDocument::from_value(file.path(), raw).unwrap();
        assert!(matches!(problems.as_slice(), [DocumentError::SourceType(name)] if name == "ehr"));
        assert_eq!(source_finding(&problems[0]).kind.gate(), Gate::Source);
        assert!(doc.check(dir.path()).iter().any(|f| f.kind
            == FindingKind::UnresolvedOperation(
                "$sourceDescriptions.ehr.ehr_get_by_id".to_owned()
            )));
    }

    #[test]
    fn a_seeded_workflow_id_outside_the_case_id_alphabet_is_refused() {
        let text = workflow(&cited(), "", "$statusCode == 404")
            .replace("workflowId: w", "workflowId: w.1");
        let (dir, doc) = fixture(&text);
        assert!(
            kinds(&dir, &doc).contains(&FindingKind::WorkflowId("w.1".to_owned())),
            "{:?}",
            kinds(&dir, &doc)
        );
    }

    fn refusal(id: &str, control: Option<&str>) -> String {
        let extensions = match control {
            Some(control) => format!("{}    x-veredictum-control: {control}\n", cited()),
            None => cited(),
        };
        workflow(&extensions, "", "$statusCode == 400")
            .replace("workflowId: w", &format!("workflowId: {id}"))
    }

    fn twin() -> String {
        workflow(&cited(), "", "$statusCode == 404").replace("workflowId: w", "workflowId: twin")
    }

    #[test]
    fn a_refusal_names_a_control_that_exists_and_accepts() {
        for control in ["twin", "[twin]", "{ pending: 835 }"] {
            let (dir, doc) = fixture(&(refusal("w", Some(control)) + &twin()));
            assert_eq!(doc.check(dir.path()), Vec::new(), "{control}");
        }
        let refused = [
            (refusal("w", None) + &twin(), ControlError::Missing),
            (
                refusal("w", Some("nope")) + &twin(),
                ControlError::Unknown {
                    control: "nope".to_owned(),
                    reason: "which is not a workflow of this document".to_owned(),
                },
            ),
            (refusal("w", Some("w")) + &twin(), ControlError::Itself),
            (refusal("w", Some("[]")) + &twin(), ControlError::Shape),
            (refusal("w", Some("{ pending: 0 }")) + &twin(), ControlError::Pending),
            (refusal("w", Some("{ pending: x }")) + &twin(), ControlError::Pending),
            (refusal("w", Some("{ pending: 1, extra: 2 }")) + &twin(), ControlError::Shape),
            (
                twin().replace(
                    "workflowId: twin\n",
                    "workflowId: twin\n    x-veredictum-control: { pending: 7 }\n",
                ) + &refusal("w", Some("twin")),
                ControlError::NotRefusal,
            ),
            (refusal("w", Some("[twin, 3]")) + &twin(), ControlError::Shape),
            (
                refusal("w", Some("[twin, nope]")) + &twin(),
                ControlError::Unknown {
                    control: "nope".to_owned(),
                    reason: "which is not a workflow of this document".to_owned(),
                },
            ),
            (
                refusal("w", Some("other")) + &refusal("other", Some("twin")) + &twin(),
                ControlError::ControlRefuses("other".to_owned()),
            ),
            (
                twin().replace(
                    "workflowId: twin\n",
                    "workflowId: twin\n    x-veredictum-control: w\n",
                ) + &refusal("w", Some("twin")),
                ControlError::NotRefusal,
            ),
            (
                refusal("w", Some("$sourceDescriptions.ehr.twin")) + &twin(),
                ControlError::Unknown {
                    control: "$sourceDescriptions.ehr.twin".to_owned(),
                    reason: "dependsOn `$sourceDescriptions.ehr.twin`, whose source `ehr` is not a `type: arazzo` source".to_owned(),
                },
            ),
        ];
        for (workflows, error) in refused {
            let (dir, doc) = fixture(&workflows);
            assert!(
                kinds(&dir, &doc).contains(&FindingKind::Control(error.clone())),
                "{error}: {:?}",
                kinds(&dir, &doc)
            );
        }
    }

    #[test]
    fn a_control_of_another_document_resolves_through_its_arazzo_source() {
        let (dir, _) = fixture(&twin());
        let workflows = refusal("w", Some("$sourceDescriptions.other.twin"));
        let doc = format!(
            "arazzo: 1.1.0\ninfo: {{ title: t, version: '1' }}\nsourceDescriptions:\n  - {{ name: ehr, url: ../../specs/rest-oas/ehr.yaml, type: openapi }}\n  - {{ name: other, url: ehr.arazzo.yaml, type: arazzo }}\nworkflows:\n{workflows}"
        );
        let file = dir.child("catalogue/cases/refused.arazzo.yaml");
        file.write_str(&doc).unwrap();
        let loaded = ArazzoDocument::load(file.path()).unwrap();
        assert_eq!(loaded.check(dir.path()), Vec::new());
        assert_eq!(
            crate::arazzo::control::controls(&loaded, "refused"),
            BTreeMap::from([(
                "refused/w".to_owned(),
                Control::Workflows(vec!["ehr/twin".to_owned()])
            )])
        );
    }
}
