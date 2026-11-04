// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `x-veredictum-control` extension: a refusal workflow names its
//! control, the workflow whose request is the same apart from the one defect
//! the refusal exists to catch.
//!
//! A refusal workflow is one with a criterion `$statusCode == 400` or
//! `$statusCode == 422`. The released text gives an error body no shape, so
//! the evaluator cannot read which part of a request a server refused; a
//! refusal row proves the defect was refused only when the server accepted
//! the same request without the defect in the same run.
//!
//! A control is named the way Arazzo 1.1.0 §Workflow Object `dependsOn` names
//! a workflow: a `workflowId` of the same document, or
//! `$sourceDescriptions.<name>.<workflowId>` of a `type: arazzo` source. A
//! refusal that runs under either arm of an option names one control per arm
//! as a list. A refusal with no twin in the catalogue yet names the issue
//! that tracks one, `{ pending: <issue> }`, and is never judged. A control was
//! accepted when the run holds a row of it and every
//! one of its rows passed or failed only criteria that yield a warning, so the server took
//! the request. A refusal row is judged when no listed control that ran was
//! refused and at least one ran. When every listed control was skipped, the
//! refusal is skipped with the first control's reason, naming it; otherwise
//! it is recorded errored, naming the control, whatever the refusal itself
//! received. No openEHR spec governs the
//! extension itself — our own design.

use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::path::Path;

use reqwest::StatusCode;
use roas_arazzo::v1_1::Workflow;
use serde_json::Value;

use crate::arazzo::document::{ArazzoDocument, CriterionPlan};
use crate::arazzo::expr::{CmpOp, Expr, Operand, SimpleCondition};
use crate::arazzo::results::{Explanation, ResultRow, Skip, Status, Whose};
use crate::arazzo::source::split_qualified;

/// The specification-extension key of a refusal workflow's control.
pub const CONTROL_KEY: &str = "x-veredictum-control";

/// The statuses that make a workflow a refusal workflow: the two that answer
/// a defective request without naming, in the status itself, which part of
/// it was refused.
pub const REFUSAL_STATUSES: [StatusCode; 2] =
    [StatusCode::BAD_REQUEST, StatusCode::UNPROCESSABLE_ENTITY];

/// Why an `x-veredictum-control`, or its absence, does not hold.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ControlError {
    /// The value is neither a non-empty string, a non-empty list of them,
    /// nor `{ pending: <issue> }`.
    #[error(
        "`{CONTROL_KEY}` is neither a non-empty string, a non-empty list of them, nor `{{ pending: <issue> }}`"
    )]
    Shape,
    /// The `pending` form does not name a positive issue number.
    #[error("`{CONTROL_KEY}` `pending` is not a positive issue number")]
    Pending,
    /// A refusal workflow names no control.
    #[error(
        "a refusal workflow (a criterion expects 400 or 422) names no `{CONTROL_KEY}`: the workflow whose request is the same apart from the one defect"
    )]
    Missing,
    /// A workflow that is not a refusal workflow names a control.
    #[error("`{CONTROL_KEY}` on a workflow that expects neither 400 nor 422")]
    NotRefusal,
    /// The control names the workflow itself.
    #[error("`{CONTROL_KEY}` names the workflow itself")]
    Itself,
    /// The control names no workflow that exists.
    #[error("`{CONTROL_KEY}` names `{control}`, {reason}")]
    Unknown {
        /// The control as written.
        control: String,
        /// Why it resolves to no workflow.
        reason: String,
    },
    /// The control is itself a refusal workflow, so its passing proves no
    /// acceptance.
    #[error("`{CONTROL_KEY}` names `{0}`, which is itself a refusal workflow")]
    ControlRefuses(String),
}

/// What a refusal workflow's `x-veredictum-control` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    /// The control workflows, one per option arm the refusal runs under.
    Workflows(Vec<String>),
    /// No twin exists yet; the issue that tracks authoring one.
    Pending(NonZeroU64),
}

/// The control a workflow's extensions name, if any.
///
/// # Errors
/// [`ControlError::Shape`] when the extension is present and is neither a
/// non-empty string, a non-empty list of non-empty strings, nor an object
/// holding `pending` alone, and [`ControlError::Pending`] when `pending` is
/// not a positive integer.
pub fn control_of(
    extensions: Option<&BTreeMap<String, Value>>,
) -> Result<Option<Control>, ControlError> {
    let named = |value: &Value| match value {
        Value::String(control) if !control.is_empty() => Ok(control.clone()),
        _ => Err(ControlError::Shape),
    };
    match extensions.and_then(|e| e.get(CONTROL_KEY)) {
        None => Ok(None),
        Some(Value::Array(items)) if !items.is_empty() => items
            .iter()
            .map(named)
            .collect::<Result<_, _>>()
            .map(|controls| Some(Control::Workflows(controls))),
        Some(Value::Object(members)) => match (members.len(), members.get("pending")) {
            (1, Some(issue)) => issue
                .as_u64()
                .and_then(NonZeroU64::new)
                .map(|issue| Some(Control::Pending(issue)))
                .ok_or(ControlError::Pending),
            _ => Err(ControlError::Shape),
        },
        Some(value) => named(value).map(|control| Some(Control::Workflows(vec![control]))),
    }
}

