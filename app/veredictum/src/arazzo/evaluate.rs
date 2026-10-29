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
//! - A criterion that does not hold under a `level: should` citation does
//!   not stop the row. When every criterion that fails in a row is a
//!   `should`, the row is *failed* and advisory: it carries its explanation
//!   and never counts against a capability.
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
use std::rc::Rc;
use std::time::Instant;

use roas_arazzo::common::reusable::ReusableOr;
use roas_arazzo::v1_1::{ParameterLocation, Step, ValueOrSelector, Workflow};
use serde_json::Value;

use crate::arazzo::check::JudgeEnv;
use crate::arazzo::cite::{Citation, Level, citation_of};
use crate::arazzo::derive::derivations_of;
use crate::arazzo::document::{ArazzoDocument, CriterionPlan};
use crate::arazzo::expr::{
    EvalError, Expr, Judgement, RequestView, ResponseView, Scope, value_text,
};
use crate::arazzo::gates::rows::{Aggregate, Declaration, InputShape, RowsFile, ServerOwnership};
use crate::arazzo::har::{self, Har, HarTransport, PageContext, Recorded, RunRole};
use crate::arazzo::oas::{OasError, ResponseSchemas};
use crate::arazzo::principal::{Principal, principal_of};
use crate::arazzo::results::{
    Engine, ExchangeRef, Explanation, ResultRow, Results, Skip, SkipReason, Status, Whose, Why,
};
use crate::arazzo::rows::Row;
use crate::arazzo::scope::{self, Coverage, scope_of};
use crate::arazzo::source::{Base, split_qualified};
use crate::exec::signature::SigningMode;
use crate::exec::transport::Transport;
use crate::ixit::DeclaredEnvironment;
use crate::vocab::{HttpMethod, Iteration};

/// What the evaluator needs to reach a server under test.
///
/// A catalogue run puts a principal's marker in `authorization` and
/// `administrative`, never its credential: the transport puts the credential
/// in the marker's place on the wire ([`crate::arazzo::credential`]).
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
    /// The `Authorization` header value sent on a step whose
    /// `x-veredictum-principal` is `administrative`, if the party supplies
    /// one. It is never recorded, and such a step is errored without it, so
    /// a caller skips those workflows first
    /// ([`crate::arazzo::principal::undeclared_skips`]).
    pub administrative: Option<String>,
    /// The SMART Platform base URL a Platform source's paths are appended
    /// to, if the party supplies one; a step on such a source is errored
    /// without it, so a caller skips those workflows first
    /// ([`crate::arazzo::platform::undeclared_skips`]).
    pub platform_url: Option<String>,
    /// The scope sets the party covers with a token. A row presenting
    /// another is skipped as `not-declared`; a covered one is sent with the
    /// scope set's marker ([`crate::arazzo::scope::marker`]).
    pub scoped: Coverage,
}

impl RunConfig {
    /// A configuration against `base_url` with no credential, no Platform
    /// base URL and no scope set covered.
    #[must_use]
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_owned(),
            authorization: None,
            signing: None,
            administrative: None,
            platform_url: None,
            scoped: Coverage::default(),
        }
    }

    /// The empty recording a run under this configuration starts, its log
    /// naming the base URLs and which principals and scope sets the run was
    /// given, never a credential.
    #[must_use]
    pub fn recording(&self) -> Har {
        let mut har = Har::new(&self.base_url);
        har.log.veredictum.administrative = self.administrative.is_some();
        har.log
            .veredictum
            .platform_url
            .clone_from(&self.platform_url);
        har.log.veredictum.scoped = self.scoped.clone();
        har
    }
}

/// What the caller decides about the document's workflows before a run.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// The rows file of each data-driven workflow, by `workflowId`.
    pub rows: BTreeMap<String, RowsFile>,
    /// The workflows not run, by `workflowId`, with the reason and citation.
    pub skips: BTreeMap<String, Skip>,
    /// The party's IXIT, which a rows file's `declared` inputs read.
    pub ixit: Option<Value>,
    /// The environment the party's IXIT declares, which a rows file's
    /// `server` reads.
    pub environment: Option<DeclaredEnvironment>,
    /// The party's statement, which a rows file's `declared` inputs read.
    pub statement: Option<Value>,
}

