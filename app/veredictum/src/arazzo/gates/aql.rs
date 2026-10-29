// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The AQL gate.
//!
//! Every AQL query a step sends parses with `openehr-query`, the implementation
//! of the released grammar (`specs/openehr/QUERY/docs/AQL/grammar/AqlLexer.g4`
//! and `AqlParser.g4`) this instrument uses, so a mistyped query never becomes
//! a false failure. A query that does not parse is a malformed request, which
//! ITS-REST answers with `400 Bad Request` ("malformed request syntax,
//! syntactically invalid content",
//! `specs/openehr/ITS-REST/specifications/docs/overview/Requests_and_responses.md`
//! §Response status codes), so it is allowed only in a step whose success
//! criteria expect `$statusCode == 400`. Such a step whose query parses must
//! say in its `description` what else makes the request invalid.
//!
//! A query is read the way the evaluator composes the request: a literal, or
//! a `$inputs` value taken from each row of the workflow's rows file. A value
//! from an earlier response is the server's data, not catalogue text, and is
//! not checked here.

use std::collections::BTreeMap;

use openehr_query::parser::{ParseError, parse_str};
use roas_arazzo::common::reusable::ReusableOr;
use roas_arazzo::v1_1::{ParameterLocation, Step, ValueOrSelector, Workflow};
use serde_json::Value;

use crate::arazzo::document::CriterionPlan;
use crate::arazzo::expr::{CmpOp, Expr, Operand, Scope, SimpleCondition, value_text};
use crate::arazzo::gates::rows::RowsFile;
use crate::arazzo::source::split_qualified;

/// Where an operation carries its AQL query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Carrier {
    /// The named query parameter.
    Parameter(&'static str),
    /// The request-body member at this JSON Pointer.
    BodyMember(&'static str),
    /// The whole `text/plain` request body, when the `query_type` parameter
    /// is absent or `AQL`.
    TextBody,
}

/// The released operations whose request carries an AQL query, each with the
/// OAS element that types it as the `AQL` schema
/// (`specs/rest-oas/query-validation.openapi.yaml` and
/// `definition-validation.openapi.yaml`).
const CARRIERS: &[(&str, Carrier)] = &[
    // `components.parameters.q`: "The AQL query to be executed."
    ("query_execute_adhoc_query", Carrier::Parameter("q")),
    // `components.schemas.AdhocQueryExecute`, property `q`.
    ("query_execute_adhoc_query_body", Carrier::BodyMember("/q")),
    // `/definition/query/{qualified_query_name}` `put`, `text/plain` body.
    ("definition_query_store.yaml", Carrier::TextBody),
    // `/definition/query/{qualified_query_name}/{version}` `put`, same body.
    ("definition_query_version_store.yaml", Carrier::TextBody),
];

/// The query language the `query_type` parameter defaults to
/// (`definition-validation.openapi.yaml` `components.parameters.query_type`,
/// `default: AQL`).
const AQL: &str = "AQL";

/// One refusal: where in the document, and what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AqlFinding {
    /// `workflowId/stepId`.
    pub location: String,
    /// What is wrong, naming the case, the row, the query and the position.
    pub message: String,
}

/// Checks every AQL query `workflow` sends; `case_id` is
/// `<document>/<workflowId>` and `rows` its rows file, when it has one.
#[must_use]
pub fn check_workflow(
    case_id: &str,
    workflow: &Workflow,
    rows: Option<&RowsFile>,
) -> Vec<AqlFinding> {
    let empty = BTreeMap::new();
    let runs: Vec<(Option<usize>, &BTreeMap<String, Value>)> = match rows {
        Some(file) => file
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| (Some(i), r))
            .collect(),
        None => vec![(None, &empty)],
    };
    let mut out = Vec::new();
    for step in &workflow.steps {
        let Some(carrier) = carrier_of(step) else {
            continue;
        };
        let location = format!("{}/{}", workflow.workflow_id, step.step_id);
        let refusal = expects_refusal(step);
        let explained = step
            .description
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty());
        for (row, inputs) in &runs {
            let Some(query) = query_of(step, carrier, inputs) else {
                continue;
            };
            let at = match row {
                Some(index) => format!("case `{case_id}` row {index}"),
                None => format!("case `{case_id}`"),
            };
            match parse_str(&query) {
                Err(error) if !refusal => out.push(AqlFinding {
                    location: location.clone(),
                    message: format!(
                        "{at}: the AQL query does not parse ({}) and the step does not expect `$statusCode == 400`: {query:?}",
                        position(&error)
                    ),
                }),
                Ok(_) if refusal && !explained => out.push(AqlFinding {
                    location: location.clone(),
                    message: format!(
                        "{at}: the step expects `$statusCode == 400` while its AQL query parses, and its `description` names nothing else that makes the request invalid: {query:?}"
                    ),
                }),
                _ => {}
            }
        }
    }
    out
}

