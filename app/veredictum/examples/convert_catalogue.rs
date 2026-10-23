// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Converts one component of the case-core catalogue into a DRAFT Arazzo
//! 1.1.0 document, its rows files and its entries in the old-to-new id map.
//!
//! The tool is a one-off for the conversion (#603) and is deleted once the
//! last component passes parity. It reads every case whose id starts with
//! `--prefix` together with the operation bindings its steps realize
//! through, and writes `<catalogue>/cases/<capability>/<document>.arazzo.yaml`
//! plus one `<workflowId>.rows.yaml` per data-driven workflow beside it. It
//! refuses to overwrite a document that already exists, so hand completion is
//! never lost.
//!
//! The mapping:
//!
//! - A case is one workflow named after its id without the prefix. A
//!   parameter matrix whose rows call different operations, or send a body in
//!   some rows and none in others, splits into one workflow per row shape,
//!   because an Arazzo workflow cannot omit a parameter or a body per row.
//!   A fixture set splits into one workflow per expected outcome.
//! - A flow step is a step whose `operationId` is the
//!   `$sourceDescriptions.<api>.<operationId>` the binding's method and path
//!   resolve to in the released `-validation` OAS. A `with_<p>` binding
//!   variant is chosen exactly when the old driver chose it: the step binds
//!   `<p>` non-null and names no variant.
//! - An outcome kind becomes `$statusCode == <status>` (a `regex` over the
//!   status with alternative statuses); each header matcher of that outcome
//!   becomes a criterion over `$response.header.<name>`.
//! - A capture becomes a step output; `${capture}` becomes
//!   `$steps.<stepId>.outputs.<name>`; `${row.<column>}` becomes
//!   `$inputs.<column>`; a `provided` identifier column becomes a
//!   `format: uuid` input the evaluator generates per row.
//! - `requires.ehr` becomes a `setup-ehr` workflow named in `dependsOn`, and
//!   `requires.templates` one upload workflow per template.
//! - `field` assertions become `regex` or presence criteria; `equivalent`
//!   becomes the `equivalent` judge of `x-veredictum-check`.
//! - Every citation is a placeholder whose `source` is `UNCITED`, which the
//!   citation gate refuses, so no draft can pass `validate` until each one is
//!   completed by hand with a verbatim quote. `sm_operation` and every SM
//!   reference are dropped.
//! - What has no Arazzo counterpart (an `instance_of`, a postcondition, a
//!   recipe, an IXIT fact, an option tag, a header matcher whose presence is
//!   optional) is written as a `# UNCONVERTED:` comment above its workflow and
//!   listed on standard output.
//!
//! ```text
//! cargo run --example convert_catalogue -- --prefix I_EHR_SERVICE. \
//!     --capability ehr --document ehr_service
//! ```
//!
//! No openEHR spec governs this tool — our own design.

#![expect(
    clippy::disallowed_types,
    reason = "a one-off converter between two YAML artifact formats, which it reads and writes as JSON values"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use clap::Parser;
use serde_json::{Map, Value, json};
use veredictum::artifacts::{ArtifactSet, load_root};
use veredictum::model::assertion::{Assertion, EquivalentTarget, IgnoreSpec};
use veredictum::model::binding::{
    HeaderMatcher, OperationBinding, QueryValue, RequestBody, WireExpectation, WireFrom,
};
use veredictum::model::case::{
    CaseCore, EhrRequirement, ExpectSpec, FixtureEntry, FlowStep, MatrixCell,
};
use veredictum::model::value::TemplatedValue;
use veredictum::refgrammar::{CaptureField, Segment, Template, ValueRef};
use veredictum::vocab::{FormatName, Iteration, OutcomeKind, ServerState};

/// The command line.
#[derive(Debug, Parser)]
struct Args {
    /// The old artifact root.
    #[arg(long, default_value = "artifacts")]
    root: PathBuf,
    /// The released OAS bundles.
    #[arg(long, default_value = "specs/rest-oas")]
    oas: PathBuf,
    /// The case-id prefix selecting the component, e.g. `I_EHR_SERVICE.`.
    #[arg(long)]
    prefix: String,
    /// The capability folder the document is written to.
    #[arg(long)]
    capability: String,
    /// The document name, without `.arazzo.yaml`.
    #[arg(long)]
    document: String,
    /// The new catalogue root.
    #[arg(long, default_value = "catalogue")]
    catalogue: PathBuf,
}

/// What a run of the converter cannot go on without.
#[derive(Debug, thiserror::Error)]
enum ConvertError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Load(#[from] veredictum::load::LoadError),
}

impl From<String> for ConvertError {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

/// The citation placeholder the gate refuses.
const UNCITED: &str = "UNCITED";

/// A released route: `(METHOD, /path/{}/…)`.
type RouteKey = (String, String);

/// What a route resolves to: `(source, operationId, OAS path)`.
type Route = (String, String, String);

/// The released operations, by method and normalized path.
#[derive(Debug, Default)]
struct OasIndex {
    routes: BTreeMap<RouteKey, Vec<Route>>,
}

impl OasIndex {
    fn load(dir: &Path) -> Result<Self, ConvertError> {
        let mut index = Self::default();
        let mut files = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.to_string_lossy().ends_with("-validation.openapi.yaml") {
                files.push(path);
            }
        }
        files.sort();
        for file in files {
            let Some(source) = file
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_suffix("-validation.openapi.yaml"))
                .map(str::to_owned)
            else {
                continue;
            };
            let document = veredictum::load::yaml_file_to_value(&file)?;
            let Some(Value::Object(paths)) = document.get("paths") else {
                continue;
            };
            for (route, item) in paths {
                let Value::Object(item) = item else { continue };
                for (method, operation) in item {
                    if let Some(Value::String(id)) = operation.get("operationId") {
                        index
                            .routes
                            .entry((method.to_uppercase(), normalize_path(route)))
                            .or_default()
                            .push((source.clone(), id.clone(), route.clone()));
                    }
                }
            }
        }
        Ok(index)
    }

    /// The source, `operationId` and OAS path of `method` on `path`.
    fn resolve(&self, method: &str, path: &str) -> Option<&Route> {
        self.routes
            .get(&(method.to_uppercase(), normalize_path(path)))
            .and_then(|found| found.first())
    }
}

