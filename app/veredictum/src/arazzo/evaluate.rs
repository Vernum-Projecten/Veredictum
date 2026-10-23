// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The evaluator: drives one workflow's steps over a [`Transport`] and
//! classifies the run as passed, failed, errored or skipped, with the
//! four-part explanation a red row carries.
//!
//! A criterion that does not hold is *failed* and counts against the server.
//! Anything that stops the instrument from judging (a transport failure, an
//! unreadable response, an expression with no value, a construct the gate
//! should have refused) is *errored* and never counts against the server.
//! No openEHR spec governs this classification — our own design.

use std::collections::BTreeMap;
use std::fmt;

use roas_arazzo::common::reusable::ReusableOr;
use roas_arazzo::v1_1::{ParameterLocation, Step, ValueOrSelector, Workflow};
use serde_json::Value;

use crate::arazzo::cite::{Citation, citation_of};
use crate::arazzo::document::{ArazzoDocument, CriterionPlan};
use crate::arazzo::expr::{
    EvalError, Expr, Judgement, RequestView, ResponseView, Scope, value_text,
};
use crate::exec::transport::Transport;

/// What the evaluator needs to reach a server under test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunConfig {
    /// The base URL every OAS path is appended to.
    pub base_url: String,
    /// The `Authorization` header value sent on every request, if any.
    pub authorization: Option<String>,
}

/// The four statuses a row ends in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowStatus {
    /// Every criterion held.
    Passed,
    /// A criterion did not hold: the server must change.
    Failed,
    /// The instrument could not judge; this never counts against the server.
    Errored,
    /// The row was not run, for the reason given.
    Skipped(SkipReason),
}

impl fmt::Display for RowStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Passed => f.write_str("PASSED"),
            Self::Failed => f.write_str("FAILED"),
            Self::Errored => f.write_str("ERRORED"),
            Self::Skipped(reason) => write!(f, "SKIPPED ({reason})"),
        }
    }
}

/// Why a row was not run: the closed list a skipped row carries one of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The statement does not claim the case's capability.
    NotClaimed,
    /// The statement selects the other branch of a register option.
    OptionNotSelected,
    /// The case applies to spec versions the statement does not declare.
    VersionOutOfRange,
    /// The case needs a format outside the claimed tech profile.
    FormatNotClaimed,
    /// The case drives an extension route the vendor does not declare.
    ExtensionNotOffered,
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotClaimed => "not-claimed",
            Self::OptionNotSelected => "option-not-selected",
            Self::VersionOutOfRange => "version-out-of-range",
            Self::FormatNotClaimed => "format-not-claimed",
            Self::ExtensionNotOffered => "extension-not-offered",
        })
    }
}

/// Whose problem a red row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Whose {
    /// The server: a failed row counts against the claimed capability.
    Server,
    /// The instrument could not judge: the row does not count against the vendor.
    Instrument,
}

impl fmt::Display for Whose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Server => "the server: a failed row counts against the claimed capability",
            Self::Instrument => {
                "the instrument could not judge: this row does not count against the vendor"
            }
        })
    }
}

/// The four-part explanation a failed or errored row carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Explanation {
    /// The step the row went red at.
    pub step_id: String,
    /// What was checked: the step's description.
    pub what: String,
    /// Why it is required: the resolved citation.
    pub why: Option<Citation>,
    /// What the criterion expects.
    pub expected: String,
    /// What the exchange delivered, or why it could not be judged.
    pub received: String,
    /// The index into [`WorkflowRun::exchanges`] of the exchange judged.
    pub exchange: Option<usize>,
    /// Whose problem it is.
    pub whose: Whose,
}

/// One exchange as the run recorded it. The `Authorization` header is never
/// recorded.
#[derive(Debug, Clone, PartialEq)]
pub struct ExchangeRecord {
    /// The step that sent it.
    pub step_id: String,
    /// The request as sent.
    pub request: RequestView,
    /// The request body as sent.
    pub request_body: Option<Value>,
    /// The response status, headers and parsed body.
    pub response: ResponseView,
    /// The response body text as received.
    pub response_text: String,
}

