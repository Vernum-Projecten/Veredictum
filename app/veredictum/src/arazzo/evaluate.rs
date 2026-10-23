// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The evaluator: drives every workflow of one document and classifies each
//! row.
//!
//! It sends each step over a [`Transport`], records each exchange as HAR 1.2,
//! judges each criterion under the condition profile, and classifies each row
//! as passed, failed, errored or skipped, a red row carrying its four-part
//! explanation.
//!
//! - A criterion that does not hold is *failed* and counts against the
//!   server. A response is validated against its operation's OAS schema only
//!   where a criterion asks for it through the `schema` judge.
//! - Anything that stops the instrument from judging is *errored* and never
//!   counts against the server: a transport failure, a body it cannot read, an
//!   expression with no value, an output a later step needs that was never
//!   captured, a judge that cannot decide, a setup workflow that did not pass,
//!   and every evaluation error Arazzo 1.1.0 §Evaluation Errors would turn
//!   into *fail*.
//! - A row the caller's [`Plan`] skips is *skipped* with one reason from the
//!   closed list and its citation.
//!
//! A workflow runs once per row of its rows file, or once with no rows file.
//! Its `dependsOn` workflows run before it: once per row under
//! `reset_per_row`, once for all rows under `single_pass`. Every
//! `format: uuid` input a row does not supply is a fresh UUID, recorded in the
//! HAR page of the run and in the results, so [`rejudge`] reuses it.
//! A step may send any header, `Accept`, `Content-Type` and `Authorization`
//! included, and a header the step sets wins over the run's credential and
//! over the request body's `contentType`.
//! No openEHR spec governs this classification — our own design.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use roas_arazzo::common::reusable::ReusableOr;
use roas_arazzo::v1_1::{ParameterLocation, Step, ValueOrSelector, Workflow};
use serde_json::Value;

use crate::arazzo::check::JudgeEnv;
use crate::arazzo::cite::{Citation, citation_of};
use crate::arazzo::document::{ArazzoDocument, CriterionPlan};
use crate::arazzo::expr::{
    EvalError, Expr, Judgement, RequestView, ResponseView, Scope, value_text,
};
use crate::arazzo::gates::rows::{InputShape, RowsFile};
use crate::arazzo::har::{self, Har, HarTransport, PageContext, Recorded, RunRole};
use crate::arazzo::oas::{OasError, ResponseSchemas};
use crate::arazzo::results::{
    Engine, ExchangeRef, Explanation, ResultRow, Results, Skip, Status, Whose, Why,
};
use crate::arazzo::rows::Row;
use crate::exec::signature::SigningMode;
use crate::exec::transport::Transport;
use crate::vocab::{HttpMethod, Iteration};

/// What the evaluator needs to reach a server under test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunConfig {
    /// The base URL every OAS path is appended to.
    pub base_url: String,
    /// The `Authorization` header value sent on every request that does not
    /// set its own, if any. It is never recorded.
    pub authorization: Option<String>,
    /// The deployment's declared signing posture, which the `signature`
    /// judge's `verifiable` fact reads.
    pub signing: Option<SigningMode>,
}

/// What the caller decides about the document's workflows before a run.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// The rows file of each data-driven workflow, by `workflowId`.
    pub rows: BTreeMap<String, RowsFile>,
    /// The workflows not run, by `workflowId`, with the reason and citation.
    pub skips: BTreeMap<String, Skip>,
}

/// Why a document cannot be run at all.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// `dependsOn` names a workflow the document does not have.
    #[error("the document has no workflow `{0}`")]
    UnknownWorkflow(String),
    /// `dependsOn` loops back to a workflow already being set up.
    #[error("`dependsOn` of `{0}` forms a cycle")]
    DependencyCycle(String),
    /// A source's schemas do not load.
    #[error(transparent)]
    Oas(#[from] OasError),
    /// A re-judgement does not line up with the recording.
    #[error("the replay diverged from the recording: {0}")]
    Replay(String),
}

/// A red outcome inside a step, before it becomes an explanation.
#[derive(Debug)]
struct Red {
    status: Status,
    why: Option<Citation>,
    expected: String,
    received: String,
    entry: Option<usize>,
}

impl Red {
    fn errored(
        why: Option<Citation>,
        expected: impl Into<String>,
        reason: impl fmt::Display,
    ) -> Box<Self> {
        Box::new(Self {
            status: Status::Errored,
            why,
            expected: expected.into(),
            received: format!("not judged: {reason}"),
            entry: None,
        })
    }

    fn at(mut self: Box<Self>, entry: usize) -> Box<Self> {
        self.entry = Some(entry);
        self
    }
}

/// One workflow run's outcome.
#[derive(Debug)]
struct WorkflowOutcome {
    status: Status,
    explanation: Option<Explanation>,
    inputs: Row,
    generated: Vec<String>,
    outputs: BTreeMap<String, Value>,
}

/// The outputs of every workflow a run depends on, by `workflowId`.
type WorkflowOutputs = BTreeMap<String, BTreeMap<String, Value>>;