/// A path with every `{param}` written as `{}`, so a binding's parameter
/// names need not match the OAS ones.
fn normalize_path(path: &str) -> String {
    let mut out = String::new();
    let mut depth = 0_u32;
    for c in path.chars() {
        match c {
            '{' => {
                depth = depth.saturating_add(1);
                out.push('{');
            }
            '}' => {
                depth = depth.saturating_sub(1);
                out.push('}');
            }
            _ if depth > 0 => {}
            _ => out.push(c),
        }
    }
    out
}

/// The parameter names of an OAS path template, in order.
fn path_params(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        let tail = rest.get(start.saturating_add(1)..).unwrap_or_default();
        let Some(end) = tail.find('}') else { break };
        out.push(tail.get(..end).unwrap_or_default().to_owned());
        rest = tail.get(end.saturating_add(1)..).unwrap_or_default();
    }
    out
}

/// A row: the cell of each matrix column.
type MatrixRow = BTreeMap<String, MatrixCell>;

/// Everything one workflow under construction refers to.
#[derive(Debug, Default)]
struct Scope {
    /// Matrix columns that are inputs, by name.
    row: BTreeMap<String, Value>,
    /// Capture name → expression.
    captures: BTreeMap<String, String>,
    /// `requires` handle name → expression.
    handles: BTreeMap<String, String>,
    /// The fixture payload input, for `${ds:fixture}`.
    fixture: bool,
}

/// What converting one case or one step leaves behind for the hand.
#[derive(Debug, Default)]
struct Notes {
    lines: Vec<String>,
}

impl Notes {
    fn add(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
    }
}

/// The converter's state for one component.
struct Converter<'a> {
    set: &'a ArtifactSet,
    oas: OasIndex,
    prefix: String,
    document: String,
    /// The sources the document names.
    sources: BTreeSet<String>,
    /// Setup workflows, by id.
    setups: BTreeMap<String, (Value, Vec<String>)>,
}

/// A workflow's rows: the iteration, and each row with its comment.
type RowsOut = (Iteration, Vec<(Value, Option<String>)>);

/// One converted workflow with its rows and the hand notes.
struct Converted {
    workflow: Value,
    rows: Option<RowsOut>,
    notes: Vec<String>,
    old_id: String,
}

fn placeholder(hint: &str) -> Value {
    json!({ "source": UNCITED, "section": UNCITED, "quote": hint })
}

fn sanitize(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn format_media(format: FormatName) -> &'static str {
    match format {
        FormatName::CanonicalJson => "application/json",
        FormatName::CanonicalXml => "application/xml",
        FormatName::WtFlat => "application/openehr.wt.flat+json",
        FormatName::WtStructured => "application/openehr.wt.structured+json",
        FormatName::Wt => "application/openehr.wt+json",
    }
}

impl<'a> Converter<'a> {
    fn binding(&self, case: &CaseCore, step: &FlowStep) -> Option<&'a OperationBinding> {
        let operation = if step.call.contains('.') {
            veredictum::ids::SmOperationRef::parse(&step.call).ok()?
        } else {
            case.sm_operation.as_ref()?.sibling(&step.call)
        };
        let set = self.set;
        let bindings = || set.bindings.iter().map(|(_, b)| b);
        if let Some(variant) = &step.variant
            && let Some(exact) = bindings()
                .find(|b| b.sm_operation == operation && b.variant.as_ref() == Some(variant))
        {
            return Some(exact);
        }
        bindings().find(|b| b.sm_operation == operation && b.variant.is_none())
    }

    /// The `with_<p>` variant the old driver auto-selected for this row.
    fn auto_variant(
        &self,
        base: &'a OperationBinding,
        step: &FlowStep,
        row: &MatrixRow,
    ) -> &'a OperationBinding {
        if step.variant.is_some() {
            return base;
        }
        self.set
            .bindings
            .iter()
            .map(|(_, b)| b)
            .find(|b| {
                b.sm_operation == base.sm_operation
                    && b.variant.as_deref().is_some_and(|v| {
                        v.strip_prefix("with_")
                            .is_some_and(|param| with_present(step, param, row))
                    })
            })
            .unwrap_or(base)
    }

    fn corpus_payload(&self, key: &str) -> Result<Value, String> {
        let manifest = self
            .set
            .corpus
            .as_ref()
            .map(|(_, m)| m)
            .ok_or("no corpus manifest")?;
        let dir = self.set.corpus_dir.as_ref().ok_or("no corpus directory")?;
        let entry = manifest
            .entries()
            .iter()
            .find(|(k, _)| k.as_str() == key)
            .map(|(_, e)| e)
            .ok_or_else(|| format!("corpus key {key} is not in the manifest"))?;
        let source = entry
            .source
            .as_ref()
            .ok_or_else(|| format!("corpus key {key} is generated, not a file"))?;
        let text = std::fs::read_to_string(dir.join(source)).map_err(|e| e.to_string())?;
        if Path::new(source)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        {
            serde_json::from_str(&text).map_err(|e| e.to_string())
        } else {
            Ok(Value::String(text))
        }
    }
}

/// Whether a step binds `param` to a non-null value for this row.
fn with_present(step: &FlowStep, param: &str, row: &MatrixRow) -> bool {
    let Some(with) = &step.with else { return false };
    let Some((_, value)) = with.iter().find(|(k, _)| k == param) else {
        return false;
    };
    match value {
        TemplatedValue::Null => false,
        TemplatedValue::Text(template) => match template.as_single_ref() {
            Some(ValueRef::Row(column)) => !matches!(
                row.get(column),
                Some(MatrixCell::Absent | MatrixCell::Null) | None
            ),
            _ => true,
        },
        _ => true,
    }
}

/// A template as a runtime expression, an embedding string or a literal.
fn template_value(template: &Template, scope: &Scope, notes: &mut Notes) -> Value {
    if let Some(reference) = template.as_single_ref() {
        return reference_value(reference, scope, notes);
    }
    let mut out = String::new();
    for segment in template.segments() {
        match segment {
            Segment::Lit(text) => out.push_str(text),
            Segment::Ref(reference) => match reference_value(reference, scope, notes) {
                Value::String(expr) if expr.starts_with('$') => {
                    let _ = write!(out, "{{{expr}}}");
                }
                Value::String(text) => out.push_str(&text),
                other => out.push_str(&other.to_string()),
            },
        }
    }
    Value::String(out)
}