/// One workflow run: its inputs, its exchanges and its classification.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowRun {
    /// The case id, `<document>/<workflowId>`.
    pub case_id: String,
    /// The inputs the run used, generated ones included, so a replay reuses them.
    pub inputs: BTreeMap<String, Value>,
    /// Every exchange, in order.
    pub exchanges: Vec<ExchangeRecord>,
    /// The classification.
    pub status: RowStatus,
    /// The explanation, on a failed or errored row.
    pub explanation: Option<Explanation>,
}

/// How many characters of a response body an explanation shows.
const EXCERPT_CHARS: usize = 240;

impl fmt::Display for WorkflowRun {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(why) = &self.explanation else {
            return write!(f, "{:<8} {}", self.status.to_string(), self.case_id);
        };
        writeln!(
            f,
            "{:<8} {}   step {}",
            self.status.to_string(),
            self.case_id,
            why.step_id
        )?;
        writeln!(f, "What     {}", why.what)?;
        match &why.why {
            Some(c) => writeln!(
                f,
                "Why      {}, {}:\n         \"{}\"",
                c.source, c.section, c.quote
            )?,
            None => writeln!(f, "Why      (no citation resolved)")?,
        }
        writeln!(f, "Expected {}", why.expected)?;
        writeln!(f, "Received {}", why.received)?;
        if let Some(record) = why.exchange.and_then(|i| self.exchanges.get(i)) {
            let excerpt: String = record.response_text.chars().take(EXCERPT_CHARS).collect();
            writeln!(
                f,
                "Exchange {} {} -> {}   (recording entry {})",
                record.request.method,
                record.request.url,
                record.response.status,
                why.exchange.map_or(0, |i| i.saturating_add(1))
            )?;
            writeln!(f, "         body: {excerpt}")?;
        }
        write!(f, "Whose    {}", why.whose)
    }
}

/// Why a workflow cannot be run at all.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// The document has no workflow of that id.
    #[error("the document has no workflow `{0}`")]
    UnknownWorkflow(String),
}

/// A red outcome inside a step, before it becomes an explanation.
#[derive(Debug)]
struct Red {
    status: RowStatus,
    why: Option<Citation>,
    expected: String,
    received: String,
    judged_exchange: bool,
}

impl Red {
    fn errored(
        why: Option<Citation>,
        expected: impl Into<String>,
        reason: impl fmt::Display,
    ) -> Self {
        Self {
            status: RowStatus::Errored,
            why,
            expected: expected.into(),
            received: format!("not judged: {reason}"),
            judged_exchange: false,
        }
    }
}

/// Runs one workflow of `document` and classifies it.
///
/// `supplied` carries input values; every declared input of `format: uuid`
/// without one is generated fresh. `document_name` is the case id prefix.
///
/// # Errors
/// [`RunError::UnknownWorkflow`] when the document has no such workflow.
/// Everything else becomes the run's classification.
pub fn run_workflow(
    document: &ArazzoDocument,
    document_name: &str,
    workflow_id: &str,
    supplied: &BTreeMap<String, Value>,
    config: &RunConfig,
    transport: &mut dyn Transport,
) -> Result<WorkflowRun, RunError> {
    let workflow = document
        .workflow(workflow_id)
        .ok_or_else(|| RunError::UnknownWorkflow(workflow_id.to_owned()))?;
    let mut run = WorkflowRun {
        case_id: format!("{document_name}/{workflow_id}"),
        inputs: inputs_for(workflow, supplied),
        exchanges: Vec::new(),
        status: RowStatus::Passed,
        explanation: None,
    };
    let workflow_cite = gated_citation(workflow.extensions.as_ref());
    let mut outputs: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
    for step in &workflow.steps {
        let step_cite = gated_citation(step.extensions.as_ref()).or_else(|| workflow_cite.clone());
        let driven = drive_step(
            document,
            step,
            step_cite.as_ref(),
            config,
            transport,
            &mut run,
            &outputs,
        );
        match driven {
            Ok(captured) => {
                outputs.insert(step.step_id.clone(), captured);
            }
            Err(red) => {
                run.status = red.status;
                run.explanation = Some(Explanation {
                    step_id: step.step_id.clone(),
                    what: step
                        .description
                        .clone()
                        .unwrap_or_else(|| step.step_id.clone()),
                    why: red.why,
                    expected: red.expected,
                    received: red.received,
                    exchange: red
                        .judged_exchange
                        .then(|| run.exchanges.len().checked_sub(1))
                        .flatten(),
                    whose: match red.status {
                        RowStatus::Failed => Whose::Server,
                        _ => Whose::Instrument,
                    },
                });
                break;
            }
        }
    }
    Ok(run)
}