impl Plan {
    /// Whether the rows file of `workflow_id` says `server: exclusive`,
    /// whether or not the party declares such a server.
    #[must_use]
    pub fn is_exclusive(&self, workflow_id: &str) -> bool {
        self.rows
            .get(workflow_id)
            .is_some_and(|file| file.server == Some(ServerOwnership::Exclusive))
    }
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
    /// A `type: arazzo` source does not load.
    #[error(transparent)]
    Document(#[from] crate::arazzo::document::DocumentError),
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
    externals: BTreeMap<String, (String, Rc<ArazzoDocument>)>,
}

/// The document a workflow is run from: its case-id prefix and the document.
type DocumentRef<'a> = (&'a str, &'a ArazzoDocument);

impl<'d> Evaluator<'d> {
    /// Prepares a live run of `document`, whose case ids are prefixed with
    /// `name`; `repo_root` is the root citations are relative to.
    ///
    /// # Errors
    /// [`RunError::Oas`] when a source's schemas do not load, and
    /// [`RunError::Document`] when a `type: arazzo` source does not load.
    pub fn new(
        document: &'d ArazzoDocument,
        name: &str,
        config: RunConfig,
        repo_root: &Path,
    ) -> Result<Self, RunError> {
        let mut externals = BTreeMap::new();
        for (source, file) in document.workflow_sources() {
            let prefix = file
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .and_then(|n| n.strip_suffix(crate::arazzo::gates::DOCUMENT_SUFFIX))
                .unwrap_or(source)
                .to_owned();
            externals.insert(
                source.clone(),
                (prefix, Rc::new(ArazzoDocument::load(file)?)),
            );
        }
        let har = config.recording();
        Ok(Self {
            externals,
            document,
            name: name.to_owned(),
            har,
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
        self.run_where(plan, transport, |_| true)
    }

    /// Runs, in document order, only the workflows `select` keeps.
    ///
    /// # Errors
    /// [`RunError`] when the document cannot be run at all; everything else
    /// becomes a row's classification.
    pub fn run_where(
        &mut self,
        plan: &Plan,
        transport: &mut dyn Transport,
        select: impl Fn(&Workflow) -> bool,
    ) -> Result<Results, RunError> {
        let mut rows = Vec::new();
        for workflow in &self.document.description().workflows {
            if select(workflow) {
                rows.extend(self.run_case(workflow, plan, transport)?);
            }
        }
        Ok(Results {
            document: self.name.clone(),
            engine: Engine {
                name: "veredictum".to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            instrument_status: crate::party::InstrumentStatus::CURRENT,
            base_url: self.config.base_url.clone(),
            rows,
        })
    }

    /// The recording of everything run so far.
    #[must_use]
    pub fn into_recording(self) -> Har {
        self.har
    }

    /// Records into `recording` instead of an empty one, so the runs of
    /// several documents share one recording and one entry numbering.
    pub(crate) fn continue_recording(&mut self, recording: Har) {
        self.har = recording;
    }

    /// Replays from `inputs`, the recorded inputs of every workflow run not
    /// yet replayed, oldest first.
    pub(crate) fn replay_inputs(&mut self, inputs: VecDeque<Row>) {
        self.replay = Some(inputs);
    }

    /// The recorded inputs this replay has not reached.
    pub(crate) fn unreplayed(&mut self) -> VecDeque<Row> {
        self.replay.take().unwrap_or_default()
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
        let rows = match self.declared_rows(workflow, file, plan, &case) {
            Ok(rows) => rows,
            Err(skipped) => return Ok(skipped),
        };
        let iteration = file.map_or(Iteration::ResetPerRow, |f| f.iteration);
        let mut shared = None;
        if iteration == Iteration::SinglePass {
            let main = self.document;
            shared = Some(self.setup(
                (&self.name.clone(), main),
                workflow,
                0,
                transport,
                &mut Vec::new(),
            )?);
        }
        let mut out = Vec::new();
        let mut produced = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            if scope::uncovered(workflow, row, &self.config.scoped).is_some() {
                produced.push(BTreeMap::new());
                out.push(ResultRow {
                    case: case.clone(),
                    row: index,
                    status: Status::Skipped,
                    inputs: row.clone(),
                    generated: Vec::new(),
                    skip: Some(scope::undeclared_skip(&self.repo_root)),
                    explanation: None,
                });
                continue;
            }
            let setup = if let Some(done) = &shared {
                done.clone()
            } else {
                let main = self.document;
                let name = self.name.clone();
                self.setup((&name, main), workflow, index, transport, &mut Vec::new())?
            };
            let outcome = match setup {
                Ok(workflows) => {
                    let main = self.document;
                    let name = self.name.clone();
                    self.run_workflow(
                        (&name, main),
                        workflow,
                        row,
                        index,
                        RunRole::Row,
                        &workflows,
                        transport,
                    )?
                }
                Err(explanation) => WorkflowOutcome {
                    status: Status::Errored,
                    explanation: Some(explanation),
                    inputs: row.clone(),
                    generated: Vec::new(),
                    outputs: BTreeMap::new(),
                },
            };
            produced.push(outcome.outputs);
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
        if let Some(file) = file
            && out.iter().all(|row| row.status == Status::Passed)
        {
            for aggregate in &file.aggregate {
                if let Some(explanation) = self.judge_distinct(aggregate, &produced)
                    && let Some(last) = out.last_mut()
                {
                    last.status = Status::Failed;
                    last.explanation = Some(explanation);
                    break;
                }
            }
        }
        Ok(out)
    }

    /// The rows a workflow runs with, each completed by the values its rows
    /// file declares; `Err` carries every row skipped as `not-declared` when
    /// the party declares one of those values not at all, or does not
    /// declare the server the file's `server` needs.
    fn declared_rows(
        &self,
        workflow: &Workflow,
        file: Option<&RowsFile>,
        plan: &Plan,
        case: &str,
    ) -> Result<Vec<Row>, Vec<ResultRow>> {
        let mut rows = file
            .map(|f| f.rows.clone())
            .filter(|rows| !rows.is_empty())
            .unwrap_or_else(|| vec![Row::new()]);
        let server_declared = file
            .and_then(|f| f.server)
            .is_none_or(|server| server.declared_by(plan.environment.as_ref()));
        let Some(file) = file.filter(|f| !f.declared.is_empty() || !server_declared) else {
            return Ok(rows);
        };
        if server_declared && let Some(values) = declared_values(file, plan) {
            for row in &mut rows {
                row.extend(values.clone());
            }
            return Ok(rows);
        }
        let why = first_citation(workflow).map(|c| Why::of(&c, &self.repo_root));
        Err(rows
            .iter()
            .enumerate()
            .map(|(index, row)| ResultRow {
                case: case.to_owned(),
                row: index,
                status: Status::Skipped,
                inputs: row.clone(),
                generated: Vec::new(),
                skip: why.clone().map(|why| Skip {
                    reason: SkipReason::NotDeclared,
                    why,
                }),
                explanation: None,
            })
            .collect())
    }

    /// Judges one `distinct` aggregate over every row's workflow outputs;
    /// `Some` explains the first value two rows share, or an output a row
    /// did not produce.
    fn judge_distinct(
        &self,
        aggregate: &Aggregate,
        produced: &[BTreeMap<String, Value>],
    ) -> Option<Explanation> {
        let name = &aggregate.distinct;
        let why = Some(Why::of(&aggregate.cite, &self.repo_root));
        let explain = |received: String, whose: Whose| Explanation {
            step: "aggregate".to_owned(),
            what: format!("No two rows produce the same `{name}`."),
            why: why.clone(),
            expected: format!("every row's `{name}` differs"),
            received,
            exchange: None,
            whose,
        };
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        for (index, outputs) in produced.iter().enumerate() {
            let Some(value) = outputs.get(name) else {
                return Some(explain(
                    format!("not judged: row {index} produced no `{name}`"),
                    Whose::Instrument,
                ));
            };
            if let Some(first) = seen.insert(value.to_string(), index) {
                let whose = if aggregate.cite.level == Level::Should {
                    Whose::Advisory
                } else {
                    Whose::Server
                };
                return Some(explain(
                    format!("rows {first} and {index} both produced {value}"),
                    whose,
                ));
            }
        }
        None
    }

    /// Runs every workflow `workflow` depends on, depth first; `Err` carries
    /// the explanation of the first one that did not pass.
    fn setup(
        &mut self,
        (name, document): DocumentRef<'_>,
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
            // NOTE: Arazzo 1.1.0 §Workflow Object `dependsOn`: a workflow of another
            // document is named `$sourceDescriptions.<name>.<workflowId>`.
            let held: Rc<ArazzoDocument>;
            let (owner, owner_document, workflow_id) = match split_qualified(id) {
                Some((source, workflow_id)) => {
                    let (prefix, external) = self
                        .externals
                        .get(source)
                        .ok_or_else(|| RunError::UnknownWorkflow(id.clone()))?;
                    let prefix = prefix.clone();
                    held = Rc::clone(external);
                    (prefix, &*held, workflow_id)
                }
                None => (name.to_owned(), document, id.as_str()),
            };
            let dependency = owner_document
                .workflow(workflow_id)
                .ok_or_else(|| RunError::UnknownWorkflow(id.clone()))?;
            stack.push(workflow.workflow_id.clone());
            let nested = self.setup((&owner, owner_document), dependency, row, transport, stack)?;
            stack.pop();
            let nested = match nested {
                Ok(nested) => nested,
                Err(explanation) => return Ok(Err(explanation)),
            };
            let outcome = self.run_workflow(
                (&owner, owner_document),
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

    #[expect(
        clippy::too_many_arguments,
        reason = "one argument per part of a run: the document, the workflow, the row, its role, its setups and the wire"
    )]
    fn run_workflow(
        &mut self,
        (name, document): DocumentRef<'_>,
        workflow: &Workflow,
        supplied: &Row,
        row: usize,
        role: RunRole,
        workflows: &WorkflowOutputs,
        transport: &mut dyn Transport,
    ) -> Result<WorkflowOutcome, RunError> {
        let (inputs, generated) = self.inputs_for(workflow, supplied)?;
        let page = self.har.begin_page(PageContext {
            case: format!("{name}/{}", workflow.workflow_id),
            row,
            role,
            inputs: inputs.clone(),
        });
        let workflow_cite = gated_citation(workflow.extensions.as_ref());
        let mut outputs: WorkflowOutputs = BTreeMap::new();
        let mut red = None;
        let mut advisory = None;
        for step in &workflow.steps {
            let step_cite =
                gated_citation(step.extensions.as_ref()).or_else(|| workflow_cite.clone());
            let context = StepContext {
                document,
                page: &page,
                step_cite: step_cite.as_ref(),
                inputs: &inputs,
                outputs: &outputs,
                workflows,
            };
            match self.drive_step(step, &context, transport) {
                Ok((captured, should)) => {
                    if advisory.is_none()
                        && let Some(should) = should
                    {
                        advisory = Some(self.explain(step, *should));
                    }
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
        if red.is_none() {
            red = advisory.map(|explanation| Explanation {
                whose: Whose::Advisory,
                ..explanation
            });
        }
        Ok(WorkflowOutcome {
            status: match &red {
                None => Status::Passed,
                Some(e) if matches!(e.whose, Whose::Server | Whose::Advisory) => Status::Failed,
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
    /// its criteria and its response schema, and returns its captured outputs
    /// with the first `should` criterion that did not hold, if any.
    fn drive_step(
        &mut self,
        step: &Step,
        context: &StepContext<'_>,
        transport: &mut dyn Transport,
    ) -> Result<Driven, Box<Red>> {
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
            compose(context.document, step, &self.config, &scope).map_err(|r| errored(&r))?;
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
        let should = self
            .judge_criteria(step, cite, &scope)
            .map_err(|red| red.at(entry))?
            .map(|red| red.at(entry));
        let mut captured = BTreeMap::new();
        for (name, value) in &step.outputs {
            if let Some(value) = output_value(value, &scope).map_err(|e| errored(&e).at(entry))? {
                captured.insert(name.clone(), value);
            }
        }
        derive_outputs(step, &mut captured).map_err(|reason| {
            Red::errored(cite.cloned(), "the step's derived outputs", reason).at(entry)
        })?;
        Ok((captured, should))
    }

    /// Judges a step's criteria in order. The first `must` criterion that
    /// does not hold, or any criterion that cannot be judged, turns the row
    /// red; a `should` criterion that does not hold is returned and judging
    /// goes on, so the row is advisory only when no `must` fails.
    fn judge_criteria(
        &mut self,
        step: &Step,
        cite: Option<&Citation>,
        scope: &Scope<'_>,
    ) -> Result<Option<Box<Red>>, Box<Red>> {
        let mut should = None;
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
                    let advisory = why.as_ref().is_some_and(|c| c.level == Level::Should);
                    let red = Box::new(Red {
                        status: Status::Failed,
                        why,
                        expected,
                        received,
                        entry: None,
                    });
                    if !advisory {
                        return Err(red);
                    }
                    should.get_or_insert(red);
                }
                Err(error) => return Err(Red::errored(why, criterion.condition.clone(), error)),
            }
        }
        Ok(should)
    }
}

/// Adds a step's `x-veredictum-derive` outputs to what it captured. A
/// derivation over an output the response did not carry is left out, so a
/// later step that reads it is errored for the missing output; a captured
/// value that is not an `OBJECT_VERSION_ID` stops the row as errored here.
fn derive_outputs(step: &Step, captured: &mut BTreeMap<String, Value>) -> Result<(), String> {
    let derivations = derivations_of(step.extensions.as_ref(), |name| {
        step.outputs.contains_key(name)
    })
    .map_err(|error| error.to_string())?;
    for (name, derivation) in derivations {
        let Some(value) = captured.get(&derivation.of) else {
            continue;
        };
        let Value::String(text) = value else {
            return Err(format!(
                "step `{}` derives `{name}` from output `{}`, which is {} rather than a string",
                step.step_id,
                derivation.of,
                json_kind(value)
            ));
        };
        let part = derivation.function.apply(text).map_err(|error| {
            format!(
                "step `{}` derives `{name}` from output `{}`: {error}",
                step.step_id, derivation.of
            )
        })?;
        captured.insert(name, Value::String(part));
    }
    Ok(())
}

/// The JSON kind of a value, for an explanation.
fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

/// The first citation of a workflow, in document order: its own, else its
/// first step's, else its first criterion's.
fn first_citation(workflow: &Workflow) -> Option<Citation> {
    gated_citation(workflow.extensions.as_ref()).or_else(|| {
        workflow.steps.iter().find_map(|step| {
            gated_citation(step.extensions.as_ref()).or_else(|| {
                step.success_criteria
                    .iter()
                    .find_map(|c| gated_citation(c.extensions.as_ref()))
            })
        })
    })
}

/// The values a rows file's `declared` inputs read from the party's
/// declarations; `None` when the party declares one of them not at all.
fn declared_values(file: &RowsFile, plan: &Plan) -> Option<Row> {
    let mut out = Row::new();
    for (input, declared) in &file.declared {
        let document = match declared.document {
            Declaration::Ixit => plan.ixit.as_ref(),
            Declaration::Statement => plan.statement.as_ref(),
        }?;
        out.insert(input.clone(), declared.read(document)?.clone());
    }
    Some(out)
}

/// A driven step: its captured outputs and the first `should` criterion
/// that did not hold.
type Driven = (BTreeMap<String, Value>, Option<Box<Red>>);

/// What a step reads from its workflow run.
struct StepContext<'a> {
    document: &'a ArazzoDocument,
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
        if parameter.in_ == Some(ParameterLocation::Query) {
            let pairs = query_pairs(&parameter.name, &parameter.value, scope);
            for (name, value) in pairs.map_err(|e| e.to_string())? {
                query.push(format!(
                    "{}={}",
                    urlencoding::encode(&name),
                    urlencoding::encode(&value)
                ));
                request
                    .query
                    .entry(name)
                    .and_modify(|joined| {
                        joined.push(',');
                        joined.push_str(&value);
                    })
                    .or_insert(value);
            }
            continue;
        }
        let value = resolve_value(&parameter.value, scope).map_err(|e| e.to_string())?;
        match parameter.in_ {
            Some(ParameterLocation::Path) => {
                request.path.insert(parameter.name.clone(), value);
            }
            Some(ParameterLocation::Header) => {
                request
                    .headers
                    .insert(parameter.name.to_ascii_lowercase(), value);
            }
            other => return Err(format!("parameter location {other:?}")),
        }
    }
    let base_url = match operation.base {
        Base::Rest => config.base_url.as_str(),
        Base::Platform => config
            .platform_url
            .as_deref()
            .ok_or_else(|| "the SMART Platform base URL is not supplied".to_owned())?,
    };
    request.url = request_url(&operation.path, &request.path, base_url, &query)?;
    let mut body = None;
    let mut body_is_json = true;
    if let Some(request_body) = &step.request_body {
        // NOTE: Arazzo 1.1.0 §Request Body Object: a payload's runtime expressions "MUST be
        // evaluated prior to calling the referenced operation"; a whole-string one is the body.
        let mut payload = body_value(request_body.payload.as_ref().unwrap_or(&Value::Null), scope)?;
        for replacement in &request_body.replacements {
            let value = match &replacement.value {
                ValueOrSelector::Literal(literal) => body_value(literal, scope)?,
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
    if let Some(authorization) = credential(step, config, scope.inputs)? {
        sent_headers
            .entry("authorization".to_owned())
            .or_insert(authorization);
    }
    Ok(Composed {
        request,
        sent_headers,
        method: operation.method,
        body,
        body_is_json,
    })
}

/// The `Authorization` value a step is sent with: the marker of the scope
/// set it names, the administrative principal's when it names that, else the
/// run's.
fn credential(step: &Step, config: &RunConfig, inputs: &Row) -> Result<Option<String>, String> {
    if let Some(reference) = scope_of(step.extensions.as_ref()).map_err(|e| e.to_string())? {
        let set = reference.resolve(inputs)?;
        if !config.scoped.covers(&set) {
            return Err(format!("no token is supplied for the scope set `{set}`"));
        }
        return Ok(Some(scope::marker(&set)));
    }
    match principal_of(step.extensions.as_ref()).map_err(|e| e.to_string())? {
        Some(Principal::Administrative) => config
            .administrative
            .clone()
            .map(Some)
            .ok_or_else(|| "the administrative principal is not supplied".to_owned()),
        None => Ok(config.authorization.clone()),
    }
}

/// The name and value pairs a query parameter is sent as. A list is one
/// pair per item under the parameter's name (OpenAPI 3.0.3 §Style Examples:
/// an exploded `form` array is `id=3&id=4&id=5`), which the request view
/// keeps comma-joined; an object is one pair per member.
fn query_pairs(
    name: &str,
    value: &ValueOrSelector,
    scope: &Scope<'_>,
) -> Result<Vec<(String, String)>, EvalError> {
    match value {
        ValueOrSelector::Literal(Value::Array(items)) => items
            .iter()
            .map(|item| {
                resolve_value(&ValueOrSelector::Literal(item.clone()), scope)
                    .map(|value| (name.to_owned(), value))
            })
            .collect(),
        ValueOrSelector::Literal(Value::Object(members)) => exploded(members, scope),
        other => Ok(vec![(name.to_owned(), resolve_value(other, scope)?)]),
    }
}

/// The URL a request is sent to: `template` with its path parameters bound
/// and its query expansions left out, under `base_url`, with `query`.
fn request_url(
    template: &str,
    path_values: &BTreeMap<String, String>,
    base_url: &str,
    query: &[String],
) -> Result<String, String> {
    let mut path = without_query_expansions(template);
    for (name, value) in path_values {
        path = path.replace(&format!("{{{name}}}"), &urlencoding::encode(value));
    }
    if path.contains('{') {
        return Err(format!("path `{path}` has an unbound parameter"));
    }
    let url = format!("{}{path}", base_url.trim_end_matches('/'));
    Ok(if query.is_empty() {
        url
    } else {
        format!("{url}?{}", query.join("&"))
    })
}

/// `template` without its RFC 6570 §3.2.8 form-style query expansions
/// (`{?name*}`) and §3.2.9 continuations (`{&name}`): the query parameters
/// they name are sent from the step's own query parameters.
fn without_query_expansions(template: &str) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let (head, tail) = rest.split_at(start);
        out.push_str(head);
        let expansion = tail.starts_with("{?") || tail.starts_with("{&");
        if let (true, Some(end)) = (expansion, tail.find('}')) {
            rest = tail.get(end.saturating_add(1)..).unwrap_or_default();
        } else {
            out.push('{');
            rest = tail.get(1..).unwrap_or_default();
        }
    }
    out.push_str(rest);
    out
}

/// A payload or replacement value as sent: a string that is a runtime
/// expression is its value, and anything else has the embedded `{$…}`
/// expressions of every string it holds substituted, at any depth.
fn body_value(value: &Value, scope: &Scope<'_>) -> Result<Value, String> {
    match value {
        Value::String(text) if text.starts_with('$') => Expr::parse(text)
            .map_err(EvalError::from)
            .and_then(|expr| {
                scope
                    .eval(&expr)?
                    .ok_or(EvalError::MissingOutput(text.clone()))
            })
            .map_err(|e| e.to_string()),
        other => embedded(other, scope),
    }
}

/// `value` with the embedded `{$…}` expressions of each string it holds
/// substituted; a string that merely starts with `$` stays data.
fn embedded(value: &Value, scope: &Scope<'_>) -> Result<Value, String> {
    match value {
        // NOTE: Arazzo 1.1.0 §Runtime Expressions: expressions "can be embedded into string
        // values by surrounding the expression with {} curly braces", a nested string included.
        Value::String(text) => scope
            .substitute(text, str::to_owned)
            .map(Value::String)
            .map_err(|e| e.to_string()),
        Value::Array(items) => items
            .iter()
            .map(|item| embedded(item, scope))
            .collect::<Result<_, _>>()
            .map(Value::Array),
        Value::Object(members) => members
            .iter()
            .map(|(key, member)| Ok((key.clone(), embedded(member, scope)?)))
            .collect::<Result<_, String>>()
            .map(Value::Object),
        literal => Ok(literal.clone()),
    }
}

/// The citation an extension map carries, read after the gate has run.
fn gated_citation(extensions: Option<&BTreeMap<String, Value>>) -> Option<Citation> {
    // NOTE: no openEHR spec governs this — our own design; `ArazzoDocument::check`
    // refuses a malformed citation before any run, so here an `Err` cannot occur.
    citation_of(extensions).ok().flatten()
}

/// A parameter value as the text it is sent as.
/// The name and value pairs an object-valued query parameter is sent as: one
/// pair per member, each member value resolved like a parameter value.
fn exploded(
    members: &serde_json::Map<String, Value>,
    scope: &Scope<'_>,
) -> Result<Vec<(String, String)>, EvalError> {
    // NOTE: OpenAPI 3.0.3 §Parameter Object: `form` is the default style of a query
    // parameter and `explode` defaults to true for it, so an object is one pair per member.
    members
        .iter()
        .map(|(name, value)| {
            resolve_value(&ValueOrSelector::Literal(value.clone()), scope)
                .map(|value| (name.clone(), value))
        })
        .collect()
}

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
    let config = rejudge_config(recording, signing);
    let mut evaluator = Evaluator::new(document, name, config, repo_root)?;
    evaluator.replay_inputs(recorded_inputs(recording));
    let mut transport = HarTransport::new(recording);
    let results = evaluator.run(plan, &mut transport)?;
    replayed_whole(&evaluator.unreplayed(), &transport)?;
    Ok(results)
}

/// The configuration a re-judgement of `recording` runs under: the recorded
/// base URL, and a placeholder credential that is never sent anywhere.
pub(crate) fn rejudge_config(recording: &Har, signing: Option<SigningMode>) -> RunConfig {
    RunConfig {
        base_url: recording.log.veredictum.base_url.clone(),
        authorization: None,
        signing,
        administrative: Some(har::REDACTED.to_owned()),
        platform_url: recording.log.veredictum.platform_url.clone(),
        scoped: recording.log.veredictum.scoped.clone(),
    }
}

/// The inputs of every workflow run `recording` holds, oldest first.
pub(crate) fn recorded_inputs(recording: &Har) -> VecDeque<Row> {
    recording
        .log
        .pages
        .iter()
        .map(|page| page.veredictum.inputs.clone())
        .collect()
}

/// Refuses a re-judgement that left recorded workflow runs or exchanges
/// unreplayed.
///
/// # Errors
/// [`RunError::Replay`] naming what was left.
pub(crate) fn replayed_whole(
    unreplayed: &VecDeque<Row>,
    transport: &HarTransport<'_>,
) -> Result<(), RunError> {
    if !unreplayed.is_empty() {
        return Err(RunError::Replay(format!(
            "{} recorded workflow run(s) were not replayed",
            unreplayed.len()
        )));
    }
    if transport.remaining() > 0 {
        return Err(RunError::Replay(format!(
            "{} recorded exchange(s) were not replayed",
            transport.remaining()
        )));
    }
    Ok(())
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
        bodies: Vec<Option<Value>>,
    }

    impl Scripted {
        fn new(answers: &[(u16, &'static str)]) -> Self {
            Self {
                answers: answers.iter().copied().collect(),
                sent: Vec::new(),
                bodies: Vec::new(),
            }
        }
    }

    impl Transport for Scripted {
        fn exchange(
            &mut self,
            method: HttpMethod,
            url: &str,
            headers: &BTreeMap<String, String>,
            body: Option<&Value>,
            _body_is_json: bool,
        ) -> Result<RawResponse, String> {
            self.sent.push((method, url.to_owned(), headers.clone()));
            self.bodies.push(body.cloned());
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
            administrative: Some("Basic YWRtaW46eA==".to_owned()),
            platform_url: None,
            scoped: Coverage::default(),
        }
    }

    fn plan_with(workflow: &str, rows: &[Row], iteration: Iteration) -> Plan {
        Plan {
            rows: BTreeMap::from([(
                workflow.to_owned(),
                RowsFile {
                    workflow: format!("ehr/{workflow}"),
                    iteration,
                    literals: Vec::new(),
                    recipe: None,
                    declared: BTreeMap::new(),
                    server: None,
                    aggregate: Vec::new(),
                    rows: rows.to_vec(),
                },
            )]),
            ..Plan::default()
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

    const SHOULD_DOC: &str = "arazzo: 1.1.0
info: { title: t, version: '1' }
sourceDescriptions:
  - { name: ehr, url: ehr.yaml, type: openapi }
workflows:
  - workflowId: read
    x-veredictum-cite: { source: s, section: d, quote: q }
    inputs: { type: object, properties: { ehr_id: { type: string, format: uuid } } }
    steps:
      - stepId: get
        description: Read it.
        operationId: $sourceDescriptions.ehr.ehr_get_by_id
        parameters: [{ name: ehr_id, in: path, value: $inputs.ehr_id }]
        successCriteria:
          - condition: $response.header.ETag != null
            x-veredictum-cite: { source: s, section: d, quote: It SHOULD be sent., level: should }
          - condition: $statusCode == 200
";

    fn drive_read(answers: &[(u16, &'static str)]) -> ResultRow {
        let (dir, document) = load(SHOULD_DOC);
        let mut transport = Scripted::new(answers);
        let (results, _) = run_document(
            &document,
            "ehr",
            config(),
            dir.path(),
            &Plan::default(),
            &mut transport,
        )
        .unwrap();
        results.rows.first().unwrap().clone()
    }

    #[test]
    fn a_row_whose_only_failing_criteria_are_should_is_failed_and_advisory() {
        let row = drive_read(&[(200, "")]);
        assert_eq!(row.status, Status::Failed);
        let why = row.explanation.unwrap();
        assert_eq!(why.whose, Whose::Advisory);
        assert_eq!(why.expected, "$response.header.ETag != null");
    }

    #[test]
    fn a_must_failure_after_a_should_failure_counts_against_the_server() {
        let row = drive_read(&[(404, "")]);
        assert_eq!(row.status, Status::Failed);
        let why = row.explanation.unwrap();
        assert_eq!(why.whose, Whose::Server);
        assert_eq!(why.expected, "status 200");
    }

    const CREATE_DOC: &str = "arazzo: 1.1.0
info: { title: t, version: '1' }
sourceDescriptions:
  - { name: ehr, url: ehr.yaml, type: openapi }
workflows:
  - workflowId: load
    x-veredictum-cite: { source: s, section: d, quote: q }
    inputs: { type: object, properties: { system_id: { type: string }, n: { type: integer } } }
    steps:
      - stepId: create
        description: Create one.
        operationId: $sourceDescriptions.ehr.ehr_create
        successCriteria: [{ condition: $statusCode == 201 }]
        outputs: { ehr_id: $response.body#/ehr_id/value }
    outputs: { ehr_id: $steps.create.outputs.ehr_id }
";

    fn rows_file(rows: &[i64], extra: &str) -> RowsFile {
        let listed = rows
            .iter()
            .map(|n| format!("  - {{ n: {n} }}\n"))
            .collect::<Vec<_>>()
            .concat();
        let text = format!("workflow: ehr/load\niteration: single_pass\n{extra}rows:\n{listed}");
        crate::arazzo::gates::rows::parse(
            &crate::load::yaml_str_to_value(&text, Path::new("r")).unwrap(),
        )
        .unwrap()
    }

    fn drive_load(file: RowsFile, plan: Plan, answers: &[(u16, &'static str)]) -> Results {
        let (dir, document) = load(CREATE_DOC);
        let mut transport = Scripted::new(answers);
        let plan = Plan {
            rows: BTreeMap::from([("load".to_owned(), file)]),
            ..plan
        };
        run_document(
            &document,
            "ehr",
            config(),
            dir.path(),
            &plan,
            &mut transport,
        )
        .unwrap()
        .0
    }

    const DISTINCT: &str = "aggregate:\n  - distinct: ehr_id\n    x-veredictum-cite: { source: s, section: d, quote: q }\n";

    #[test]
    fn a_distinct_aggregate_fails_the_last_row_when_two_rows_share_a_value() {
        let results = drive_load(
            rows_file(&[1, 2, 3], DISTINCT),
            Plan::default(),
            &[
                (201, r#"{"ehr_id":{"value":"a"}}"#),
                (201, r#"{"ehr_id":{"value":"b"}}"#),
                (201, r#"{"ehr_id":{"value":"a"}}"#),
            ],
        );
        let statuses: Vec<Status> = results.rows.iter().map(|r| r.status).collect();
        assert_eq!(statuses, [Status::Passed, Status::Passed, Status::Failed]);
        let why = results.rows.last().unwrap().explanation.clone().unwrap();
        assert_eq!(why.step, "aggregate");
        assert_eq!(why.whose, Whose::Server);
        assert!(why.received.contains("rows 0 and 2"), "{}", why.received);
    }

    #[test]
    fn a_distinct_aggregate_holds_over_distinct_values() {
        let results = drive_load(
            rows_file(&[1, 2], DISTINCT),
            Plan::default(),
            &[
                (201, r#"{"ehr_id":{"value":"a"}}"#),
                (201, r#"{"ehr_id":{"value":"b"}}"#),
            ],
        );
        assert!(results.rows.iter().all(|r| r.status == Status::Passed));
    }

    #[test]
    fn a_declared_input_reads_the_ixit_and_an_undeclared_one_skips_every_row() {
        let declared = "declared:\n  system_id: ixit.system_id\n";
        let skipped = drive_load(rows_file(&[1, 2], declared), Plan::default(), &[]);
        assert_eq!(skipped.rows.len(), 2);
        for row in &skipped.rows {
            assert_eq!(row.status, Status::Skipped);
            assert_eq!(row.skip.as_ref().unwrap().reason, SkipReason::NotDeclared);
        }
        let plan = Plan {
            ixit: Some(serde_json::json!({ "system_id": "sys.example" })),
            ..Plan::default()
        };
        let ran = drive_load(
            rows_file(&[1], declared),
            plan,
            &[(201, r#"{"ehr_id":{"value":"a"}}"#)],
        );
        let row = ran.rows.first().unwrap();
        assert_eq!(row.status, Status::Passed);
        assert_eq!(
            row.inputs.get("system_id"),
            Some(&Value::from("sys.example"))
        );
    }

    #[test]
    fn an_exclusive_server_workflow_runs_only_on_a_server_declared_exclusive() {
        let exclusive = "server: exclusive\n";
        let ixit = |exclusive_server: bool| Plan {
            environment: Some(DeclaredEnvironment {
                exclusive_server,
                ..DeclaredEnvironment::default()
            }),
            ..Plan::default()
        };
        for plan in [Plan::default(), ixit(false)] {
            let skipped = drive_load(rows_file(&[1, 2], exclusive), plan, &[]);
            assert_eq!(skipped.rows.len(), 2);
            for row in &skipped.rows {
                assert_eq!(row.status, Status::Skipped);
                assert_eq!(row.skip.as_ref().unwrap().reason, SkipReason::NotDeclared);
            }
        }
        let ran = drive_load(
            rows_file(&[1], exclusive),
            ixit(true),
            &[(201, r#"{"ehr_id":{"value":"a"}}"#)],
        );
        assert_eq!(ran.rows.first().unwrap().status, Status::Passed);
    }

    const INPUTLESS_DOC: &str = "arazzo: 1.1.0
info: { title: t, version: '1' }
sourceDescriptions:
  - { name: ehr, url: ehr.yaml, type: openapi }
workflows:
  - workflowId: wipe
    x-veredictum-cite: { source: s, section: d, quote: q }
    steps:
      - stepId: create
        operationId: $sourceDescriptions.ehr.ehr_create
        successCriteria: [{ condition: $statusCode == 201 }]
";

    #[test]
    fn a_row_less_exclusive_workflow_runs_once_when_declared_and_is_skipped_otherwise() {
        let file = crate::arazzo::gates::rows::parse(&serde_json::json!({
            "workflow": "admin/wipe",
            "iteration": "reset_per_row",
            "server": "exclusive"
        }))
        .unwrap();
        assert!(file.rows.is_empty());
        let (dir, document) = load(INPUTLESS_DOC);
        let wipe = document.description().workflows.first().unwrap();
        assert_eq!(
            crate::arazzo::gates::rows::check_rows(&file, wipe),
            Vec::<String>::new()
        );
        let drive = |exclusive_server: bool, answers: &[(u16, &'static str)]| {
            let plan = Plan {
                rows: BTreeMap::from([("wipe".to_owned(), file.clone())]),
                environment: Some(DeclaredEnvironment {
                    exclusive_server,
                    ..DeclaredEnvironment::default()
                }),
                ..Plan::default()
            };
            assert!(plan.is_exclusive("wipe"));
            let mut transport = Scripted::new(answers);
            run_document(
                &document,
                "admin",
                config(),
                dir.path(),
                &plan,
                &mut transport,
            )
            .unwrap()
            .0
        };
        let ran = drive(true, &[(201, r#"{"ehr_id":{"value":"a"}}"#)]);
        assert_eq!(ran.rows.len(), 1);
        let row = ran.rows.first().unwrap();
        assert_eq!(row.status, Status::Passed);
        assert!(row.inputs.is_empty());
        let skipped = drive(false, &[]);
        assert_eq!(skipped.rows.len(), 1);
        let row = skipped.rows.first().unwrap();
        assert_eq!(row.status, Status::Skipped);
        assert_eq!(row.skip.as_ref().unwrap().reason, SkipReason::NotDeclared);
    }

    const SHARED_SETUP: &str = "arazzo: 1.1.0
info: { title: s, version: '1' }
sourceDescriptions:
  - { name: ehr, url: ehr.yaml, type: openapi }
workflows:
  - workflowId: make
    x-veredictum-cite: { source: s, section: d, quote: q }
    steps:
      - stepId: create
        operationId: $sourceDescriptions.ehr.ehr_create
        successCriteria: [{ condition: $statusCode == 201 }]
";

    const USES_SETUP: &str = "arazzo: 1.1.0
info: { title: t, version: '1' }
sourceDescriptions:
  - { name: ehr, url: ehr.yaml, type: openapi }
  - { name: setup, url: setup.arazzo.yaml, type: arazzo }
workflows:
  - workflowId: read
    dependsOn: [$sourceDescriptions.setup.make]
    x-veredictum-cite: { source: s, section: d, quote: q }
    inputs: { type: object, properties: { ehr_id: { type: string, format: uuid } } }
    steps:
      - stepId: get
        operationId: $sourceDescriptions.ehr.ehr_get_by_id
        parameters: [{ name: ehr_id, in: path, value: $inputs.ehr_id }]
        successCriteria: [{ condition: $statusCode == 200 }]
";

    #[test]
    fn a_setup_from_an_arazzo_source_runs_before_the_row() {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("ehr.yaml").write_str(OAS).unwrap();
        dir.child("setup.arazzo.yaml")
            .write_str(SHARED_SETUP)
            .unwrap();
        dir.child("doc.yaml").write_str(USES_SETUP).unwrap();
        let document = ArazzoDocument::load(&dir.path().join("doc.yaml")).unwrap();
        assert_eq!(document.gate(None), Vec::new());
        let mut transport = Scripted::new(&[(201, ""), (200, "")]);
        let (results, har) = run_document(
            &document,
            "doc",
            config(),
            dir.path(),
            &Plan::default(),
            &mut transport,
        )
        .unwrap();
        assert_eq!(only(&results).status, Status::Passed);
        let (method, url, _) = transport.sent.first().unwrap();
        assert_eq!(
            (*method, url.as_str()),
            (HttpMethod::Post, "http://sut/v1/ehr")
        );
        assert!(har.to_text().unwrap().contains("setup/make"));
        let mut failing = Scripted::new(&[(500, "")]);
        let (results, _) = run_document(
            &document,
            "doc",
            config(),
            dir.path(),
            &Plan::default(),
            &mut failing,
        )
        .unwrap();
        assert_eq!(only(&results).status, Status::Errored);
    }

    #[test]
    fn a_dependency_on_a_missing_arazzo_workflow_is_refused() {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("ehr.yaml").write_str(OAS).unwrap();
        dir.child("setup.arazzo.yaml")
            .write_str(SHARED_SETUP)
            .unwrap();
        dir.child("doc.yaml")
            .write_str(&USES_SETUP.replace("setup.make", "setup.absent"))
            .unwrap();
        let document = ArazzoDocument::load(&dir.path().join("doc.yaml")).unwrap();
        assert!(
            document
                .gate(None)
                .iter()
                .any(|f| f.kind.to_string().contains("dependsOn")),
            "{:?}",
            document.gate(None)
        );
    }

    #[test]
    fn a_dependency_on_a_present_arazzo_workflow_passes_the_gate() {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("ehr.yaml").write_str(OAS).unwrap();
        dir.child("setup.arazzo.yaml")
            .write_str(SHARED_SETUP)
            .unwrap();
        dir.child("doc.yaml").write_str(USES_SETUP).unwrap();
        let document = ArazzoDocument::load(&dir.path().join("doc.yaml")).unwrap();
        let findings = document.gate(None);
        assert!(
            !findings
                .iter()
                .any(|f| f.kind.to_string().contains("dependsOn")),
            "{findings:?}"
        );
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
    fn a_payload_that_is_a_runtime_expression_sends_its_value() {
        let doc = DOC.replace(
            "          - { name: Authorization, in: header, value: Bearer step-token }\n",
            "          - { name: Authorization, in: header, value: Bearer step-token }\n        requestBody: { contentType: application/json, payload: $inputs.body }\n",
        )
        .replace(
            "properties: { ehr_id: { type: string, format: uuid } }",
            "properties: { ehr_id: { type: string, format: uuid }, body: { type: object } }",
        );
        let (dir, document) = load(&doc);
        let mut transport =
            Scripted::new(&[(201, ""), (409, ""), (200, r#"{"ehr_id":{"value":"abc"}}"#)]);
        let body = serde_json::json!({ "_type": "EHR_STATUS", "is_queryable": true });
        let row = Row::from([
            ("ehr_id".to_owned(), Value::from("abc")),
            ("body".to_owned(), body.clone()),
        ]);
        let plan = plan_with("twice", &[row], Iteration::ResetPerRow);
        let (results, _) = run_document(
            &document,
            "ehr",
            config(),
            dir.path(),
            &plan,
            &mut transport,
        )
        .unwrap();
        assert_eq!(only(&results).status, Status::Passed);
        assert_eq!(transport.bodies.first(), Some(&Some(body)));
        assert_eq!(transport.bodies.get(1), Some(&None));
    }

    #[test]
    fn a_text_payload_sends_its_embedded_expressions_substituted() {
        let doc = DOC.replace(
            "          - { name: Authorization, in: header, value: Bearer step-token }\n",
            "          - { name: Authorization, in: header, value: Bearer step-token }\n        requestBody: { contentType: application/xml, payload: '<id>t-{$inputs.ehr_id}</id>' }\n",
        );
        let (dir, document) = load(&doc);
        let mut transport =
            Scripted::new(&[(201, ""), (409, ""), (200, r#"{"ehr_id":{"value":"abc"}}"#)]);
        let row = Row::from([("ehr_id".to_owned(), Value::from("abc"))]);
        let plan = plan_with("twice", &[row], Iteration::ResetPerRow);
        let (results, _) = run_document(
            &document,
            "ehr",
            config(),
            dir.path(),
            &plan,
            &mut transport,
        )
        .unwrap();
        assert_eq!(only(&results).status, Status::Passed);
        assert_eq!(
            transport.bodies.first(),
            Some(&Some(Value::from("<id>t-abc</id>")))
        );
    }

    #[test]
    fn a_json_payload_sends_its_nested_embedded_expressions_substituted() {
        let doc = DOC.replace(
            "          - { name: Authorization, in: header, value: Bearer step-token }\n",
            "          - { name: Authorization, in: header, value: Bearer step-token }\n        requestBody: { contentType: application/json, payload: { a: [{ b: 'x-{$inputs.ehr_id}' }], c: '$data' } }\n",
        );
        let (dir, document) = load(&doc);
        let mut transport =
            Scripted::new(&[(201, ""), (409, ""), (200, r#"{"ehr_id":{"value":"abc"}}"#)]);
        let row = Row::from([("ehr_id".to_owned(), Value::from("abc"))]);
        let plan = plan_with("twice", &[row], Iteration::ResetPerRow);
        let (results, _) = run_document(
            &document,
            "ehr",
            config(),
            dir.path(),
            &plan,
            &mut transport,
        )
        .unwrap();
        assert_eq!(only(&results).status, Status::Passed);
        assert_eq!(
            transport.bodies.first(),
            Some(&Some(
                serde_json::json!({ "a": [{ "b": "x-abc" }], "c": "$data" })
            ))
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

    const DERIVED: &str = "arazzo: 1.1.0
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
        outputs: { version_uid: $response.body#/uid }
        x-veredictum-derive:
          object: { function: object_id, of: version_uid }
          system: { function: creating_system_id, of: version_uid }
          tree: { function: version_tree_id, of: version_uid }
      - stepId: get
        operationId: $sourceDescriptions.ehr.ehr_get_by_id
        parameters: [{ name: ehr_id, in: path, value: $steps.create.outputs.object }]
        successCriteria:
          - condition: $statusCode == 200
          - { context: $steps.create.outputs.system, type: regex, condition: '^sys\\.example$' }
          - { context: $steps.create.outputs.tree, type: regex, condition: '^2$' }
";

    #[test]
    fn a_derived_output_addresses_a_later_step() {
        let (dir, document) = load(DERIVED);
        let mut transport =
            Scripted::new(&[(201, r#"{"uid":"9f3c::sys.example::2"}"#), (200, "{}")]);
        let (results, _) = run_document(
            &document,
            "ehr",
            config(),
            dir.path(),
            &Plan::default(),
            &mut transport,
        )
        .unwrap();
        assert_eq!(only(&results).status, Status::Passed, "{}", only(&results));
        assert_eq!(
            transport.sent.get(1).map(|(_, url, _)| url.as_str()),
            Some("http://sut/v1/ehr/9f3c")
        );
    }

    #[test]
    fn a_value_not_of_the_lexical_form_is_errored_never_failed() {
        for body in [
            r#"{"uid":"9f3c"}"#,
            r#"{"uid":"9f3c::::2"}"#,
            r#"{"uid":42}"#,
        ] {
            let (dir, document) = load(DERIVED);
            let mut transport = Scripted::new(&[(201, body), (200, "{}")]);
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
            assert_eq!(row.status, Status::Errored, "{body}: {row}");
            let explanation = row.explanation.as_ref().unwrap();
            assert_eq!(explanation.step, "create");
            assert!(
                explanation.received.contains("step `create` derives"),
                "{}",
                explanation.received
            );
            assert_eq!(transport.sent.len(), 1, "the derived request is never sent");
        }
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
    fn an_object_query_parameter_is_one_pair_per_member() {
        let inputs = BTreeMap::from([("id".to_owned(), serde_json::json!("e-1"))]);
        let outputs = BTreeMap::new();
        let workflows = BTreeMap::new();
        let scope = Scope {
            inputs: &inputs,
            outputs: &outputs,
            workflows: &workflows,
            exchange: None,
        };
        let members = serde_json::json!({ "ehr": "$inputs.id", "n": 3 });
        let pairs = exploded(members.as_object().unwrap(), &scope).unwrap();
        assert_eq!(
            pairs,
            vec![
                ("ehr".to_owned(), "e-1".to_owned()),
                ("n".to_owned(), "3".to_owned())
            ]
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

    /// Under `single_pass` the one setup run is the ground of every row, so a
    /// setup that does not pass errors every row and drives none of them.
    #[test]
    fn a_single_pass_setup_that_does_not_pass_errors_every_row_undriven() {
        let (results, transport) = run_setup(Iteration::SinglePass, &[(500, "")]);
        let statuses: Vec<Status> = results.rows.iter().map(|r| r.status).collect();
        assert_eq!(
            statuses,
            vec![Status::Skipped, Status::Errored, Status::Errored]
        );
        assert_eq!(transport.sent.len(), 1, "only the setup reached the server");
        for row in results.rows.iter().skip(1) {
            let why = row.explanation.as_ref().unwrap();
            assert_eq!(why.step, "setup/create");
            assert_eq!(why.whose, Whose::Instrument);
        }
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

    const ADMIN_OAS: &str = "openapi: 3.0.3
info: { title: t, version: '1' }
paths:
  /admin/ehr/{ehr_id}:
    delete:
      operationId: admin_ehr_delete
      parameters: [{ name: ehr_id, in: path, required: true, schema: { type: string } }]
      responses: { '204': { description: Deleted. } }
  /admin/ehr/all{?ehr_id*}:
    delete:
      operationId: admin_ehr_delete_all
      parameters: [{ name: ehr_id, in: query, style: form, explode: true, schema: { type: string } }]
      responses: { '204': { description: Deleted. } }
";

    const ADMIN_DOC: &str = "arazzo: 1.1.0
info: { title: t, version: '1' }
sourceDescriptions:
  - { name: admin, url: admin.yaml, type: openapi }
workflows:
  - workflowId: delete
    x-veredictum-cite: { source: s, section: d, quote: q }
    inputs: { type: object, properties: { a: { type: string }, b: { type: string } } }
    steps:
      - stepId: one
        description: Delete one.
        operationId: $sourceDescriptions.admin.admin_ehr_delete
        x-veredictum-principal: administrative
        parameters: [{ name: ehr_id, in: path, value: $inputs.a }]
        successCriteria: [{ condition: $statusCode == 204 }]
      - stepId: some
        description: Delete a subset.
        operationId: $sourceDescriptions.admin.admin_ehr_delete_all
        x-veredictum-principal: administrative
        parameters: [{ name: ehr_id, in: query, value: [$inputs.a, '{$inputs.b}'] }]
        successCriteria: [{ condition: $statusCode == 204 }]
";

    fn admin_document() -> (assert_fs::TempDir, ArazzoDocument) {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("admin.yaml").write_str(ADMIN_OAS).unwrap();
        dir.child("doc.yaml").write_str(ADMIN_DOC).unwrap();
        let document = ArazzoDocument::load(&dir.path().join("doc.yaml")).unwrap();
        (dir, document)
    }

    fn admin_plan() -> Plan {
        let row = Row::from([
            ("a".to_owned(), Value::from("e-1")),
            ("b".to_owned(), Value::from("e 2")),
        ]);
        plan_with("delete", &[row], Iteration::ResetPerRow)
    }

    #[test]
    fn an_administrative_step_is_sent_with_the_administrative_credential_and_a_list_query_explodes()
    {
        let (dir, document) = admin_document();
        let mut transport = Scripted::new(&[(204, ""), (204, "")]);
        let (results, har) = run_document(
            &document,
            "admin",
            config(),
            dir.path(),
            &admin_plan(),
            &mut transport,
        )
        .unwrap();
        assert_eq!(only(&results).status, Status::Passed, "{results:?}");
        let (_, url, headers) = transport.sent.first().unwrap();
        assert_eq!(url, "http://sut/v1/admin/ehr/e-1");
        assert_eq!(
            headers.get("authorization").map(String::as_str),
            Some("Basic YWRtaW46eA==")
        );
        let (_, url, _) = transport.sent.get(1).unwrap();
        assert_eq!(url, "http://sut/v1/admin/ehr/all?ehr_id=e-1&ehr_id=e%202");
        let text = har.to_text().unwrap();
        assert!(!text.contains("YWRtaW46eA"), "{text}");
        let replayed = rejudge(&document, "admin", dir.path(), None, &admin_plan(), &har).unwrap();
        assert_eq!(replayed.to_text().unwrap(), results.to_text().unwrap());
    }

    #[test]
    fn an_administrative_step_without_the_principal_is_errored_never_failed() {
        let (dir, document) = admin_document();
        let mut transport = Scripted::new(&[]);
        let config = RunConfig {
            administrative: None,
            ..config()
        };
        let (results, _) = run_document(
            &document,
            "admin",
            config,
            dir.path(),
            &admin_plan(),
            &mut transport,
        )
        .unwrap();
        assert_eq!(only(&results).status, Status::Errored);
        assert!(transport.sent.is_empty());
    }

    const PLATFORM_OAS: &str = "openapi: 3.0.3
info: { title: t, version: '1' }
x-veredictum-base: platform
paths:
  /.well-known/smart-configuration:
    get: { operationId: discover, responses: { default: { description: d } } }
";

    const REST_OAS: &str = "openapi: 3.0.3
info: { title: t, version: '1' }
paths:
  /templates:
    get: { operationId: list, responses: { default: { description: d } } }
";

    const SMART_DOC: &str = "arazzo: 1.1.0
info: { title: t, version: '1' }
sourceDescriptions:
  - { name: platform, url: platform.yaml, type: openapi }
  - { name: rest, url: rest.yaml, type: openapi }
workflows:
  - workflowId: smart
    x-veredictum-cite: { source: s, section: d, quote: q }
    inputs: { type: object, properties: { scope: { type: string } } }
    steps:
      - stepId: discover
        operationId: $sourceDescriptions.platform.discover
        successCriteria: [{ condition: $statusCode == 200 }]
      - stepId: list
        operationId: $sourceDescriptions.rest.list
        x-veredictum-scope: $inputs.scope
        successCriteria: [{ condition: $statusCode == 200 }]
";

    fn smart_document() -> (assert_fs::TempDir, ArazzoDocument) {
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("platform.yaml").write_str(PLATFORM_OAS).unwrap();
        dir.child("rest.yaml").write_str(REST_OAS).unwrap();
        dir.child("doc.yaml").write_str(SMART_DOC).unwrap();
        let document = ArazzoDocument::load(&dir.path().join("doc.yaml")).unwrap();
        (dir, document)
    }

    fn smart_plan(scopes: &[&str]) -> Plan {
        let rows: Vec<Row> = scopes
            .iter()
            .map(|scope| Row::from([("scope".to_owned(), Value::from(*scope))]))
            .collect();
        plan_with("smart", &rows, Iteration::ResetPerRow)
    }

    fn smart_config() -> RunConfig {
        RunConfig {
            platform_url: Some("http://platform/gw/".to_owned()),
            scoped: Coverage {
                issued: std::collections::BTreeSet::from([scope::ScopeSet::parse(
                    "system/template-*.r",
                )
                .unwrap()]),
                minted: false,
            },
            ..config()
        }
    }

    #[test]
    fn a_platform_step_goes_to_the_platform_base_and_a_scoped_step_carries_its_marker() {
        let (dir, document) = smart_document();
        let plan = smart_plan(&["system/template-*.r", "system/template-*.c"]);
        let mut transport = Scripted::new(&[(200, ""), (200, "")]);
        let (results, har) = run_document(
            &document,
            "smart",
            smart_config(),
            dir.path(),
            &plan,
            &mut transport,
        )
        .unwrap();
        let statuses: Vec<Status> = results.rows.iter().map(|row| row.status).collect();
        assert_eq!(statuses, [Status::Passed, Status::Skipped], "{results:?}");
        let skip = results
            .rows
            .get(1)
            .and_then(|row| row.skip.as_ref())
            .unwrap();
        assert_eq!(skip.reason, SkipReason::NotDeclared);
        assert_eq!(skip.why.quote, scope::undeclared_citation().quote);
        let sent: Vec<(&str, Option<&str>)> = transport
            .sent
            .iter()
            .map(|(_, url, headers)| {
                (
                    url.as_str(),
                    headers.get("authorization").map(String::as_str),
                )
            })
            .collect();
        let marker = scope::marker(&scope::ScopeSet::parse("system/template-*.r").unwrap());
        assert_eq!(
            sent,
            [
                (
                    "http://platform/gw/.well-known/smart-configuration",
                    Some("Basic eDp5")
                ),
                ("http://sut/v1/templates", Some(marker.as_str())),
            ]
        );
        assert_eq!(
            har.log.veredictum.platform_url.as_deref(),
            Some("http://platform/gw/")
        );
        let text = har.to_text().unwrap();
        assert!(!text.contains(scope::MARKER_PREFIX), "{text}");
        let replayed = rejudge(&document, "smart", dir.path(), None, &plan, &har).unwrap();
        assert_eq!(replayed.to_text().unwrap(), results.to_text().unwrap());
    }

    #[test]
    fn a_platform_step_without_the_platform_base_is_errored_never_failed() {
        let (dir, document) = smart_document();
        let config = RunConfig {
            platform_url: None,
            ..smart_config()
        };
        let mut transport = Scripted::new(&[]);
        let (results, _) = run_document(
            &document,
            "smart",
            config,
            dir.path(),
            &smart_plan(&["system/template-*.r"]),
            &mut transport,
        )
        .unwrap();
        assert_eq!(only(&results).status, Status::Errored);
        assert!(transport.sent.is_empty());
    }

    #[test]
    fn a_query_expansion_leaves_the_path_and_a_path_brace_stays() {
        assert_eq!(
            without_query_expansions("/admin/ehr/all{?ehr_id*}"),
            "/admin/ehr/all"
        );
        assert_eq!(without_query_expansions("/a/{id}/b{&x}"), "/a/{id}/b");
        assert_eq!(without_query_expansions("/a/{id"), "/a/{id");
    }
}