fn reference_value(reference: &ValueRef, scope: &Scope, notes: &mut Notes) -> Value {
    match reference {
        ValueRef::Row(column) => {
            if scope.row.contains_key(column) {
                Value::String(format!("$inputs.{column}"))
            } else {
                notes.add(format!(
                    "row column `{column}` is not an input of this workflow"
                ));
                Value::String(format!("$inputs.{column}"))
            }
        }
        ValueRef::Capture { name, .. } => {
            let name = name.as_str();
            if let Some(expr) = scope.captures.get(name).or_else(|| scope.handles.get(name)) {
                Value::String(expr.clone())
            } else {
                notes.add(format!("`${{{name}}}` names no capture or handle"));
                Value::String(format!("$inputs.{name}"))
            }
        }
        ValueRef::FixtureDataSet if scope.fixture => Value::String("$inputs.payload".to_owned()),
        other => {
            notes.add(format!(
                "`${{{other}}}` has no runtime-expression counterpart"
            ));
            Value::String(format!("UNCONVERTED ${{{other}}}"))
        }
    }
}

/// A templated `with` value as an Arazzo value.
fn templated(value: &TemplatedValue, scope: &Scope, notes: &mut Notes) -> Value {
    match value {
        TemplatedValue::Null => Value::Null,
        TemplatedValue::Bool(b) => Value::Bool(*b),
        TemplatedValue::Number(n) => Value::Number(n.clone()),
        TemplatedValue::Text(template) => template_value(template, scope, notes),
        TemplatedValue::Seq(items) => {
            Value::Array(items.iter().map(|v| templated(v, scope, notes)).collect())
        }
        TemplatedValue::Map(entries) => Value::Object(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), templated(v, scope, notes)))
                .collect(),
        ),
    }
}

/// The criteria one outcome of a binding asserts.
#[expect(
    clippy::too_many_lines,
    reason = "one arm per closed header-matcher form, in vocabulary order"
)]
fn outcome_criteria(
    binding: &OperationBinding,
    kind: OutcomeKind,
    universal: Option<u16>,
    accept: Option<&str>,
    scope: &Scope,
    notes: &mut Notes,
) -> Vec<Value> {
    let label = format!("binding {} outcome {}", binding_name(binding), kind.token());
    let expectation: Option<&WireExpectation> = binding
        .outcomes
        .as_ref()
        .and_then(|o| o.iter().find(|(k, _)| k.0 == kind))
        .map(|(_, e)| e);
    let Some(expectation) = expectation else {
        if let Some(status) = universal {
            return vec![json!({
                "condition": format!("$statusCode == {status}"),
                "x-veredictum-cite": placeholder(&format!("universal outcome {}: status {status}", kind.token())),
            })];
        }
        notes.add(format!("{label}: the binding declares no such outcome"));
        return Vec::new();
    };
    let status = expectation.status.value();
    let mut out = Vec::new();
    match &expectation.alt_status {
        Some(alt) if !alt.is_empty() => {
            let codes: Vec<String> = std::iter::once(status)
                .chain(alt.iter().map(|s| s.value()))
                .map(|s| s.to_string())
                .collect();
            out.push(json!({
                "context": "$statusCode",
                "type": "regex",
                "condition": format!("^({})$", codes.join("|")),
                "x-veredictum-cite": placeholder(&format!("{label}: status {}", codes.join(" or "))),
            }));
        }
        _ => out.push(json!({
            "condition": format!("$statusCode == {status}"),
            "x-veredictum-cite": placeholder(&format!("{label}: status {status}")),
        })),
    }
    for (name, expectations) in expectation.headers.iter().flatten() {
        for header in expectations.all() {
            let context = format!("$response.header.{name}");
            let hint = format!("{label}: header {name}");
            if header.optional {
                notes.add(format!(
                    "{hint}: presence is optional, which no criterion expresses"
                ));
                continue;
            }
            if header.applies.is_some() {
                notes.add(format!("{hint}: the matcher carries a version floor"));
            }
            let criterion = match &header.matcher {
                HeaderMatcher::Present => json!({ "condition": format!("{context} != null") }),
                HeaderMatcher::Absent => json!({ "condition": format!("{context} == null") }),
                HeaderMatcher::Negotiated => {
                    let Some(media) = accept else {
                        notes.add(format!("{hint}: negotiated with no Accept sent"));
                        continue;
                    };
                    json!({
                        "context": context, "type": "regex",
                        "condition": format!("^{}", regex::escape(media)),
                    })
                }
                HeaderMatcher::Pattern(pattern) => {
                    let resolved = placeholders_to_expressions(pattern, scope);
                    let unresolved = veredictum::model::binding::placeholder_names(&resolved);
                    if !unresolved.is_empty() {
                        notes.add(format!(
                            "{hint}: placeholders {unresolved:?} name no capture, handle or input"
                        ));
                    }
                    json!({
                        "context": context, "type": "regex",
                        "condition": format!("^(?:{resolved})$"),
                    })
                }
                HeaderMatcher::Literal(template) => {
                    let value = template_value(template, scope, notes);
                    let text = match value {
                        Value::String(t) if t.starts_with('$') => format!("{{{t}}}"),
                        Value::String(t) => regex::escape(&t),
                        other => regex::escape(&other.to_string()),
                    };
                    json!({ "context": context, "type": "regex", "condition": format!("^{text}$") })
                }
                HeaderMatcher::PresentWithBody | HeaderMatcher::LatestVersionUid => {
                    notes.add(format!(
                        "{hint}: a conditional matcher no criterion expresses"
                    ));
                    continue;
                }
            };
            let mut criterion = criterion;
            if let Value::Object(map) = &mut criterion {
                map.insert("x-veredictum-cite".to_owned(), placeholder(&hint));
            }
            out.push(criterion);
        }
    }
    if let Some(body) = expectation.body {
        notes.add(format!("{label}: body selector {body:?} is not converted"));
    }
    out
}

/// A header pattern's `<name>` placeholders as embedded expressions.
fn placeholders_to_expressions(pattern: &str, scope: &Scope) -> String {
    let mut out = pattern.to_owned();
    for (name, expr) in scope.captures.iter().chain(&scope.handles) {
        out = out.replace(&format!("<{name}>"), &format!("{{{expr}}}"));
    }
    for name in scope.row.keys() {
        out = out.replace(&format!("<{name}>"), &format!("{{$inputs.{name}}}"));
    }
    out
}