/// The supplied inputs, plus a fresh UUID for every declared `format: uuid`
/// input without a value.
fn inputs_for(workflow: &Workflow, supplied: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
    let mut inputs = supplied.clone();
    let properties = workflow
        .inputs
        .as_ref()
        .and_then(|schema| schema.get("properties"))
        .and_then(Value::as_object);
    for (name, schema) in properties.into_iter().flatten() {
        let is_uuid = schema.get("format").and_then(Value::as_str) == Some("uuid");
        if is_uuid && !inputs.contains_key(name) {
            inputs.insert(
                name.clone(),
                Value::String(uuid::Uuid::new_v4().to_string()),
            );
        }
    }
    inputs
}

/// One composed request, ready to send.
struct Composed {
    request: RequestView,
    method: crate::vocab::HttpMethod,
    body: Option<Value>,
    body_is_json: bool,
}

/// Drives one step: composes and sends its request, judges its criteria and
/// returns its captured outputs.
fn drive_step(
    document: &ArazzoDocument,
    step: &Step,
    step_cite: Option<&Citation>,
    config: &RunConfig,
    transport: &mut dyn Transport,
    run: &mut WorkflowRun,
    outputs: &BTreeMap<String, BTreeMap<String, Value>>,
) -> Result<BTreeMap<String, Value>, Box<Red>> {
    let errored = |reason: &dyn fmt::Display| {
        Box::new(Red::errored(
            step_cite.cloned(),
            "the step to be driven",
            reason,
        ))
    };
    let scope = Scope {
        inputs: &run.inputs,
        outputs,
        exchange: None,
    };
    let composed = compose(document, step, config, &scope).map_err(|reason| errored(&reason))?;
    let mut sent_headers = composed.request.headers.clone();
    if let Some(authorization) = &config.authorization {
        sent_headers.insert("authorization".to_owned(), authorization.clone());
    }
    let raw = transport
        .exchange(
            composed.method,
            &composed.request.url,
            &sent_headers,
            composed.body.as_ref(),
            composed.body_is_json,
        )
        .map_err(|reason| errored(&reason))?;
    let parsed = if raw.text.trim().is_empty() {
        Ok(None)
    } else {
        serde_json::from_str::<Value>(&raw.text)
            .map(Some)
            .map_err(|error| error.to_string())
    };
    run.exchanges.push(ExchangeRecord {
        step_id: step.step_id.clone(),
        request: composed.request,
        request_body: composed.body,
        response: ResponseView {
            status: raw.status.as_u16(),
            headers: raw
                .headers
                .iter()
                .map(|(name, value)| (name.to_ascii_lowercase(), value.clone()))
                .collect(),
            body: parsed,
        },
        response_text: raw.text,
    });
    let Some(record) = run.exchanges.last() else {
        return Err(errored(&"the exchange was not recorded"));
    };
    let scope = Scope {
        inputs: &run.inputs,
        outputs,
        exchange: Some((&record.request, &record.response)),
    };
    judge_criteria(step, step_cite, &scope)?;
    let mut captured = BTreeMap::new();
    for (name, value) in &step.outputs {
        let ValueOrSelector::Literal(Value::String(text)) = value else {
            return Err(errored(&format!(
                "output `{name}` is not a runtime expression"
            )));
        };
        let value = Expr::parse(text)
            .map_err(EvalError::from)
            .and_then(|expr| scope.eval(&expr))
            .map_err(|e| errored(&e))?
            .ok_or_else(|| {
                errored(&format!(
                    "output `{name}` ({text}) is absent from the response"
                ))
            })?;
        captured.insert(name.clone(), value);
    }
    Ok(captured)
}