/// Drives one document and records it.
#[derive(Debug)]
pub struct Evaluator<'d> {
    document: &'d ArazzoDocument,
    name: String,
    config: RunConfig,
    repo_root: PathBuf,
    schemas: ResponseSchemas,
    har: Har,
    replay: Option<VecDeque<Row>>,
}

impl<'d> Evaluator<'d> {
    /// Prepares a live run of `document`, whose case ids are prefixed with
    /// `name`; `repo_root` is the root citations are relative to.
    ///
    /// # Errors
    /// [`RunError::Oas`] when a source's schemas do not load.
    pub fn new(
        document: &'d ArazzoDocument,
        name: &str,
        config: RunConfig,
        repo_root: &Path,
    ) -> Result<Self, RunError> {
        Ok(Self {
            document,
            name: name.to_owned(),
            har: Har::new(&config.base_url),
            schemas: ResponseSchemas::load(document, repo_root)?,
            config,
            repo_root: repo_root.to_owned(),
            replay: None,
        })
    }

    /// Runs every workflow of the document in document order.
    ///
    /// # Errors
    /// [`RunError`] when the document cannot be run at all; everything else
    /// becomes a row's classification.
    pub fn run(&mut self, plan: &Plan, transport: &mut dyn Transport) -> Result<Results, RunError> {
        let mut rows = Vec::new();
        for workflow in &self.document.description().workflows {
            rows.extend(self.run_case(workflow, plan, transport)?);
        }
        if let Some(left) = &self.replay
            && !left.is_empty()
        {
            return Err(RunError::Replay(format!(
                "{} recorded workflow run(s) were not replayed",
                left.len()
            )));
        }
        Ok(Results {
            document: self.name.clone(),
            engine: Engine {
                name: "veredictum".to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            base_url: self.config.base_url.clone(),
            rows,
        })
    }

    /// The recording of everything run so far.
    #[must_use]
    pub fn into_recording(self) -> Har {
        self.har
    }

    fn case_id(&self, workflow_id: &str) -> String {
        format!("{}/{workflow_id}", self.name)
    }

    fn run_case(
        &mut self,
        workflow: &Workflow,
        plan: &Plan,
        transport: &mut dyn Transport,
    ) -> Result<Vec<ResultRow>, RunError> {
        let case = self.case_id(&workflow.workflow_id);
        if let Some(skip) = plan.skips.get(&workflow.workflow_id) {
            return Ok(vec![ResultRow {
                case,
                row: 0,
                status: Status::Skipped,
                inputs: Row::new(),
                generated: Vec::new(),
                skip: Some(skip.clone()),
                explanation: None,
            }]);
        }
        let file = plan.rows.get(&workflow.workflow_id);
        let rows = file.map_or_else(|| vec![Row::new()], |f| f.rows.clone());
        let iteration = file.map_or(Iteration::ResetPerRow, |f| f.iteration);
        let mut shared = None;
        if iteration == Iteration::SinglePass {
            shared = Some(self.setup(workflow, 0, transport, &mut Vec::new())?);
        }
        let mut out = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            let setup = match &shared {
                Some(done) => done.clone(),
                None => self.setup(workflow, index, transport, &mut Vec::new())?,
            };
            let outcome = match setup {
                Ok(workflows) => {
                    self.run_workflow(workflow, row, index, RunRole::Row, &workflows, transport)?
                }
                Err(explanation) => WorkflowOutcome {
                    status: Status::Errored,
                    explanation: Some(explanation),
                    inputs: row.clone(),
                    generated: Vec::new(),
                    outputs: BTreeMap::new(),
                },
            };
            out.push(ResultRow {
                case: case.clone(),
                row: index,
                status: outcome.status,
                inputs: outcome.inputs,
                generated: outcome.generated,
                skip: None,
                explanation: outcome.explanation,
            });
        }
        Ok(out)
    }

    /// Runs every workflow `workflow` depends on, depth first; `Err` carries
    /// the explanation of the first one that did not pass.
    fn setup(
        &mut self,
        workflow: &Workflow,
        row: usize,
        transport: &mut dyn Transport,
        stack: &mut Vec<String>,
    ) -> Result<Result<WorkflowOutputs, Explanation>, RunError> {
        let mut outputs = WorkflowOutputs::new();
        for id in &workflow.depends_on {
            if stack.contains(id) || *id == workflow.workflow_id {
                return Err(RunError::DependencyCycle(id.clone()));
            }
            let document = self.document;
            let dependency = document
                .workflow(id)
                .ok_or_else(|| RunError::UnknownWorkflow(id.clone()))?;
            stack.push(workflow.workflow_id.clone());
            let nested = self.setup(dependency, row, transport, stack)?;
            stack.pop();
            let nested = match nested {
                Ok(nested) => nested,
                Err(explanation) => return Ok(Err(explanation)),
            };
            let outcome = self.run_workflow(
                dependency,
                &Row::new(),
                row,
                RunRole::Setup,
                &nested,
                transport,
            )?;
            if outcome.status != Status::Passed {
                return Ok(Err(setup_explanation(id, &outcome)));
            }
            outputs.extend(nested);
            outputs.insert(id.clone(), outcome.outputs);
        }
        Ok(Ok(outputs))
    }