fn binding_name(binding: &OperationBinding) -> String {
    match &binding.variant {
        Some(variant) => format!("{}#{variant}", binding.sm_operation),
        None => binding.sm_operation.to_string(),
    }
}

/// An RM path as a JSON Pointer under the response body.
fn body_pointer(path: &str) -> String {
    format!("$response.body#/{}", path.trim_start_matches('/'))
}

fn assertion_criteria(
    assertion: &Assertion,
    scope: &Scope,
    converter: &Converter<'_>,
    binding: &OperationBinding,
    notes: &mut Notes,
) -> Vec<Value> {
    let hint = |what: &str| placeholder(&format!("assertion {what}"));
    match assertion {
        Assertion::Field {
            path,
            equals,
            exists,
            absent,
            matches,
            ..
        } => {
            let context = body_pointer(path);
            let mut out = Vec::new();
            if let Some(value) = equals {
                match templated(value, scope, notes) {
                    Value::String(text) => {
                        let pattern = if text.starts_with('$') {
                            format!("{{{text}}}")
                        } else {
                            regex::escape(&text)
                        };
                        out.push(json!({
                            "context": context, "type": "regex",
                            "condition": format!("^{pattern}$"),
                            "x-veredictum-cite": hint(&format!("field {path} equals")),
                        }));
                    }
                    other => out.push(json!({
                        "condition": format!("{context} == {other}"),
                        "x-veredictum-cite": hint(&format!("field {path} equals")),
                    })),
                }
            }
            if *exists == Some(true) {
                out.push(json!({ "condition": format!("{context} != null"), "x-veredictum-cite": hint(&format!("field {path} exists")) }));
            }
            if *absent == Some(true) {
                out.push(json!({ "condition": format!("{context} == null"), "x-veredictum-cite": hint(&format!("field {path} absent")) }));
            }
            if let Some(pattern) = matches {
                out.push(json!({ "context": context, "type": "regex", "condition": pattern, "x-veredictum-cite": hint(&format!("field {path} matches")) }));
            }
            if out.is_empty() {
                notes.add(format!(
                    "field {path}: a predicate with no criterion counterpart"
                ));
            }
            out
        }
        Assertion::Equivalent { to, ignoring } => {
            let target = match to {
                EquivalentTarget::Ref(ValueRef::DataSet { key, view: None }) => {
                    match converter.corpus_payload(key.as_str()) {
                        Ok(value) => value,
                        Err(e) => {
                            notes.add(format!("equivalent: {e}"));
                            return Vec::new();
                        }
                    }
                }
                EquivalentTarget::Ref(reference) => reference_value(reference, scope, notes),
                EquivalentTarget::Committed => {
                    notes.add("equivalent to `committed`: name the committed input by hand");
                    return Vec::new();
                }
            };
            let mut paths = Vec::new();
            for spec in &ignoring.0 {
                match spec {
                    IgnoreSpec::Path(path) => paths.push(path.clone()),
                    IgnoreSpec::Named(_) => paths.extend(binding.server_assigned.iter().cloned()),
                }
            }
            vec![json!({
                "condition": "$statusCode == 200",
                "x-veredictum-check": { "judge": "equivalent", "context": "$response.body", "to": target, "ignoring": paths },
                "x-veredictum-cite": hint("equivalent"),
            })]
        }
        other => {
            notes.add(format!(
                "assertion {} has no criterion counterpart",
                assertion_name(other)
            ));
            Vec::new()
        }
    }
}

fn assertion_name(assertion: &Assertion) -> String {
    let debug = format!("{assertion:?}");
    debug
        .split([' ', '{', '('])
        .next()
        .unwrap_or_default()
        .to_owned()
}