/// Judges a step's criteria in order; the first that does not hold, or
/// cannot be judged, turns the row red.
fn judge_criteria(
    step: &Step,
    step_cite: Option<&Citation>,
    scope: &Scope<'_>,
) -> Result<(), Box<Red>> {
    for criterion in &step.success_criteria {
        let why = gated_citation(criterion.extensions.as_ref()).or_else(|| step_cite.cloned());
        let judged = match CriterionPlan::of(criterion) {
            Err(kind) => {
                return Err(Box::new(Red::errored(
                    why,
                    criterion.condition.clone(),
                    kind,
                )));
            }
            Ok(CriterionPlan::Simple(condition)) => condition.judge(scope, &criterion.condition),
            Ok(CriterionPlan::Regex(regex)) => regex.judge(scope),
        };
        match judged {
            Ok(Judgement { holds: true, .. }) => {}
            Ok(Judgement {
                holds: false,
                expected,
                received,
            }) => {
                return Err(Box::new(Red {
                    status: RowStatus::Failed,
                    why,
                    expected,
                    received,
                    judged_exchange: true,
                }));
            }
            Err(error) => {
                return Err(Box::new(Red {
                    judged_exchange: true,
                    ..Red::errored(why, criterion.condition.clone(), error)
                }));
            }
        }
    }
    Ok(())
}

/// Composes a step's request from its operation, parameters and body.
fn compose(
    document: &ArazzoDocument,
    step: &Step,
    config: &RunConfig,
    scope: &Scope<'_>,
) -> Result<Composed, String> {
    let Some(operation) = step
        .operation_id
        .as_deref()
        .and_then(|qualified| document.operation(qualified))
    else {
        return Err("its operationId does not resolve".to_owned());
    };
    let mut request = RequestView {
        method: format!("{:?}", operation.method).to_uppercase(),
        ..RequestView::default()
    };
    let mut query = Vec::new();
    for parameter in &step.parameters {
        let ReusableOr::Item(parameter) = parameter else {
            return Err("a reusable parameter".to_owned());
        };
        let value = resolve_value(&parameter.value, scope).map_err(|e| e.to_string())?;
        match parameter.in_ {
            Some(ParameterLocation::Path) => {
                request.path.insert(parameter.name.clone(), value);
            }
            Some(ParameterLocation::Query) => {
                query.push(format!(
                    "{}={}",
                    urlencoding::encode(&parameter.name),
                    urlencoding::encode(&value)
                ));
                request.query.insert(parameter.name.clone(), value);
            }
            Some(ParameterLocation::Header) => {
                request
                    .headers
                    .insert(parameter.name.to_ascii_lowercase(), value);
            }
            other => return Err(format!("parameter location {other:?}")),
        }
    }
    let mut path = operation.path.clone();
    for (name, value) in &request.path {
        path = path.replace(&format!("{{{name}}}"), &urlencoding::encode(value));
    }
    if path.contains('{') {
        return Err(format!("path `{path}` has an unbound parameter"));
    }
    request.url = format!("{}{path}", config.base_url.trim_end_matches('/'));
    if !query.is_empty() {
        request.url = format!("{}?{}", request.url, query.join("&"));
    }
    let mut body = None;
    let mut body_is_json = true;
    if let Some(request_body) = &step.request_body {
        let mut payload = request_body.payload.clone().unwrap_or(Value::Null);
        for replacement in &request_body.replacements {
            let value = match &replacement.value {
                ValueOrSelector::Literal(Value::String(text)) if text.starts_with('$') => {
                    Expr::parse(text)
                        .map_err(EvalError::from)
                        .and_then(|expr| {
                            scope
                                .eval(&expr)?
                                .ok_or(EvalError::MissingOutput(text.clone()))
                        })
                        .map_err(|e| e.to_string())?
                }
                ValueOrSelector::Literal(literal) => literal.clone(),
                ValueOrSelector::Selector(_) => return Err("a selector value".to_owned()),
            };
            let Some(slot) = payload.pointer_mut(&replacement.target) else {
                return Err(format!(
                    "replacement target `{}` is not in the payload",
                    replacement.target
                ));
            };
            *slot = value;
        }
        body_is_json = !payload.is_string();
        if let Some(content_type) = &request_body.content_type {
            request
                .headers
                .insert("content-type".to_owned(), content_type.clone());
        }
        body = Some(payload);
    }
    Ok(Composed {
        request,
        method: operation.method,
        body,
        body_is_json,
    })
}
/// The citation an extension map carries, read after the gate has run.
fn gated_citation(extensions: Option<&BTreeMap<String, Value>>) -> Option<Citation> {
    // NOTE: no openEHR spec governs this — our own design; `ArazzoDocument::check`
    // refuses a malformed citation before any run, so here an `Err` cannot occur.
    citation_of(extensions).ok().flatten()
}