    /// The inputs of one run: the recorded ones on a replay, else the
    /// supplied ones plus a fresh UUID for every `format: uuid` input left out.
    fn inputs_for(
        &mut self,
        workflow: &Workflow,
        supplied: &Row,
    ) -> Result<(Row, Vec<String>), RunError> {
        let names = InputShape::of(workflow).generated;
        let generated: Vec<String> = names
            .into_iter()
            .filter(|name| !supplied.contains_key(name))
            .collect();
        if let Some(queue) = &mut self.replay {
            let inputs = queue.pop_front().ok_or_else(|| {
                RunError::Replay(
                    "the recording holds fewer workflow runs than the replay".to_owned(),
                )
            })?;
            return Ok((inputs, generated));
        }
        let mut inputs = supplied.clone();
        for name in &generated {
            inputs.insert(
                name.clone(),
                Value::String(uuid::Uuid::new_v4().to_string()),
            );
        }
        Ok((inputs, generated))
    }

    fn run_workflow(
        &mut self,
        workflow: &Workflow,
        supplied: &Row,
        row: usize,
        role: RunRole,
        workflows: &WorkflowOutputs,
        transport: &mut dyn Transport,
    ) -> Result<WorkflowOutcome, RunError> {
        let (inputs, generated) = self.inputs_for(workflow, supplied)?;
        let page = self.har.begin_page(PageContext {
            case: self.case_id(&workflow.workflow_id),
            row,
            role,
            inputs: inputs.clone(),
        });
        let workflow_cite = gated_citation(workflow.extensions.as_ref());
        let mut outputs: WorkflowOutputs = BTreeMap::new();
        let mut red = None;
        for step in &workflow.steps {
            let step_cite =
                gated_citation(step.extensions.as_ref()).or_else(|| workflow_cite.clone());
            let context = StepContext {
                page: &page,
                step_cite: step_cite.as_ref(),
                inputs: &inputs,
                outputs: &outputs,
                workflows,
            };
            match self.drive_step(step, &context, transport) {
                Ok(captured) => {
                    outputs.insert(step.step_id.clone(), captured);
                }
                Err(found) => {
                    red = Some(self.explain(step, *found));
                    break;
                }
            }
        }
        let mut captured = BTreeMap::new();
        if red.is_none() {
            let scope = Scope {
                inputs: &inputs,
                outputs: &outputs,
                workflows,
                exchange: None,
            };
            for (name, value) in &workflow.outputs {
                match output_value(value, &scope) {
                    Ok(Some(value)) => {
                        captured.insert(name.clone(), value);
                    }
                    Ok(None) => {}
                    Err(reason) => {
                        red = Some(Explanation {
                            step: "outputs".to_owned(),
                            what: format!("Capture the workflow output `{name}`."),
                            why: workflow_cite.as_ref().map(|c| Why::of(c, &self.repo_root)),
                            expected: format!("the output `{name}` evaluates"),
                            received: format!("not judged: {reason}"),
                            exchange: None,
                            whose: Whose::Instrument,
                        });
                        break;
                    }
                }
            }
        }
        Ok(WorkflowOutcome {
            status: match &red {
                None => Status::Passed,
                Some(e) if e.whose == Whose::Server => Status::Failed,
                Some(_) => Status::Errored,
            },
            explanation: red,
            inputs,
            generated,
            outputs: captured,
        })
    }

    fn explain(&self, step: &Step, red: Red) -> Explanation {
        let exchange = red.entry.and_then(|entry| {
            self.har.log.entries.get(entry).map(|e| ExchangeRef {
                entry,
                method: e.request.method.clone(),
                url: e.request.url.clone(),
                status: e.response.status,
            })
        });
        Explanation {
            step: step.step_id.clone(),
            what: step
                .description
                .clone()
                .unwrap_or_else(|| step.step_id.clone()),
            why: red.why.as_ref().map(|c| Why::of(c, &self.repo_root)),
            expected: red.expected,
            received: red.received,
            exchange,
            whose: match red.status {
                Status::Failed => Whose::Server,
                _ => Whose::Instrument,
            },
        }
    }