/// Whether `workflow` is a refusal workflow: one of its criteria is a
/// `simple` condition `$statusCode == 400` or `$statusCode == 422`.
#[must_use]
pub fn is_refusal(workflow: &Workflow) -> bool {
    workflow
        .steps
        .iter()
        .flat_map(|step| &step.success_criteria)
        .any(|criterion| match CriterionPlan::of(criterion) {
            Ok(CriterionPlan::Simple(condition)) => expects_refusal(&condition),
            // NOTE: no openEHR spec governs this — our own design; any other criterion
            // shape is not a status expectation, and a malformed one is the profile gate's.
            _ => false,
        })
}

/// Whether `condition` is `$statusCode == <a refusal status>`, either way
/// round.
fn expects_refusal(condition: &SimpleCondition) -> bool {
    if condition.op != CmpOp::Eq {
        return false;
    }
    let status = match (&condition.left, &condition.right) {
        (Operand::Expr(Expr::StatusCode), Operand::Integer(code))
        | (Operand::Integer(code), Operand::Expr(Expr::StatusCode)) => *code,
        _ => return false,
    };
    u16::try_from(status)
        .ok()
        .and_then(|code| StatusCode::from_u16(code).ok())
        .is_some_and(|code| REFUSAL_STATUSES.contains(&code))
}

/// The control of every refusal case of `document` (run under `name`), by
/// the refusal's case id, each control workflow named by its case id.
///
/// A control of another document carries that document's case-id prefix,
/// its file name without the document suffix, as the evaluator names it.
#[must_use]
pub fn controls(document: &ArazzoDocument, name: &str) -> BTreeMap<String, Control> {
    let mut out = BTreeMap::new();
    for workflow in &document.description().workflows {
        let case = format!("{name}/{}", workflow.workflow_id);
        // NOTE: no openEHR spec governs this — our own design; `validate` refuses a
        // malformed control before any run, so here an `Err` names no control.
        let named = match control_of(workflow.extensions.as_ref()) {
            Ok(Some(Control::Workflows(named))) => named,
            Ok(Some(Control::Pending(issue))) => {
                out.insert(case, Control::Pending(issue));
                continue;
            }
            Ok(None) | Err(_) => continue,
        };
        let targets: Vec<String> = named
            .iter()
            .filter_map(|control| match split_qualified(control) {
                Some((source, workflow_id)) => document
                    .workflow_sources()
                    .get(source)
                    .and_then(|file| document_prefix(file))
                    .map(|prefix| format!("{prefix}/{workflow_id}")),
                None => Some(format!("{name}/{control}")),
            })
            .collect();
        if !targets.is_empty() {
            out.insert(case, Control::Workflows(targets));
        }
    }
    out
}

/// The case-id prefix of the document at `file`: its file name without the
/// document suffix.
fn document_prefix(file: &Path) -> Option<&str> {
    file.file_name()
        .and_then(std::ffi::OsStr::to_str)
        .and_then(|name| name.strip_suffix(crate::arazzo::gates::DOCUMENT_SUFFIX))
}

/// How one control's rows ended in a run.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ControlOutcome {
    /// Every row passed or failed only criteria that yield a warning.
    Accepted,
    /// Every row was skipped; the skip of the first, when it carries one.
    Skipped(Option<Skip>),
    /// A row that ran ended with this status and was not a warning.
    Refused(Status),
}

impl ControlOutcome {
    /// The outcome of the control's first row.
    fn of(row: &ResultRow) -> Self {
        let warning = row
            .explanation
            .as_ref()
            .is_some_and(|explanation| explanation.whose == Whose::Warning);
        match row.status {
            Status::Passed => Self::Accepted,
            Status::Failed if warning => Self::Accepted,
            Status::Skipped => Self::Skipped(row.skip.clone()),
            other => Self::Refused(other),
        }
    }