/// The carrier of the operation `step` calls, when it carries AQL.
fn carrier_of(step: &Step) -> Option<Carrier> {
    let (_, operation_id) = split_qualified(step.operation_id.as_deref()?)?;
    CARRIERS
        .iter()
        .find(|(id, _)| *id == operation_id)
        .map(|(_, carrier)| *carrier)
}

/// Whether a success criterion of `step` is the plain `$statusCode == 400`.
fn expects_refusal(step: &Step) -> bool {
    step.success_criteria.iter().any(|criterion| {
        matches!(
            CriterionPlan::of(criterion),
            Ok(CriterionPlan::Simple(SimpleCondition {
                left: Operand::Expr(Expr::StatusCode),
                op: CmpOp::Eq,
                right: Operand::Integer(400),
            }))
        )
    })
}

/// The query `step` sends with `inputs`, or `None` when the catalogue does
/// not determine it.
fn query_of(step: &Step, carrier: Carrier, inputs: &BTreeMap<String, Value>) -> Option<String> {
    let outputs = BTreeMap::new();
    let scope = Scope {
        inputs,
        outputs: &outputs,
        workflows: &outputs,
        exchange: None,
    };
    match carrier {
        Carrier::Parameter(name) => parameter(step, name, &scope),
        Carrier::BodyMember(pointer) => body(step, &scope, pointer)?.pointer(pointer).map(text_of),
        Carrier::TextBody => {
            let language = parameter(step, "query_type", &scope);
            if language.as_deref().is_some_and(|l| l != AQL) {
                return None;
            }
            let media = step.request_body.as_ref()?.content_type.as_deref();
            if media.is_some_and(|m| !is_text_plain(m)) {
                return None;
            }
            body(step, &scope, "").as_ref().map(text_of)
        }
    }
}

/// Whether `media` names `text/plain`, the only request body the stores
/// declare (`definition-validation.openapi.yaml`, the `put` operations of
/// `/definition/query/{qualified_query_name}` and its `{version}` path); a
/// body sent in another media type is not AQL text but the payload of a
/// request the server refuses for its format.
fn is_text_plain(media: &str) -> bool {
    media
        .split(';')
        .next()
        .is_some_and(|essence| essence.trim().eq_ignore_ascii_case("text/plain"))
}

/// The text a value is sent as.
fn text_of(value: &Value) -> String {
    value_text(value)
}

/// The value of the query parameter `name`, as the evaluator sends it.
fn parameter(step: &Step, name: &str, scope: &Scope<'_>) -> Option<String> {
    let parameter = step
        .parameters
        .iter()
        .find_map(|parameter| match parameter {
            ReusableOr::Item(item)
                if item.name == name && item.in_ == Some(ParameterLocation::Query) =>
            {
                Some(item)
            }
            _ => None,
        })?;
    // NOTE: no openEHR spec governs this — our own design; a value the catalogue does not determine
    // (an earlier response, a missing input) is the evaluator's and the other gates' to refuse.
    match &parameter.value {
        ValueOrSelector::Literal(Value::String(text)) if text.starts_with('$') => {
            let expr = Expr::parse(text).ok()?;
            scope.eval(&expr).ok().flatten().as_ref().map(text_of)
        }
        ValueOrSelector::Literal(Value::String(text)) => scope
            .substitute(text, str::to_owned)
            .ok()
            .or_else(|| placeholders(text)),
        ValueOrSelector::Literal(other) => Some(other.to_string()),
        ValueOrSelector::Selector(_) => None,
    }
}