impl Converter<'_> {
    #[expect(
        clippy::too_many_lines,
        reason = "one block per part of a flow step, in step order"
    )]
    fn step(
        &mut self,
        case: &CaseCore,
        step: &FlowStep,
        row: &MatrixRow,
        scope: &mut Scope,
        notes: &mut Notes,
    ) -> Option<Value> {
        let Some(base) = self.binding(case, step) else {
            notes.add(format!(
                "step {}: no binding for `{}`",
                step.step, step.call
            ));
            return None;
        };
        let binding = self.auto_variant(base, step, row);
        let Some(request) = &binding.request else {
            notes.add(format!(
                "step {}: binding {} has no request (unrealized or an extension)",
                step.step,
                binding_name(binding)
            ));
            return None;
        };
        let method = format!("{:?}", request.method).to_uppercase();
        let Some((source, operation, oas_path)) =
            self.oas.resolve(&method, request.path.raw()).cloned()
        else {
            notes.add(format!(
                "step {}: {method} {} is in no released OAS",
                step.step,
                request.path.raw()
            ));
            return None;
        };
        self.sources.insert(source.clone());
        let step_id = format!("{}-{}", sanitize(&step.call.replace('.', "_")), step.step);
        let with: BTreeMap<String, &TemplatedValue> = step
            .with
            .iter()
            .flatten()
            .map(|(k, v)| (k.clone(), v))
            .collect();
        let lookup = |name: &str, scope: &Scope, notes: &mut Notes| -> Option<Value> {
            if let Some(value) = with.get(name) {
                if !with_present(step, name, row) {
                    return None;
                }
                return Some(templated(value, scope, notes));
            }
            scope
                .captures
                .get(name)
                .or_else(|| scope.handles.get(name))
                .map(|e| Value::String(e.clone()))
        };
        let mut parameters = Vec::new();
        for (binding_param, oas_param) in request.path.params().iter().zip(path_params(&oas_path)) {
            match lookup(binding_param.as_str(), scope, notes) {
                Some(value) => {
                    parameters.push(json!({ "name": oas_param, "in": "path", "value": value }));
                }
                None => notes.add(format!(
                    "step {}: path parameter `{binding_param}` is unbound",
                    step.step
                )),
            }
        }
        for (name, value) in request.query.iter().flatten() {
            match value {
                QueryValue::Single(template) => {
                    let resolved = match template.as_single_ref() {
                        Some(ValueRef::Capture { name: inner, .. }) => {
                            lookup(inner.as_str(), scope, notes)
                        }
                        _ => Some(template_value(template, scope, notes)),
                    };
                    if let Some(resolved) = resolved {
                        parameters.push(json!({ "name": name, "in": "query", "value": resolved }));
                    }
                }
                QueryValue::Repeated(_) => {
                    notes.add(format!("step {}: repeated query `{name}`", step.step));
                }
            }
        }
        for (name, template) in request.headers.iter().flatten() {
            let value = template_value(template, scope, notes);
            parameters.push(json!({ "name": name, "in": "header", "value": value }));
        }
        let format = step.format.or_else(|| binding.formats.first().copied());
        let accept = format.map(format_media);
        if let Some(media) = accept
            && !binding.formats.is_empty()
        {
            parameters.push(json!({ "name": "Accept", "in": "header", "value": media }));
        }
        let mut out = Map::new();
        out.insert("stepId".to_owned(), Value::String(step_id.clone()));
        out.insert(
            "description".to_owned(),
            Value::String(format!(
                "UNDESCRIBED: {} via {}",
                step.call,
                binding_name(binding)
            )),
        );
        out.insert(
            "operationId".to_owned(),
            Value::String(format!("$sourceDescriptions.{source}.{operation}")),
        );
        if !parameters.is_empty() {
            out.insert("parameters".to_owned(), Value::Array(parameters));
        }
        if let Some(RequestBody::Named { name, optional }) = &request.body {
            match with.get(name.as_str()) {
                Some(value) if with_present(step, name, row) => {
                    let payload = match value {
                        TemplatedValue::Text(t) => match t.as_single_ref() {
                            Some(ValueRef::DataSet { key, view: None }) => {
                                match self.corpus_payload(key.as_str()) {
                                    Ok(v) => Some(v),
                                    Err(e) => {
                                        notes.add(format!("step {}: {e}", step.step));
                                        None
                                    }
                                }
                            }
                            Some(ValueRef::Recipe(recipe)) => {
                                notes.add(format!(
                                    "step {}: body from recipe `{recipe}` has no Arazzo recipe; the body is left out",
                                    step.step
                                ));
                                None
                            }
                            _ => Some(templated(value, scope, notes)),
                        },
                        other => Some(templated(other, scope, notes)),
                    };
                    if let Some(payload) = payload {
                        let content_type = if payload.is_string()
                            && !payload.as_str().is_some_and(|s| s.starts_with('$'))
                        {
                            "application/xml"
                        } else {
                            format.map_or("application/json", format_media)
                        };
                        out.insert(
                            "requestBody".to_owned(),
                            json!({ "contentType": content_type, "payload": payload }),
                        );
                    }
                }
                _ if *optional => {}
                _ => notes.add(format!(
                    "step {}: required body `{name}` is unbound",
                    step.step
                )),
            }
        }
        let mut criteria = Vec::new();
        let universal = |kind: OutcomeKind| {
            self.set
                .selectors
                .as_ref()
                .and_then(|(_, s)| s.universal_outcomes.as_ref())
                .and_then(|u| u.iter().find(|(k, _)| k == kind.token()))
                .map(|(_, o)| o.status)
        };
        match &step.expect {
            ExpectSpec::Kind(kind) => {
                criteria.extend(outcome_criteria(
                    binding,
                    *kind,
                    universal(*kind),
                    accept,
                    scope,
                    notes,
                ));
            }
            ExpectSpec::FixtureExpected => {
                if let Some(Value::String(kind)) = scope.row.get("expected")
                    && let Some(kind) = OutcomeKind::from_token(kind)
                {
                    criteria.extend(outcome_criteria(
                        binding,
                        kind,
                        universal(kind),
                        accept,
                        scope,
                        notes,
                    ));
                }
            }
        }
        for assertion in &step.assertions {
            criteria.extend(assertion_criteria(assertion, scope, self, binding, notes));
        }
        out.insert("successCriteria".to_owned(), Value::Array(criteria));
        let mut outputs = Map::new();
        for (name, source) in step.capture.iter().flatten() {
            let CaptureField::Field {
                name: field,
                list: false,
            } = &source.field
            else {
                notes.add(format!(
                    "step {}: capture `{name}` reads {}",
                    step.step, source
                ));
                continue;
            };
            let Some((_, capture)) = binding.captures.iter().flatten().find(|(k, _)| k == field)
            else {
                notes.add(format!("step {}: binding captures no `{field}`", step.step));
                continue;
            };
            let expr = match &capture.from {
                WireFrom::Body { path } => body_pointer(&path.replace('.', "/")),
                WireFrom::Header {
                    name,
                    last_segment: false,
                } => format!("$response.header.{name}"),
                other => {
                    notes.add(format!(
                        "step {}: capture `{name}` from {other:?}",
                        step.step
                    ));
                    continue;
                }
            };
            if capture.strip.is_some() || capture.transform.is_some() {
                notes.add(format!(
                    "step {}: capture `{name}` strips or transforms its value",
                    step.step
                ));
            }
            if capture.fallback.is_some() {
                notes.add(format!(
                    "step {}: capture `{name}` has a fallback",
                    step.step
                ));
            }
            outputs.insert(name.as_str().to_owned(), Value::String(expr));
            scope.captures.insert(
                name.as_str().to_owned(),
                format!("$steps.{step_id}.outputs.{}", name.as_str()),
            );
        }
        if !outputs.is_empty() {
            out.insert("outputs".to_owned(), Value::Object(outputs));
        }
        Some(Value::Object(out))
    }

    fn setup_ehr(&mut self) -> String {
        let id = "setup-ehr".to_owned();
        if !self.setups.contains_key(&id) {
            self.sources.insert("ehr".to_owned());
            self.setups.insert(
                id.clone(),
                (
                    json!({
                        "workflowId": id,
                        "summary": "An EHR the case addresses",
                        "description": "Creates an EHR and hands its ehr_id to the cases that depend on it.",
                        "x-veredictum-cite": placeholder("requires.ehr"),
                        "steps": [{
                            "stepId": "create",
                            "description": "Create an EHR and ask for the full resource in the response.",
                            "operationId": "$sourceDescriptions.ehr.ehr_create",
                            "parameters": [
                                { "name": "Prefer", "in": "header", "value": "return=representation" },
                                { "name": "Accept", "in": "header", "value": "application/json" }
                            ],
                            "successCriteria": [{ "condition": "$statusCode == 201" }],
                            "outputs": { "ehr_id": "$response.body#/ehr_id/value" }
                        }],
                        "outputs": { "ehr_id": "$steps.create.outputs.ehr_id" }
                    }),
                    Vec::new(),
                ),
            );
        }
        id
    }

    fn setup_template(&mut self, key: &str) -> String {
        let id = format!("upload_template-{}", sanitize(key));
        if !self.setups.contains_key(&id) {
            self.sources.insert("definition".to_owned());
            let mut notes = Vec::new();
            let payload = self.corpus_payload(key).unwrap_or_else(|e| {
                notes.push(format!("template {key}: {e}"));
                Value::String(String::new())
            });
            self.setups.insert(
                id.clone(),
                (
                    json!({
                        "workflowId": id,
                        "summary": format!("The template {key} is on the server"),
                        "description": "Uploads the operational template, or finds it already there.",
                        "x-veredictum-cite": placeholder(&format!("requires.templates {key}")),
                        "steps": [{
                            "stepId": "upload",
                            "description": "Upload the template, or find it already there.",
                            "operationId": "$sourceDescriptions.definition.definition_template_adl1.4_upload",
                            "parameters": [{ "name": "Content-Type", "in": "header", "value": "application/xml" }],
                            "requestBody": { "contentType": "application/xml", "payload": payload },
                            "successCriteria": [{ "context": "$statusCode", "type": "regex", "condition": "^(201|409)$" }]
                        }]
                    }),
                    notes,
                ),
            );
        }
        id
    }

    /// The row shapes of a case: groups of rows that call the same
    /// operations and send a body in the same steps.
    fn row_groups(&self, case: &CaseCore, rows: &[MatrixRow]) -> Vec<(String, Vec<usize>)> {
        let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut labels: BTreeMap<String, String> = BTreeMap::new();
        for (index, row) in rows.iter().enumerate() {
            let mut signature = String::new();
            let mut label = Vec::new();
            for step in &case.flow {
                let Some(base) = self.binding(case, step) else {
                    continue;
                };
                let binding = self.auto_variant(base, step, row);
                let _ = write!(signature, "{}|", binding_name(binding));
                if let Some(RequestBody::Named { name, .. }) =
                    binding.request.as_ref().and_then(|r| r.body.as_ref())
                {
                    let present = with_present(step, name, row);
                    let _ = write!(signature, "{present}|");
                }
            }
            for (column, cell) in row {
                if matches!(cell, MatrixCell::Absent) {
                    label.push(format!("{column}_absent"));
                }
            }
            labels
                .entry(signature.clone())
                .or_insert_with(|| label.join("-"));
            groups.entry(signature).or_default().push(index);
        }
        groups
            .into_iter()
            .map(|(signature, indices)| (labels.remove(&signature).unwrap_or_default(), indices))
            .collect()
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one block per part of a case core, in field order"
    )]
    fn convert_case(&mut self, case: &CaseCore) -> Vec<Converted> {
        let base_id = sanitize(
            case.id
                .as_str()
                .strip_prefix(&self.prefix)
                .unwrap_or(case.id.as_str()),
        );
        let mut case_notes = Notes::default();
        if case.option.is_some() {
            case_notes.add(format!(
                "option tag `{}`: the arm a statement selects",
                case.option
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default()
            ));
        }
        if !case.formats.is_empty() {
            case_notes.add(format!("format axis {:?}", case.formats));
        }
        if !case.ambiguities.is_empty() {
            case_notes.add(format!(
                "ambiguity register entries {:?}: re-adjudicate against the released text",
                case.ambiguities
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            ));
        }
        for guard in &case.guards {
            case_notes.add(format!("guard: {guard}"));
        }
        if let Some(server @ ServerState::Empty) = &case.requires.server {
            case_notes.add(format!(
                "requires.server {server:?}: generated inputs replace an empty server"
            ));
        }
        for postcondition in &case.postconditions {
            case_notes.add(format!(
                "postcondition {} is not converted",
                assertion_name(postcondition)
            ));
        }
        if !case.verified_by.is_empty() {
            case_notes.add(format!(
                "verified_by {:?}: read the state back in a step by hand",
                case.verified_by
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            ));
        }
        let mut depends = Vec::new();
        let mut handles = BTreeMap::new();
        if let Some(EhrRequirement::Exists { .. }) = &case.requires.ehr {
            let id = self.setup_ehr();
            handles.insert(
                "ehr_id".to_owned(),
                format!("$workflows.{id}.outputs.ehr_id"),
            );
            depends.push(id);
        }
        for template in &case.requires.templates {
            depends.push(self.setup_template(template.as_str()));
        }
        // NOTE: no openEHR spec governs this — our own design; a matrix row and a
        // fixture entry both become one rows-file row, keyed by column.
        let parameters = case.parameters.as_ref();
        let iteration = parameters.map_or(Iteration::ResetPerRow, |p| p.iteration);
        let (rows, fixture): (Vec<MatrixRow>, Option<&[FixtureEntry]>) = match parameters
            .and_then(|p| p.matrix.as_ref())
        {
            Some(matrix) => (
                matrix
                    .rows
                    .iter()
                    .map(|cells| {
                        matrix
                            .columns
                            .iter()
                            .cloned()
                            .zip(cells.iter().cloned())
                            .collect()
                    })
                    .collect(),
                None,
            ),
            None => match parameters.and_then(|p| p.fixture_set.as_deref()) {
                Some(entries) => (
                    entries
                        .iter()
                        .map(|e| {
                            BTreeMap::from([(
                                "expected".to_owned(),
                                MatrixCell::Literal(Value::String(e.expected.token().to_owned())),
                            )])
                        })
                        .collect(),
                    Some(entries),
                ),
                None => (vec![MatrixRow::new()], None),
            },
        };
        let groups = if fixture.is_some() {
            let mut by_kind: BTreeMap<String, Vec<usize>> = BTreeMap::new();
            for (index, row) in rows.iter().enumerate() {
                if let Some(MatrixCell::Literal(Value::String(kind))) = row.get("expected") {
                    by_kind.entry(kind.clone()).or_default().push(index);
                }
            }
            by_kind.into_iter().collect::<Vec<_>>()
        } else {
            self.row_groups(case, &rows)
        };
        let split = groups.len() > 1;
        let mut out = Vec::new();
        for (label, indices) in groups {
            let workflow_id = if split && !label.is_empty() {
                format!("{base_id}-{}", sanitize(&label))
            } else {
                base_id.clone()
            };
            let mut notes = Notes {
                lines: case_notes.lines.clone(),
            };
            let Some(first) = indices.first().and_then(|i| rows.get(*i)) else {
                continue;
            };
            let mut scope = Scope {
                handles: handles.clone(),
                fixture: fixture.is_some(),
                ..Scope::default()
            };
            let mut properties = Map::new();
            let mut required = Vec::new();
            let group: Vec<&MatrixRow> = indices.iter().filter_map(|i| rows.get(*i)).collect();
            let columns: BTreeSet<&String> = group.iter().flat_map(|row| row.keys()).collect();
            for column in columns {
                let cells: Vec<Option<&MatrixCell>> =
                    group.iter().map(|row| row.get(column)).collect();
                let literal = |cell: Option<&MatrixCell>| -> bool {
                    matches!(cell, Some(MatrixCell::Literal(_) | MatrixCell::Null))
                };
                if fixture.is_some() && column == "expected" {
                    if let Some(cell) = first.get(column) {
                        scope.row.insert(column.clone(), cell_value(cell));
                    }
                } else if cells
                    .iter()
                    .all(|c| matches!(c, Some(MatrixCell::Provided)))
                {
                    properties.insert(
                        column.clone(),
                        json!({ "type": "string", "format": "uuid" }),
                    );
                    scope.row.insert(column.clone(), Value::Null);
                } else if cells.iter().copied().any(literal) {
                    properties.insert(column.clone(), json!({}));
                    if cells.iter().copied().all(literal) {
                        required.push(Value::String(column.clone()));
                    } else {
                        notes.add(format!(
                            "column `{column}` is a literal in some rows only; the others leave it out"
                        ));
                    }
                    scope.row.insert(column.clone(), Value::Null);
                }
            }
            if fixture.is_some() {
                properties.insert("payload".to_owned(), json!({}));
                required.push(Value::String("payload".to_owned()));
            }
            let mut steps = Vec::new();
            for step in &case.flow {
                if let Some(converted) = self.step(case, step, first, &mut scope, &mut notes) {
                    steps.push(converted);
                }
            }
            if steps.is_empty() {
                out.push(Converted {
                    workflow: Value::Null,
                    rows: None,
                    notes: notes.lines,
                    old_id: case.id.to_string(),
                });
                continue;
            }
            let mut workflow = Map::new();
            workflow.insert("workflowId".to_owned(), Value::String(workflow_id.clone()));
            workflow.insert(
                "summary".to_owned(),
                Value::String(case.description.clone()),
            );
            workflow.insert(
                "description".to_owned(),
                Value::String(case.test_purpose.trim().to_owned()),
            );
            workflow.insert(
                "x-veredictum-cite".to_owned(),
                placeholder(&format!(
                    "old spec_refs: {}",
                    case.spec_refs
                        .iter()
                        .filter(|r| !r.starts_with("SM "))
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("; ")
                )),
            );
            if !depends.is_empty() {
                workflow.insert("dependsOn".to_owned(), json!(depends));
            }
            if !properties.is_empty() {
                let mut inputs = Map::new();
                inputs.insert("type".to_owned(), Value::String("object".to_owned()));
                if !required.is_empty() {
                    inputs.insert("required".to_owned(), Value::Array(required));
                }
                inputs.insert("properties".to_owned(), Value::Object(properties));
                workflow.insert("inputs".to_owned(), Value::Object(inputs));
            }
            workflow.insert("steps".to_owned(), Value::Array(steps));
            let row_values: Vec<(Value, Option<String>)> = indices
                .iter()
                .filter_map(|i| rows.get(*i).map(|row| (*i, row)))
                .filter_map(|(i, row)| {
                    let mut values = Map::new();
                    for (column, cell) in row {
                        if matches!(cell, MatrixCell::Literal(_) | MatrixCell::Null)
                            && !(fixture.is_some() && column == "expected")
                        {
                            values.insert(column.clone(), cell_value(cell));
                        }
                    }
                    let mut comment = None;
                    if let Some(entry) = fixture.and_then(|f| f.get(i)) {
                        match self.corpus_payload(entry.data_set.as_str()) {
                            Ok(payload) => {
                                values.insert("payload".to_owned(), payload);
                            }
                            Err(e) => notes.add(format!("fixture row {i}: {e}")),
                        }
                        comment = Some(format!(
                            "{} — {}",
                            entry.data_set,
                            entry.defect.clone().unwrap_or_default()
                        ));
                    }
                    (!values.is_empty()).then_some((Value::Object(values), comment))
                })
                .collect();
            let rows_out = (!row_values.is_empty()).then_some((iteration, row_values));
            out.push(Converted {
                workflow: Value::Object(workflow),
                rows: rows_out,
                notes: notes.lines,
                old_id: case.id.to_string(),
            });
        }
        out
    }
}