    /// The outcome of one more row of the control.
    fn with(self, row: &ResultRow) -> Self {
        match (self, Self::of(row)) {
            (refused @ Self::Refused(_), _) | (_, refused @ Self::Refused(_)) => refused,
            (Self::Accepted, _) | (_, Self::Accepted) => Self::Accepted,
            (skipped @ Self::Skipped(_), Self::Skipped(_)) => skipped,
        }
    }
}

/// What a refusal row's controls make of it.
#[derive(Debug)]
enum Judgement {
    /// A control was accepted and none was refused: the row stands.
    Judged,
    /// Every control was skipped: the row is skipped with the first
    /// control's reason, naming that control.
    Skipped(Skip),
    /// The row is not judged, for this reason.
    Unjudged(String),
}

/// Records errored every judged refusal row whose controls were not
/// accepted, and skipped every one whose controls were all skipped.
///
/// A control shows the server accepted the request without the defect in
/// this run. `controls` maps each refusal case to its controls' cases, as
/// [`controls`] builds it. A refusal whose every control was skipped carries
/// the first control's skip reason and citation, with the control named in
/// the detail, because the instrument chose not to judge it. A skipped or
/// already errored refusal row is left as it is.
pub fn apply(rows: &mut [ResultRow], controls: &BTreeMap<String, Control>) {
    let mut outcome: BTreeMap<String, ControlOutcome> = BTreeMap::new();
    for row in rows.iter() {
        if controls.values().any(|control| match control {
            Control::Workflows(named) => named.contains(&row.case),
            Control::Pending(_) => false,
        }) {
            let next = match outcome.remove(&row.case) {
                Some(seen) => seen.with(row),
                None => ControlOutcome::of(row),
            };
            outcome.insert(row.case.clone(), next);
        }
    }
    for row in rows.iter_mut() {
        let Some(control) = controls.get(&row.case) else {
            continue;
        };
        if !matches!(row.status, Status::Passed | Status::Failed) {
            continue;
        }
        let (what, expected, received) = match control {
            Control::Pending(issue) => (
                format!("The refusal has no control yet (#{issue}), so it is not judged."),
                "a control for this refusal".to_owned(),
                format!("no control yet: #{issue}"),
            ),
            Control::Workflows(named) => {
                let received = match judge(named, &outcome) {
                    Judgement::Judged => continue,
                    Judgement::Skipped(skip) => {
                        row.status = Status::Skipped;
                        row.explanation = None;
                        row.skip = Some(skip);
                        continue;
                    }
                    Judgement::Unjudged(received) => received,
                };
                let listed = named
                    .iter()
                    .map(|control| format!("`{control}`"))
                    .collect::<Vec<_>>()
                    .join(" or ");
                (
                    format!(
                        "The control {listed}, the same request without the defect, is accepted in the same run."
                    ),
                    format!("the control {listed} accepted"),
                    received,
                )
            }
        };
        let exchange = row.explanation.take().and_then(|e| e.exchange);
        row.status = Status::Errored;
        row.explanation = Some(Explanation {
            step: "control".to_owned(),
            what,
            why: None,
            expected,
            received: format!("not judged: {received}"),
            exchange,
            whose: Whose::Instrument,
        });
    }
}

/// What the controls `named` make of a refusal row: judged when one was
/// accepted and none refused, skipped when every one of them was skipped
/// with a skip record, and otherwise not judged, naming a control that was
/// refused or saying that none ran.
fn judge(named: &[String], outcome: &BTreeMap<String, ControlOutcome>) -> Judgement {
    let mut accepted = false;
    let mut first_skip: Option<(&str, &Skip)> = None;
    let mut all_skipped = true;
    for control in named {
        match outcome.get(control) {
            Some(ControlOutcome::Refused(status)) => {
                return Judgement::Unjudged(format!("the control `{control}` ended {status}"));
            }
            Some(ControlOutcome::Accepted) => accepted = true,
            Some(ControlOutcome::Skipped(Some(skip))) => {
                first_skip.get_or_insert((control, skip));
            }
            Some(ControlOutcome::Skipped(None)) | None => all_skipped = false,
        }
    }
    if accepted {
        return Judgement::Judged;
    }
    if let (true, Some((control, skip))) = (all_skipped, first_skip) {
        let detail = match &skip.detail {
            Some(detail) => format!(
                "the control `{control}` is skipped as {}: {detail}",
                skip.reason
            ),
            None => format!("the control `{control}` is skipped as {}", skip.reason),
        };
        return Judgement::Skipped(Skip {
            reason: skip.reason,
            why: skip.why.clone(),
            detail: Some(detail),
        });
    }
    Judgement::Unjudged(match named {
        [control] => format!("the control `{control}` did not run"),
        _ => "no control ran".to_owned(),
    })
}