    /// Drives one step: composes and sends its request, records it, judges
    /// its criteria and its response schema, and returns its captured outputs.
    fn drive_step(
        &mut self,
        step: &Step,
        context: &StepContext<'_>,
        transport: &mut dyn Transport,
    ) -> Result<BTreeMap<String, Value>, Box<Red>> {
        let cite = context.step_cite;
        let errored = |reason: &dyn fmt::Display| {
            Red::errored(cite.cloned(), "the step to be driven", reason)
        };
        let scope = Scope {
            inputs: context.inputs,
            outputs: context.outputs,
            workflows: context.workflows,
            exchange: None,
        };
        let composed =
            compose(self.document, step, &self.config, &scope).map_err(|r| errored(&r))?;
        let body_text = composed
            .body
            .as_ref()
            .map(|body| har::body_text(body, composed.body_is_json));
        let started = Instant::now();
        let outcome = transport.exchange(
            composed.method,
            &composed.request.url,
            &composed.sent_headers,
            composed.body.as_ref(),
            composed.body_is_json,
        );
        let millis = started.elapsed().as_secs_f64() * 1000.0;
        let entry = self.har.record(&Recorded {
            page: context.page,
            step: &step.step_id,
            method: &composed.request.method,
            url: &composed.request.url,
            request_headers: &composed.sent_headers,
            query: &composed.request.query,
            body: body_text.as_deref(),
            outcome: outcome.as_ref().map_err(String::as_str),
            millis,
        });
        let raw = outcome.map_err(|reason| errored(&reason).at(entry))?;
        let response = ResponseView {
            status: raw.status.as_u16(),
            headers: raw
                .headers
                .iter()
                .map(|(name, value)| (name.to_ascii_lowercase(), value.clone()))
                .collect(),
            body: if raw.text.trim().is_empty() {
                Ok(None)
            } else {
                serde_json::from_str::<Value>(&raw.text)
                    .map(Some)
                    .map_err(|error| error.to_string())
            },
        };
        let scope = Scope {
            exchange: Some((&composed.request, &response)),
            ..scope
        };
        self.judge_criteria(step, cite, &scope)
            .map_err(|red| red.at(entry))?;
        let mut captured = BTreeMap::new();
        for (name, value) in &step.outputs {
            if let Some(value) = output_value(value, &scope).map_err(|e| errored(&e).at(entry))? {
                captured.insert(name.clone(), value);
            }
        }
        Ok(captured)
    }

    /// Judges a step's criteria in order; the first that does not hold, or
    /// cannot be judged, turns the row red.
    fn judge_criteria(
        &mut self,
        step: &Step,
        cite: Option<&Citation>,
        scope: &Scope<'_>,
    ) -> Result<(), Box<Red>> {
        let mut env = JudgeEnv {
            signing: self.config.signing.as_ref(),
            schemas: Some(&mut self.schemas),
            operation: step.operation_id.as_deref(),
        };
        for criterion in &step.success_criteria {
            let why = gated_citation(criterion.extensions.as_ref()).or_else(|| cite.cloned());
            let plan = match CriterionPlan::of(criterion) {
                Ok(plan) => plan,
                Err(kind) => return Err(Red::errored(why, criterion.condition.clone(), kind)),
            };
            match judge(&plan, scope, &criterion.condition, &mut env) {
                Ok(Judgement { holds: true, .. }) => {}
                Ok(Judgement {
                    holds: false,
                    expected,
                    received,
                }) => {
                    return Err(Box::new(Red {
                        status: Status::Failed,
                        why,
                        expected,
                        received,
                        entry: None,
                    }));
                }
                Err(error) => return Err(Red::errored(why, criterion.condition.clone(), error)),
            }
        }
        Ok(())
    }
}

/// What a step reads from its workflow run.
struct StepContext<'a> {
    page: &'a str,
    step_cite: Option<&'a Citation>,
    inputs: &'a Row,
    outputs: &'a WorkflowOutputs,
    workflows: &'a WorkflowOutputs,
}

/// The explanation a row carries when a workflow it depends on did not pass.
fn setup_explanation(id: &str, outcome: &WorkflowOutcome) -> Explanation {
    let inner = outcome.explanation.clone();
    let step = inner.as_ref().map_or_else(String::new, |e| e.step.clone());
    Explanation {
        step: format!("{id}/{step}"),
        what: format!("Set the row up with the workflow `{id}`."),
        why: inner.as_ref().and_then(|e| e.why.clone()),
        expected: inner
            .as_ref()
            .map_or_else(String::new, |e| e.expected.clone()),
        received: format!(
            "not judged: the setup workflow `{id}` ended {}: {}",
            outcome.status,
            inner.as_ref().map_or("", |e| e.received.as_str())
        ),
        exchange: inner.and_then(|e| e.exchange),
        whose: Whose::Instrument,
    }
}

/// Judges one planned criterion.
fn judge(
    plan: &CriterionPlan,
    scope: &Scope<'_>,
    condition: &str,
    env: &mut JudgeEnv<'_>,
) -> Result<Judgement, EvalError> {
    match plan {
        CriterionPlan::Simple(simple) => simple.judge(scope, condition),
        CriterionPlan::Regex(regex) => regex.judge(scope),
        CriterionPlan::JsonPath(query) => query.judge(scope),
        CriterionPlan::Checked(inner, check) => {
            let judged = judge(inner, scope, condition, env)?;
            if !judged.holds {
                return Ok(judged);
            }
            check.judge(scope, env)
        }
    }
}

/// An output's value: `None` when it is absent from a readable response.
fn output_value(value: &ValueOrSelector, scope: &Scope<'_>) -> Result<Option<Value>, EvalError> {
    let ValueOrSelector::Literal(Value::String(text)) = value else {
        return Err(EvalError::Parse(
            crate::arazzo::expr::ParseError::Expression(
                "an output that is not a runtime expression".to_owned(),
            ),
        ));
    };
    scope.eval(&Expr::parse(text)?)
}