fn cell_value(cell: &MatrixCell) -> Value {
    match cell {
        MatrixCell::Literal(value) => value.clone(),
        MatrixCell::Null | MatrixCell::Absent | MatrixCell::Provided => Value::Null,
    }
}

/// Writes `value` as block YAML at `indent`, every string double-quoted
/// unless it is a plain token.
fn emit(value: &Value, indent: usize, out: &mut String) {
    let pad = " ".repeat(indent);
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (key, value) in map {
                let _ = write!(out, "{pad}{key}:");
                emit_after_key(value, indent, out);
            }
        }
        Value::Array(items) if !items.is_empty() => {
            for item in items {
                let _ = write!(out, "{pad}- ");
                emit_item(item, indent.saturating_add(2), out);
            }
        }
        other => {
            let _ = writeln!(out, "{pad}{}", scalar(other));
        }
    }
}

fn emit_after_key(value: &Value, indent: usize, out: &mut String) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            out.push('\n');
            emit(value, indent.saturating_add(2), out);
        }
        Value::Array(items) if !items.is_empty() => {
            out.push('\n');
            emit(value, indent.saturating_add(2), out);
        }
        other => {
            let _ = writeln!(out, " {}", scalar(other));
        }
    }
}

/// An item of a list, whose first key shares the `- ` line.
fn emit_item(value: &Value, indent: usize, out: &mut String) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            let pad = " ".repeat(indent);
            for (position, (key, value)) in map.iter().enumerate() {
                if position == 0 {
                    let _ = write!(out, "{key}:");
                } else {
                    let _ = write!(out, "{pad}{key}:");
                }
                emit_after_key(value, indent, out);
            }
        }
        Value::Array(items) if !items.is_empty() => {
            out.push('\n');
            emit(value, indent, out);
        }
        other => {
            let _ = writeln!(out, "{}", scalar(other));
        }
    }
}