/// A parameter value as the text it is sent as.
fn resolve_value(value: &ValueOrSelector, scope: &Scope<'_>) -> Result<String, EvalError> {
    match value {
        ValueOrSelector::Literal(Value::String(text)) if text.starts_with('$') => {
            let expr = Expr::parse(text)?;
            let value = scope
                .eval(&expr)?
                .ok_or_else(|| EvalError::MissingOutput(text.clone()))?;
            Ok(value_text(&value))
        }
        ValueOrSelector::Literal(Value::String(text)) => scope.substitute(text, str::to_owned),
        ValueOrSelector::Literal(other) => Ok(other.to_string()),
        ValueOrSelector::Selector(_) => Err(EvalError::Parse(
            crate::arazzo::expr::ParseError::Expression("a selector value".to_owned()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::transport::RawResponse;
    use crate::vocab::HttpMethod;
    use assert_fs::prelude::*;
    use reqwest::StatusCode;

    /// A transport that answers from a fixed queue and records what it was sent.
    struct Scripted {
        answers: Vec<(u16, &'static str)>,
        sent: Vec<(HttpMethod, String, BTreeMap<String, String>)>,
    }

    impl Transport for Scripted {
        fn exchange(
            &mut self,
            method: HttpMethod,
            url: &str,
            headers: &BTreeMap<String, String>,
            _body: Option<&Value>,
            _body_is_json: bool,
        ) -> Result<RawResponse, String> {
            self.sent.push((method, url.to_owned(), headers.clone()));
            if self.answers.is_empty() {
                return Err("connection refused".to_owned());
            }
            let (status, text) = self.answers.remove(0);
            Ok(RawResponse {
                status: StatusCode::from_u16(status).map_err(|e| e.to_string())?,
                headers: BTreeMap::new(),
                text: text.to_owned(),
            })
        }
    }

    const DOC: &str = "arazzo: 1.1.0
info: { title: t, version: '1' }
sourceDescriptions:
  - { name: ehr, url: ehr.yaml, type: openapi }
workflows:
  - workflowId: twice
    x-veredictum-cite: { source: s, section: d, quote: q }
    inputs: { type: object, properties: { ehr_id: { type: string, format: uuid } } }
    steps:
      - stepId: create
        description: Create it.
        operationId: $sourceDescriptions.ehr.ehr_create_with_id
        parameters: [{ name: ehr_id, in: path, value: $inputs.ehr_id }]
        successCriteria: [{ condition: $statusCode == 201 }]
      - stepId: create-again
        description: Create it again.
        operationId: $sourceDescriptions.ehr.ehr_create_with_id
        parameters: [{ name: ehr_id, in: path, value: $inputs.ehr_id }]
        successCriteria: [{ condition: $statusCode == 409 }]
      - stepId: read-back
        description: Read it back.
        operationId: $sourceDescriptions.ehr.ehr_get_by_id
        parameters: [{ name: ehr_id, in: path, value: $inputs.ehr_id }]
        successCriteria:
          - condition: $statusCode == 200
          - { context: $response.body#/ehr_id/value, type: regex, condition: '^{$inputs.ehr_id}$' }
";
    const OAS: &str = "openapi: 3.0.3
paths:
  /ehr/{ehr_id}:
    put: { operationId: ehr_create_with_id }
    get: { operationId: ehr_get_by_id }
";

    fn drive(answers: Vec<(u16, &'static str)>) -> (WorkflowRun, Scripted) {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("ehr.yaml").write_str(OAS).unwrap();
        dir.child("doc.yaml").write_str(DOC).unwrap();
        let document = ArazzoDocument::load(&dir.path().join("doc.yaml")).unwrap();
        let mut transport = Scripted {
            answers,
            sent: Vec::new(),
        };
        let config = RunConfig {
            base_url: "http://sut/v1/".to_owned(),
            authorization: Some("Basic eDp5".to_owned()),
        };
        let supplied = BTreeMap::from([("ehr_id".to_owned(), Value::from("abc"))]);
        let run = run_workflow(
            &document,
            "ehr",
            "twice",
            &supplied,
            &config,
            &mut transport,
        )
        .unwrap();
        (run, transport)
    }

    #[test]
    fn a_conformant_exchange_passes() {
        let (run, transport) = drive(vec![
            (201, ""),
            (409, ""),
            (200, r#"{"ehr_id":{"value":"abc"}}"#),
        ]);
        assert_eq!(run.status, RowStatus::Passed);
        assert_eq!(run.explanation, None);
        let (method, url, headers) = transport.sent.first().unwrap();
        assert_eq!(*method, HttpMethod::Put);
        assert_eq!(url, "http://sut/v1/ehr/abc");
        assert_eq!(
            headers.get("authorization").map(String::as_str),
            Some("Basic eDp5")
        );
        assert!(
            run.exchanges
                .iter()
                .all(|e| !e.request.headers.contains_key("authorization")),
            "the credential is never recorded"
        );
    }

    #[test]
    fn a_criterion_that_does_not_hold_fails_against_the_server() {
        let (run, _) = drive(vec![(201, ""), (201, "")]);
        assert_eq!(run.status, RowStatus::Failed);
        let why = run.explanation.as_ref().unwrap();
        assert_eq!(why.step_id, "create-again");
        assert_eq!(why.what, "Create it again.");
        assert_eq!(why.expected, "status 409");
        assert_eq!(why.received, "status 201");
        assert_eq!(why.whose, Whose::Server);
        assert_eq!(why.exchange, Some(1));
        assert_eq!(why.why.as_ref().map(|c| c.quote.as_str()), Some("q"));
        let text = run.to_string();
        assert!(
            text.starts_with("FAILED   ehr/twice   step create-again"),
            "{text}"
        );
    }

    #[test]
    fn a_transport_failure_or_unreadable_body_is_errored_never_failed() {
        let (run, _) = drive(vec![(201, "")]);
        assert_eq!(run.status, RowStatus::Errored);
        assert_eq!(
            run.explanation.as_ref().map(|e| e.whose),
            Some(Whose::Instrument)
        );

        let (run, _) = drive(vec![(201, ""), (409, ""), (200, "<ehr/>")]);
        assert_eq!(run.status, RowStatus::Errored);
        assert_eq!(
            run.explanation.as_ref().map(|e| e.step_id.as_str()),
            Some("read-back")
        );
    }

    #[test]
    fn a_wrong_identifier_in_the_read_back_fails() {
        let (run, _) = drive(vec![
            (201, ""),
            (409, ""),
            (200, r#"{"ehr_id":{"value":"ABC"}}"#),
        ]);
        assert_eq!(run.status, RowStatus::Failed);
        assert_eq!(
            run.explanation.map(|e| e.received),
            Some("$response.body#/ehr_id/value is \"ABC\"".to_owned())
        );
    }

    #[test]
    fn an_undeclared_uuid_input_is_generated() {
        let workflow: Workflow = serde_json::from_value(serde_json::json!({
            "workflowId": "w",
            "inputs": { "type": "object", "properties": {
                "a": { "type": "string", "format": "uuid" },
                "b": { "type": "string" } } },
            "steps": []
        }))
        .unwrap();
        let inputs = inputs_for(&workflow, &BTreeMap::new());
        assert!(uuid::Uuid::parse_str(inputs.get("a").and_then(Value::as_str).unwrap()).is_ok());
        assert_eq!(inputs.get("b"), None);
    }
}