/// One composed request, ready to send.
struct Composed {
    request: RequestView,
    sent_headers: BTreeMap<String, String>,
    method: HttpMethod,
    body: Option<Value>,
    body_is_json: bool,
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
                .entry("content-type".to_owned())
                .or_insert_with(|| content_type.clone());
        }
        body = Some(payload);
    }
    let mut sent_headers = request.headers.clone();
    if let Some(authorization) = &config.authorization {
        sent_headers
            .entry("authorization".to_owned())
            .or_insert_with(|| authorization.clone());
    }
    Ok(Composed {
        request,
        sent_headers,
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

/// Runs `document` live and returns its results and its recording.
///
/// # Errors
/// [`RunError`] when the document cannot be run at all.
pub fn run_document(
    document: &ArazzoDocument,
    name: &str,
    config: RunConfig,
    repo_root: &Path,
    plan: &Plan,
    transport: &mut dyn Transport,
) -> Result<(Results, Har), RunError> {
    let mut evaluator = Evaluator::new(document, name, config, repo_root)?;
    let results = evaluator.run(plan, transport)?;
    Ok((results, evaluator.into_recording()))
}

/// Re-judges a recording of `document` without a server.
///
/// Every request is answered from the recording and every input is the
/// recorded one, so the results are the bytes the recorded run wrote.
/// `signing` is the posture the recorded run was given.
///
/// # Errors
/// [`RunError::Replay`] when the replay does not line up with the
/// recording, and any other [`RunError`] a live run would return.
pub fn rejudge(
    document: &ArazzoDocument,
    name: &str,
    repo_root: &Path,
    signing: Option<SigningMode>,
    plan: &Plan,
    recording: &Har,
) -> Result<Results, RunError> {
    let config = RunConfig {
        base_url: recording.log.veredictum.base_url.clone(),
        authorization: None,
        signing,
    };
    let mut evaluator = Evaluator::new(document, name, config, repo_root)?;
    evaluator.replay = Some(
        recording
            .log
            .pages
            .iter()
            .map(|page| page.veredictum.inputs.clone())
            .collect(),
    );
    let mut transport = HarTransport::new(recording);
    let results = evaluator.run(plan, &mut transport)?;
    if transport.remaining() > 0 {
        return Err(RunError::Replay(format!(
            "{} recorded exchange(s) were not replayed",
            transport.remaining()
        )));
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arazzo::results::SkipReason;
    use crate::exec::transport::RawResponse;
    use assert_fs::prelude::*;
    use reqwest::StatusCode;

    /// A transport that answers from a fixed queue and records what it was sent.
    struct Scripted {
        answers: VecDeque<(u16, &'static str)>,
        sent: Vec<(HttpMethod, String, BTreeMap<String, String>)>,
    }

    impl Scripted {
        fn new(answers: &[(u16, &'static str)]) -> Self {
            Self {
                answers: answers.iter().copied().collect(),
                sent: Vec::new(),
            }
        }
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
            let Some((status, text)) = self.answers.pop_front() else {
                return Err("transport: connection refused".to_owned());
            };
            let mut headers = BTreeMap::new();
            if text.starts_with('{') {
                headers.insert("Content-Type".to_owned(), "application/json".to_owned());
            }
            Ok(RawResponse {
                status: StatusCode::from_u16(status).map_err(|e| e.to_string())?,
                headers,
                text: text.to_owned(),
            })
        }
    }

    const OAS: &str = "openapi: 3.0.3
paths:
  /ehr:
    post:
      operationId: ehr_create
      responses:
        '201':
          description: |
            `201 Created` is returned when the EHR has been created.
          content:
            application/json:
              schema: { $ref: '#/components/schemas/Ehr' }
  /ehr/{ehr_id}:
    put: { operationId: ehr_create_with_id }
    get:
      operationId: ehr_get_by_id
      responses:
        '200':
          description: The EHR.
          content:
            application/json:
              schema: { $ref: '#/components/schemas/Ehr' }
components:
  schemas:
    Ehr:
      type: object
      required: [ehr_id]
      properties:
        ehr_id: { type: object, required: [value], properties: { value: { type: string } } }
";

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
        parameters:
          - { name: ehr_id, in: path, value: $inputs.ehr_id }
          - { name: Authorization, in: header, value: Bearer step-token }
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

    fn load(doc: &str) -> (assert_fs::TempDir, ArazzoDocument) {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("ehr.yaml").write_str(OAS).unwrap();
        dir.child("doc.yaml").write_str(doc).unwrap();
        let document = ArazzoDocument::load(&dir.path().join("doc.yaml")).unwrap();
        (dir, document)
    }

    fn config() -> RunConfig {
        RunConfig {
            base_url: "http://sut/v1/".to_owned(),
            authorization: Some("Basic eDp5".to_owned()),
            signing: None,
        }
    }

    fn plan_with(workflow: &str, rows: &[Row], iteration: Iteration) -> Plan {
        Plan {
            rows: BTreeMap::from([(
                workflow.to_owned(),
                RowsFile {
                    workflow: format!("ehr/{workflow}"),
                    iteration,
                    rows: rows.to_vec(),
                },
            )]),
            skips: BTreeMap::new(),
        }
    }

    fn drive(answers: &[(u16, &'static str)]) -> (Results, Har, Scripted) {
        let (dir, document) = load(DOC);
        let mut transport = Scripted::new(answers);
        let row = Row::from([("ehr_id".to_owned(), Value::from("abc"))]);
        let plan = plan_with("twice", &[row], Iteration::ResetPerRow);
        let (results, har) = run_document(
            &document,
            "ehr",
            config(),
            dir.path(),
            &plan,
            &mut transport,
        )
        .unwrap();
        (results, har, transport)
    }

    fn only(results: &Results) -> &ResultRow {
        results.rows.first().unwrap()
    }

    #[test]
    fn a_conformant_exchange_passes_and_the_step_header_wins_over_the_credential() {
        let (results, har, transport) =
            drive(&[(201, ""), (409, ""), (200, r#"{"ehr_id":{"value":"abc"}}"#)]);
        assert_eq!(only(&results).status, Status::Passed);
        let (method, url, headers) = transport.sent.first().unwrap();
        assert_eq!(*method, HttpMethod::Put);
        assert_eq!(url, "http://sut/v1/ehr/abc");
        assert_eq!(
            headers.get("authorization").map(String::as_str),
            Some("Bearer step-token")
        );
        let (_, _, second) = transport.sent.get(1).unwrap();
        assert_eq!(
            second.get("authorization").map(String::as_str),
            Some("Basic eDp5")
        );
        let text = har.to_text().unwrap();
        assert!(
            !text.contains("eDp5") && !text.contains("step-token"),
            "{text}"
        );
    }

    #[test]
    fn a_criterion_that_does_not_hold_fails_against_the_server() {
        let (results, _, _) = drive(&[(201, ""), (201, "")]);
        let row = only(&results);
        assert_eq!(row.status, Status::Failed);
        let why = row.explanation.as_ref().unwrap();
        assert_eq!(why.step, "create-again");
        assert_eq!(why.what, "Create it again.");
        assert_eq!(why.expected, "status 409");
        assert_eq!(why.received, "status 201");
        assert_eq!(why.whose, Whose::Server);
        assert_eq!(why.exchange.as_ref().map(|e| e.entry), Some(1));
        assert_eq!(why.why.as_ref().map(|c| c.quote.as_str()), Some("q"));
    }

    #[test]
    fn a_transport_failure_or_unreadable_body_is_errored_never_failed() {
        let (results, har, _) = drive(&[(201, "")]);
        let row = only(&results);
        assert_eq!(row.status, Status::Errored);
        assert_eq!(
            row.explanation.as_ref().map(|e| e.whose),
            Some(Whose::Instrument)
        );
        assert_eq!(
            har.log.entries.len(),
            2,
            "the failed exchange is recorded too"
        );

        let (results, _, _) = drive(&[(201, ""), (409, ""), (200, "<ehr/>")]);
        let row = only(&results);
        assert_eq!(row.status, Status::Errored);
        assert_eq!(
            row.explanation.as_ref().map(|e| e.step.as_str()),
            Some("read-back")
        );
    }

    #[test]
    fn an_absent_value_in_a_readable_response_fails() {
        let (results, _, _) =
            drive(&[(201, ""), (409, ""), (200, r#"{"ehr_id":{"value":"ABC"}}"#)]);
        let row = only(&results);
        assert_eq!(row.status, Status::Failed);
        assert_eq!(
            row.explanation.as_ref().map(|e| e.received.as_str()),
            Some("$response.body#/ehr_id/value is \"ABC\"")
        );
    }

    fn run_read_back(doc: &str, body: &'static str) -> ResultRow {
        let (dir, document) = load(doc);
        let mut transport = Scripted::new(&[(201, ""), (409, ""), (200, body)]);
        let (results, _) = run_document(
            &document,
            "ehr",
            config(),
            dir.path(),
            &Plan::default(),
            &mut transport,
        )
        .unwrap();
        results.rows.into_iter().next().unwrap()
    }

    const PRESENT: &str = "- { condition: $response.body#/ehr_id != null }";
    const REGEX: &str =
        "- { context: $response.body#/ehr_id/value, type: regex, condition: '^{$inputs.ehr_id}$' }";
    const OFF_SCHEMA: &str = r#"{"ehr_id":{"id":"abc"}}"#;

    #[test]
    fn a_response_is_not_validated_against_its_schema_unless_a_criterion_asks() {
        let doc = DOC.replace(REGEX, PRESENT);
        assert_eq!(run_read_back(&doc, OFF_SCHEMA).status, Status::Passed);
    }

    #[test]
    fn the_schema_judge_fails_a_body_outside_the_declared_schema() {
        let doc = DOC.replace(
            REGEX,
            "- { condition: $statusCode == 200, x-veredictum-check: { judge: schema, against: oas } }",
        );
        assert_eq!(
            run_read_back(&doc, r#"{"ehr_id":{"value":"abc"},"x":1}"#).status,
            Status::Passed,
            "extra members are allowed"
        );
        let row = run_read_back(&doc, OFF_SCHEMA);
        assert_eq!(row.status, Status::Failed);
        let why = row.explanation.as_ref().unwrap();
        assert!(why.received.contains("value"), "{why:?}");
        assert!(why.expected.contains("ehr.yaml"), "{why:?}");
        assert_eq!(why.why.as_ref().map(|w| w.quote.as_str()), Some("q"));
    }

    #[test]
    fn the_schema_judge_errors_where_no_schema_is_declared_or_the_body_is_unreadable() {
        let doc = DOC.replace(
            "successCriteria: [{ condition: $statusCode == 409 }]",
            "successCriteria: [{ condition: $statusCode == 409, x-veredictum-check: { judge: schema, against: oas } }]",
        );
        let (dir, document) = load(&doc);
        let mut transport = Scripted::new(&[(201, ""), (409, r#"{"a":1}"#)]);
        let (results, _) = run_document(
            &document,
            "ehr",
            config(),
            dir.path(),
            &Plan::default(),
            &mut transport,
        )
        .unwrap();
        let row = only(&results);
        assert_eq!(row.status, Status::Errored, "{row}");
        let doc = DOC.replace(
            REGEX,
            "- { condition: $statusCode == 200, x-veredictum-check: { judge: schema, against: oas } }",
        );
        assert_eq!(run_read_back(&doc, "{not json").status, Status::Errored);
    }

    #[test]
    fn a_missing_capture_is_errored_when_a_later_step_reads_it() {
        let doc = "arazzo: 1.1.0
info: { title: t, version: '1' }
sourceDescriptions:
  - { name: ehr, url: ehr.yaml, type: openapi }
workflows:
  - workflowId: chain
    x-veredictum-cite: { source: s, section: d, quote: q }
    steps:
      - stepId: create
        operationId: $sourceDescriptions.ehr.ehr_create
        successCriteria: [{ condition: $statusCode == 201 }]
        outputs: { ehr_id: $response.body#/ehr_id/value }
      - stepId: get
        operationId: $sourceDescriptions.ehr.ehr_get_by_id
        parameters: [{ name: ehr_id, in: path, value: $steps.create.outputs.ehr_id }]
        successCriteria: [{ condition: $statusCode == 200 }]
";
        let (dir, document) = load(doc);
        let mut transport = Scripted::new(&[(201, "")]);
        let (results, _) = run_document(
            &document,
            "ehr",
            config(),
            dir.path(),
            &Plan::default(),
            &mut transport,
        )
        .unwrap();
        let row = only(&results);
        assert_eq!(row.status, Status::Errored);
        assert_eq!(
            row.explanation.as_ref().map(|e| e.step.as_str()),
            Some("get")
        );
        assert_eq!(transport.sent.len(), 1, "the unbound request is never sent");
    }

    #[test]
    fn an_xpath_criterion_is_errored_never_failed() {
        let doc = DOC.replace(
            "successCriteria: [{ condition: $statusCode == 201 }]",
            "successCriteria: [{ context: $response.body, type: xpath, condition: /ehr }]",
        );
        let (dir, document) = load(&doc);
        let mut transport = Scripted::new(&[(201, "")]);
        let (results, _) = run_document(
            &document,
            "ehr",
            config(),
            dir.path(),
            &Plan::default(),
            &mut transport,
        )
        .unwrap();
        let row = only(&results);
        assert_eq!(row.status, Status::Errored);
        assert!(
            row.explanation.as_ref().unwrap().received.contains("xpath"),
            "{row:?}"
        );
    }

    #[test]
    fn a_generated_uuid_is_recorded_in_the_results_and_the_recording() {
        let (dir, document) = load(DOC);
        let mut transport = Scripted::new(&[(201, "")]);
        let (results, har) = run_document(
            &document,
            "ehr",
            config(),
            dir.path(),
            &Plan::default(),
            &mut transport,
        )
        .unwrap();
        let row = only(&results);
        assert_eq!(row.generated, vec!["ehr_id".to_owned()]);
        let id = row.inputs.get("ehr_id").and_then(Value::as_str).unwrap();
        assert!(uuid::Uuid::parse_str(id).is_ok());
        assert_eq!(
            har.log.pages.first().map(|p| &p.veredictum.inputs),
            Some(&row.inputs)
        );
    }

    const SETUP_DOC: &str = "arazzo: 1.1.0
info: { title: t, version: '1' }
sourceDescriptions:
  - { name: ehr, url: ehr.yaml, type: openapi }
workflows:
  - workflowId: setup
    x-veredictum-cite: { source: s, section: d, quote: q }
    steps:
      - stepId: create
        operationId: $sourceDescriptions.ehr.ehr_create
        successCriteria: [{ condition: $statusCode == 201 }]
        outputs: { ehr_id: $response.body#/ehr_id/value }
    outputs: { ehr_id: $steps.create.outputs.ehr_id }
  - workflowId: read
    x-veredictum-cite: { source: s, section: d, quote: q }
    dependsOn: [setup]
    inputs: { type: object, properties: { want: { type: integer } } }
    steps:
      - stepId: get
        operationId: $sourceDescriptions.ehr.ehr_get_by_id
        parameters: [{ name: ehr_id, in: path, value: $workflows.setup.outputs.ehr_id }]
        successCriteria: [{ condition: $statusCode == $inputs.want }]
";

    fn run_setup(iteration: Iteration, answers: &[(u16, &'static str)]) -> (Results, Scripted) {
        let (dir, document) = load(SETUP_DOC);
        let rows = [
            Row::from([("want".to_owned(), Value::from(200))]),
            Row::from([("want".to_owned(), Value::from(404))]),
        ];
        let mut plan = plan_with("read", &rows, iteration);
        plan.skips.insert(
            "setup".to_owned(),
            Skip {
                reason: SkipReason::NotClaimed,
                why: Why {
                    source: "s".to_owned(),
                    section: "d".to_owned(),
                    quote: "q".to_owned(),
                    link: None,
                    report: None,
                },
            },
        );
        let mut transport = Scripted::new(answers);
        let (results, _) = run_document(
            &document,
            "ehr",
            config(),
            dir.path(),
            &plan,
            &mut transport,
        )
        .unwrap();
        (results, transport)
    }

    const EHR: &str = r#"{"ehr_id":{"value":"e1"}}"#;

    #[test]
    fn reset_per_row_runs_the_setup_once_per_row() {
        let (results, transport) = run_setup(
            Iteration::ResetPerRow,
            &[(201, EHR), (200, EHR), (201, EHR), (404, "")],
        );
        let statuses: Vec<Status> = results.rows.iter().map(|r| r.status).collect();
        assert_eq!(
            statuses,
            vec![Status::Skipped, Status::Passed, Status::Passed]
        );
        assert_eq!(transport.sent.len(), 4);
        assert_eq!(
            transport.sent.get(1).map(|s| s.1.as_str()),
            Some("http://sut/v1/ehr/e1")
        );
        let skipped = results.rows.first().unwrap();
        assert_eq!(
            skipped.skip.as_ref().map(|s| s.reason),
            Some(SkipReason::NotClaimed)
        );
    }

    #[test]
    fn single_pass_runs_the_setup_once_for_all_rows() {
        let (results, transport) =
            run_setup(Iteration::SinglePass, &[(201, EHR), (200, EHR), (200, EHR)]);
        let statuses: Vec<Status> = results.rows.iter().map(|r| r.status).collect();
        assert_eq!(
            statuses,
            vec![Status::Skipped, Status::Passed, Status::Failed]
        );
        assert_eq!(transport.sent.len(), 3);
    }

    #[test]
    fn a_setup_that_does_not_pass_errors_the_row() {
        let (results, _) = run_setup(Iteration::ResetPerRow, &[(500, ""), (201, EHR), (404, "")]);
        let row = results.rows.get(1).unwrap();
        assert_eq!(row.status, Status::Errored);
        let why = row.explanation.as_ref().unwrap();
        assert_eq!(why.step, "setup/create");
        assert!(why.received.contains("ended FAILED"), "{why:?}");
        assert_eq!(results.rows.get(2).map(|r| r.status), Some(Status::Passed));
    }

    #[test]
    fn re_judging_the_recording_reproduces_the_bytes() {
        let cases: [&[(u16, &'static str)]; 3] = [
            &[(201, ""), (409, ""), (200, r#"{"ehr_id":{"value":"abc"}}"#)],
            &[(201, ""), (201, "")],
            &[(201, ""), (409, ""), (200, "<ehr/>")],
        ];
        for answers in cases {
            let (dir, document) = load(DOC);
            let mut transport = Scripted::new(answers);
            let (results, har) = run_document(
                &document,
                "ehr",
                config(),
                dir.path(),
                &Plan::default(),
                &mut transport,
            )
            .unwrap();
            let recorded = har.to_text().unwrap();
            let replayed = rejudge(
                &document,
                "ehr",
                dir.path(),
                None,
                &Plan::default(),
                &Har::from_text(&recorded).unwrap(),
            )
            .unwrap();
            assert_eq!(replayed.to_text().unwrap(), results.to_text().unwrap());
        }
        let (results, _) = run_setup(Iteration::ResetPerRow, &[(500, ""), (201, EHR), (404, "")]);
        assert_eq!(results.rows.len(), 3);
    }

    #[test]
    fn a_recording_of_another_document_does_not_replay() {
        let (dir, document) = load(DOC);
        let mut transport = Scripted::new(&[(201, "")]);
        let (_, har) = run_document(
            &document,
            "ehr",
            config(),
            dir.path(),
            &Plan::default(),
            &mut transport,
        )
        .unwrap();
        let (dir2, other) = load(SETUP_DOC);
        assert!(matches!(
            rejudge(&other, "ehr", dir2.path(), None, &Plan::default(), &har),
            Err(RunError::Replay(_))
        ));
        drop(dir);
    }
}