fn scalar(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(text) => {
            let plain = !text.is_empty()
                && text
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_-./".contains(c))
                && text.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && !matches!(
                    text.as_str(),
                    "null" | "true" | "false" | "yes" | "no" | "on" | "off"
                );
            if plain {
                text.clone()
            } else {
                Value::String(text.clone()).to_string()
            }
        }
        Value::Object(_) => "{}".to_owned(),
        Value::Array(_) => "[]".to_owned(),
    }
}

/// Merges this component's entries into the id map at `path`.
fn write_id_map(path: &Path, converted: &[Converted], document: &str) -> Result<(), ConvertError> {
    let mut cases: BTreeMap<String, Value> = BTreeMap::new();
    if path.is_file()
        && let Some(Value::Object(existing)) = veredictum::load::yaml_file_to_value(path)?
            .get("cases")
            .cloned()
    {
        cases.extend(existing);
    }
    let mut fresh: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for item in converted {
        let ids = fresh.entry(item.old_id.clone()).or_default();
        if let Some(Value::String(id)) = item.workflow.get("workflowId") {
            ids.push(Value::String(format!("{document}/{id}")));
        }
    }
    for (old, new) in fresh {
        if new.is_empty() {
            cases.insert(
                old,
                json!({ "new": [], "open": "the converter found no released operation to drive; adjudicate by hand" }),
            );
        } else {
            cases.insert(old, json!({ "new": new }));
        }
    }
    let mut text = String::from(
        "# The old-to-new case id map (#603): every case id of the case-core\n\
         # catalogue, and the Arazzo case ids (`<document>/<workflowId>`) that\n\
         # carry it. An entry with no new id names why in `open`.\n",
    );
    emit(&json!({ "cases": cases }), 0, &mut text);
    std::fs::write(path, text)?;
    Ok(())
}