/// The value an embedded `{$…}` expression stands for while the gate parses
/// a query whose expression the catalogue does not determine, such as the
/// identifier an earlier step was answered with.
const PLACEHOLDER: &str = "00000000-0000-4000-8000-000000000000";

/// `text` with every embedded `{$…}` expression replaced by [`PLACEHOLDER`];
/// `None` when an embedded expression is unterminated or does not parse.
fn placeholders(text: &str) -> Option<String> {
    // NOTE: no openEHR spec governs this — our own design; the gate checks the AQL the
    // catalogue wrote, so a runtime value stands in as a placeholder, never as AQL.
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("{$") {
        let (before, tail) = rest.split_at(start);
        out.push_str(before);
        let inner_and_after = tail.get(1..)?;
        let end = inner_and_after.find('}')?;
        let (inner, after) = inner_and_after.split_at(end);
        Expr::parse(inner).ok()?;
        out.push_str(PLACEHOLDER);
        rest = after.get(1..)?;
    }
    out.push_str(rest);
    Some(out)
}

/// The request body, with every replacement applied as the evaluator does;
/// `None` when a replacement the catalogue does not determine touches the
/// query at `carrier` (`""` for the whole body).
///
/// A replacement elsewhere in the body, such as a query parameter taken from
/// an earlier response, leaves the query text the catalogue wrote, so it is
/// skipped and the query is still checked.
fn body(step: &Step, scope: &Scope<'_>, carrier: &str) -> Option<Value> {
    let request_body = step.request_body.as_ref()?;
    let mut payload = match request_body.payload.clone()? {
        Value::String(text) if !text.starts_with('$') => {
            Value::String(scope.substitute(&text, str::to_owned).ok()?)
        }
        other => other,
    };
    for replacement in &request_body.replacements {
        let touches_query = overlaps(&replacement.target, carrier);
        // NOTE: no openEHR spec governs this — our own design; an undetermined value is not
        // catalogue text, and a malformed replacement is the profile gate's to refuse.
        let value = match &replacement.value {
            ValueOrSelector::Literal(Value::String(text)) if text.starts_with('$') => {
                let value = Expr::parse(text)
                    .ok()
                    .and_then(|expr| scope.eval(&expr).ok().flatten());
                match value {
                    Some(value) => value,
                    None if touches_query => return None,
                    None => continue,
                }
            }
            ValueOrSelector::Literal(literal) => literal.clone(),
            ValueOrSelector::Selector(_) if touches_query => return None,
            ValueOrSelector::Selector(_) => continue,
        };
        *payload.pointer_mut(&replacement.target)? = value;
    }
    Some(payload)
}

/// Whether the JSON Pointers `a` and `b` (RFC 6901) name the same member, or
/// one names a member inside the other; `""` names the whole document.
fn overlaps(a: &str, b: &str) -> bool {
    let inside = |outer: &str, inner: &str| {
        inner
            .strip_prefix(outer)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    };
    inside(a, b) || inside(b, a)
}