#[expect(
    clippy::print_stdout,
    reason = "the converter's report to the operator is its standard output"
)]
#[expect(
    clippy::too_many_lines,
    reason = "the document is written top to bottom in one pass"
)]
fn main() -> Result<(), ConvertError> {
    let args = Args::parse();
    let loaded = load_root(&args.root)?;
    if !loaded.errors.is_empty() {
        return Err(ConvertError::Message(format!(
            "{} artifact(s) do not load",
            loaded.errors.len()
        )));
    }
    let set = loaded.set;
    let mut converter = Converter {
        set: &set,
        oas: OasIndex::load(&args.oas)?,
        prefix: args.prefix.clone(),
        document: args.document.clone(),
        sources: BTreeSet::new(),
        setups: BTreeMap::new(),
    };
    let mut cases: Vec<&CaseCore> = set
        .cases
        .iter()
        .map(|(_, case)| case)
        .filter(|case| case.id.as_str().starts_with(&args.prefix))
        .collect();
    cases.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
    let mut results = Vec::new();
    for case in cases {
        results.extend(converter.convert_case(case));
    }
    let folder = args.catalogue.join("cases").join(&args.capability);
    std::fs::create_dir_all(&folder)?;
    let target = folder.join(format!("{}.arazzo.yaml", args.document));
    if target.exists() {
        return Err(ConvertError::Message(format!(
            "{} exists; the converter never overwrites a document",
            target.display()
        )));
    }
    let mut text = format!(
        "# DRAFT converted from the case cores `{}*` by the one-off converter (#603).\n\
         # Every `UNCITED` citation and every `# UNCONVERTED:` note is completed by hand.\n",
        args.prefix
    );
    let sources: Vec<Value> = converter
        .sources
        .iter()
        .map(|name| {
            json!({ "name": name, "url": format!("../../../specs/rest-oas/{name}-validation.openapi.yaml"), "type": "openapi" })
        })
        .collect();
    emit(
        &json!({
            "arazzo": "1.1.0",
            "info": { "title": format!("Converted {}", args.prefix.trim_end_matches('.')), "version": "0.1.0" },
            "sourceDescriptions": sources,
        }),
        0,
        &mut text,
    );
    text.push_str("workflows:\n");
    for (setup, notes) in converter.setups.values() {
        for note in notes {
            let _ = writeln!(text, "  # UNCONVERTED: {note}");
        }
        text.push_str("  - ");
        emit_item(setup, 4, &mut text);
        text.push('\n');
    }
    for item in &results {
        let _ = writeln!(text, "  # from {}", item.old_id);
        for note in &item.notes {
            let _ = writeln!(text, "  # UNCONVERTED: {note}");
            println!("{}: UNCONVERTED: {note}", item.old_id);
        }
        if item.workflow.is_null() {
            let _ = writeln!(
                text,
                "  # UNCONVERTED: no step could be converted, so no workflow is written"
            );
            println!("{}: no workflow written", item.old_id);
            continue;
        }
        text.push_str("  - ");
        emit_item(&item.workflow, 4, &mut text);
        text.push('\n');
        if let (Some((iteration, rows)), Some(Value::String(id))) =
            (&item.rows, item.workflow.get("workflowId"))
        {
            let mut rows_text = format!(
                "workflow: {}/{id}\niteration: {}\nrows:\n",
                converter.document,
                match iteration {
                    Iteration::SinglePass => "single_pass",
                    Iteration::ResetPerRow => "reset_per_row",
                }
            );
            for (row, comment) in rows {
                if let Some(comment) = comment {
                    let _ = writeln!(rows_text, "  # {comment}");
                }
                rows_text.push_str("  - ");
                emit_item(row, 4, &mut rows_text);
            }
            std::fs::write(folder.join(format!("{id}.rows.yaml")), rows_text)?;
        }
    }
    std::fs::write(&target, text)?;
    write_id_map(
        &args.catalogue.join("id-map.yaml"),
        &results,
        &args.document,
    )?;
    println!(
        "wrote {} workflow(s) and {} setup workflow(s) to {}",
        results.len(),
        converter.setups.len(),
        target.display()
    );
    Ok(())
}