/// Where the parser refused, in token indices and source bytes.
fn position(error: &ParseError) -> String {
    match error {
        ParseError::Lex(lex) => format!(
            "no token at bytes {}..{}, {:?}",
            lex.span.start, lex.span.end, lex.slice
        ),
        ParseError::Syntax { faults } => faults
            .iter()
            .map(|fault| match &fault.bytes {
                Some(bytes) => format!("{fault}, bytes {}..{}", bytes.start, bytes.end),
                None => fault.to_string(),
            })
            .collect::<Vec<_>>()
            .join("; "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn workflow(steps: &Value) -> Workflow {
        serde_json::from_value(json!({
            "workflowId": "w",
            "inputs": { "type": "object", "properties": { "q": { "type": "string" } } },
            "steps": steps
        }))
        .unwrap()
    }

    fn get_step(q: &str, status: u16) -> Value {
        json!({
            "stepId": "s",
            "operationId": "$sourceDescriptions.query.query_execute_adhoc_query",
            "parameters": [{ "name": "q", "in": "query", "value": q }],
            "successCriteria": [{ "condition": format!("$statusCode == {status}") }]
        })
    }

    fn rows(queries: &[&str]) -> RowsFile {
        RowsFile {
            workflow: "query/w".to_owned(),
            iteration: crate::vocab::Iteration::ResetPerRow,
            literals: Vec::new(),
            recipe: None,
            declared: BTreeMap::new(),
            server: None,
            aggregate: Vec::new(),
            rows: queries
                .iter()
                .map(|q| BTreeMap::from([("q".to_owned(), json!(q))]))
                .collect(),
        }
    }

    const VALID: &str = "SELECT e/ehr_id/value FROM EHR e";
    const INVALID: &str = "SELECT e/ehr_id/value EHR e";

    #[test]
    fn an_embedded_runtime_value_is_parsed_as_a_placeholder() {
        let pinned = "SELECT e/ehr_id/value FROM EHR e WHERE e/ehr_id/value = '{$steps.create.outputs.ehr_id}'";
        let found = check_workflow("query/w", &workflow(&json!([get_step(pinned, 200)])), None);
        assert_eq!(found, Vec::new());
        let broken =
            "SELECT e/ehr_id/value EHR e WHERE e/ehr_id/value = '{$steps.create.outputs.ehr_id}'";
        let found = check_workflow("query/w", &workflow(&json!([get_step(broken, 200)])), None);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(placeholders("x {$not an expression} y"), None);
    }

    #[test]
    fn a_literal_query_that_parses_passes() {
        let found = check_workflow("query/w", &workflow(&json!([get_step(VALID, 200)])), None);
        assert_eq!(found, Vec::new());
    }

    #[test]
    fn a_query_that_does_not_parse_is_refused_with_its_position() {
        let found = check_workflow("query/w", &workflow(&json!([get_step(INVALID, 200)])), None);
        let [finding] = found.as_slice() else {
            panic!("{found:?}");
        };
        assert_eq!(finding.location, "w/s");
        assert!(finding.message.contains("case `query/w`"), "{finding:?}");
        assert!(finding.message.contains("bytes 22..25"), "{finding:?}");
        assert!(finding.message.contains(INVALID), "{finding:?}");
    }

    #[test]
    fn rows_feed_the_query_and_each_row_is_judged() {
        let wf = workflow(&json!([get_step("$inputs.q", 200)]));
        let found = check_workflow("query/w", &wf, Some(&rows(&[VALID, INVALID, "SELECT |"])));
        let messages: Vec<&str> = found.iter().map(|f| f.message.as_str()).collect();
        assert_eq!(messages.len(), 2, "{messages:?}");
        assert!(messages[0].contains("row 1"), "{messages:?}");
        assert!(messages[1].contains("row 2") && messages[1].contains("no token at bytes 7..8"));
    }

    #[test]
    fn a_refusal_step_may_carry_an_unparsable_query_and_must_explain_a_parsable_one() {
        let found = check_workflow("query/w", &workflow(&json!([get_step(INVALID, 400)])), None);
        assert_eq!(found, Vec::new());
        let found = check_workflow("query/w", &workflow(&json!([get_step(VALID, 400)])), None);
        assert!(
            matches!(found.as_slice(), [f] if f.message.contains("names nothing else")),
            "{found:?}"
        );
        let mut explained = get_step(VALID, 400);
        explained["description"] = json!("`fetch` is negative");
        let found = check_workflow("query/w", &workflow(&json!([explained])), None);
        assert_eq!(found, Vec::new());
    }

    #[test]
    fn body_members_replacements_and_text_bodies_are_read() {
        let post = json!({
            "stepId": "post",
            "operationId": "$sourceDescriptions.query.query_execute_adhoc_query_body",
            "requestBody": {
                "contentType": "application/json",
                "payload": { "q": VALID },
                "replacements": [{ "target": "/q", "value": "$inputs.q" }]
            },
            "successCriteria": [{ "condition": "$statusCode == 200" }]
        });
        let store = json!({
            "stepId": "store",
            "operationId": "$sourceDescriptions.definition.definition_query_store.yaml",
            "parameters": [{ "name": "qualified_query_name", "in": "path", "value": "org::q" }],
            "requestBody": { "contentType": "text/plain", "payload": INVALID },
            "successCriteria": [{ "condition": "$statusCode == 200" }]
        });
        let found = check_workflow(
            "query/w",
            &workflow(&json!([post, store])),
            Some(&rows(&[INVALID])),
        );
        let at: Vec<&str> = found.iter().map(|f| f.location.as_str()).collect();
        assert_eq!(at, ["w/post", "w/store"]);
    }

    #[test]
    fn pointers_overlap_only_by_whole_members() {
        assert!(overlaps("/q", "/q"));
        assert!(overlaps("", "/q"));
        assert!(overlaps("/q/0", "/q"));
        assert!(!overlaps("/query_parameters/ehr_id", "/q"));
        assert!(!overlaps("/q", "/query_parameters"));
    }

    #[test]
    fn a_runtime_replacement_beside_the_query_leaves_it_checked() {
        let post = |q: &str, target: &str| {
            json!({
                "stepId": "post",
                "operationId": "$sourceDescriptions.query.query_execute_adhoc_query_body",
                "requestBody": {
                    "contentType": "application/json",
                    "payload": { "q": q, "query_parameters": { "ehr_id": "x" } },
                    "replacements": [{ "target": target, "value": "$workflows.setup.outputs.ehr_id" }]
                },
                "successCriteria": [{ "condition": "$statusCode == 200" }]
            })
        };
        let beside = post(INVALID, "/query_parameters/ehr_id");
        let found = check_workflow("query/w", &workflow(&json!([beside])), None);
        assert_eq!(found.len(), 1, "{found:?}");
        let over = post(INVALID, "/q");
        let found = check_workflow("query/w", &workflow(&json!([over])), None);
        assert_eq!(found, Vec::new());
    }

    #[test]
    fn another_query_language_and_a_runtime_value_are_not_aql_text() {
        let store = json!({
            "stepId": "store",
            "operationId": "$sourceDescriptions.definition.definition_query_store.yaml",
            "parameters": [
                { "name": "qualified_query_name", "in": "path", "value": "org::q" },
                { "name": "query_type", "in": "query", "value": "OTHER" }
            ],
            "requestBody": { "contentType": "text/plain", "payload": INVALID },
            "successCriteria": [{ "condition": "$statusCode == 200" }]
        });
        let earlier = get_step("$steps.list.outputs.q", 200);
        let found = check_workflow("query/w", &workflow(&json!([store, earlier])), None);
        assert_eq!(found, Vec::new());
    }

    #[test]
    fn a_store_body_in_another_media_type_is_not_aql_text() {
        let store = |media: &str| {
            json!({
                "stepId": "store",
                "operationId": "$sourceDescriptions.definition.definition_query_store.yaml",
                "parameters": [{ "name": "qualified_query_name", "in": "path", "value": "org::q" }],
                "requestBody": { "contentType": media, "payload": { "q": VALID } },
                "successCriteria": [{ "condition": "$statusCode == 415" }]
            })
        };
        let json_body = check_workflow(
            "query/w",
            &workflow(&json!([store("application/json")])),
            None,
        );
        assert_eq!(json_body, Vec::new());
        let text_body = check_workflow(
            "query/w",
            &workflow(&json!([store("text/plain; charset=utf-8")])),
            None,
        );
        assert_eq!(text_body.len(), 1, "{text_body:?}");
        assert!(is_text_plain("Text/Plain"));
        assert!(!is_text_plain("application/json"));
    }

    #[test]
    fn an_operation_without_aql_is_not_read() {
        let step = json!({
            "stepId": "s",
            "operationId": "$sourceDescriptions.ehr.ehr_get_by_id",
            "parameters": [{ "name": "q", "in": "query", "value": INVALID }],
            "successCriteria": [{ "condition": "$statusCode == 200" }]
        });
        assert_eq!(
            check_workflow("ehr/w", &workflow(&json!([step])), None),
            Vec::new()
        );
    }
}
