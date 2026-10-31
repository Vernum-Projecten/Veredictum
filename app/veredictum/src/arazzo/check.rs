// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `x-veredictum-check` extension: the judges no standard Arazzo
//! criterion can express, each a thin adapter over the engine's existing
//! judge.
//!
//! - `schema` validates the step's response against the schema its operation
//!   declares in the `-validation` OAS ([`crate::arazzo::oas`]). It runs only
//!   where a case asks for it, on a criterion that cites the OAS element.
//! - `result_set` wraps [`crate::exec::assertions::eval_result_set_against`],
//!   the `RESULT_SET` bag, order, cell and count rules.
//! - `equivalent` wraps [`crate::exec::assertions::equivalent`] and its
//!   diagnostic [`crate::exec::assertions::equivalence_mismatch`], with the
//!   ignore paths [`crate::exec::assertions::strip_ignored`] reads.
//! - `signature` wraps [`crate::exec::assertions::eval_signature`], the
//!   `ORIGINAL_VERSION.signature` facts.
//! - `json_type` decides which of the six JSON types of RFC 8259 §3 a value
//!   is, which no `simple`, `regex` or RFC 9535 criterion can assert.
//!
//! A value that is a string starting with `$` is a runtime expression and is
//! read from the run; any other value is a literal. A judge that does not hold
//! is a [`Judgement`] that fails the row; a judge that cannot decide is an
//! [`EvalError::Unjudgeable`] that makes the row errored. No openEHR spec
//! governs the extension's shape — our own design.

use serde::Deserialize;
use serde_json::Value;

use crate::arazzo::expr::{EvalError, Expr, Judgement, ParseError, Scope, value_text};
use crate::arazzo::oas::{ResponseSchemas, SchemaVerdict, is_json_media};
use crate::exec::assertions::{
    self, AssertionOutcome, ResolvedResultSet, eval_result_set_against, eval_signature,
};
use crate::exec::signature::SigningMode;
use crate::model::assertion::ColumnSpec;
use crate::vocab::{CellComparison, ResultSetMatch};

/// One `x-veredictum-check` value, tagged by `judge`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "judge", rename_all = "snake_case", deny_unknown_fields)]
pub enum Check {
    /// AQL `RESULT_SET` comparison (QUERY `master03-syntax.adoc` §DISTINCT for
    /// the bag rule, §ORDER BY for the order rule).
    ResultSet {
        /// The runtime expression that yields the served result set.
        context: String,
        /// How the expected rows are compared.
        #[serde(rename = "match")]
        match_mode: ResultSetMatch,
        /// The expected rows: a literal array of rows or a runtime
        /// expression. In a literal array, a cell that is a string starting
        /// with `$` is a runtime expression too, such as the identifier an
        /// earlier step was answered with.
        #[serde(default)]
        rows: Option<Value>,
        /// The exact row count, for `match: count`.
        #[serde(default)]
        count: Option<u64>,
        /// The expected column names, in order.
        #[serde(default)]
        columns: Option<Vec<String>>,
        /// How a scalar cell is compared; the exact lexeme when absent.
        #[serde(default)]
        cells: Option<CellComparison>,
    },
    /// Two RM documents compared while ignoring named paths.
    Equivalent {
        /// The runtime expression that yields the served document.
        context: String,
        /// The document it must equal: a literal or a runtime expression.
        to: Value,
        /// RM paths ignored on both sides (`**` matches any depth).
        #[serde(default)]
        ignoring: Vec<String>,
    },
    /// `ORIGINAL_VERSION.signature` facts (RM common
    /// `master06-change_control_package.adoc` §Digital Signature).
    Signature {
        /// The runtime expression that yields the served version envelope.
        context: String,
        /// The envelope carries a non-empty signature.
        #[serde(default)]
        present: Option<bool>,
        /// The signature verifies under the declared signing posture, over
        /// the RFC 8785 canonical form [`crate::exec::signature`] fixes; no
        /// openEHR spec governs this — our own design.
        #[serde(default)]
        verifiable: Option<bool>,
        /// The stored signature equals this value, verbatim.
        #[serde(default)]
        equals: Option<String>,
        /// The stored signature differs from this value.
        #[serde(default)]
        distinct_from: Option<String>,
    },
    /// The step's response body validates against the schema its operation
    /// declares for the served status and media type.
    Schema {
        /// Which declared schema: today only the `-validation` OAS.
        against: SchemaSource,
    },
    /// The value at a runtime expression is of one JSON type.
    JsonType {
        /// The runtime expression that yields the judged value.
        context: String,
        /// The JSON type it must be.
        #[serde(rename = "type")]
        json_type: JsonType,
    },
}

/// The type of a JSON value: the four primitive and the two structured types
/// of RFC 8259 §3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JsonType {
    /// An object.
    Object,
    /// An array.
    Array,
    /// A string.
    String,
    /// A number.
    Number,
    /// `true` or `false`.
    Boolean,
    /// `null`.
    Null,
}

impl JsonType {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[Self] = &[
        Self::Object,
        Self::Array,
        Self::String,
        Self::Number,
        Self::Boolean,
        Self::Null,
    ];

    /// The type of `value`.
    #[must_use]
    pub fn of(value: &Value) -> Self {
        match value {
            Value::Object(_) => Self::Object,
            Value::Array(_) => Self::Array,
            Value::String(_) => Self::String,
            Value::Number(_) => Self::Number,
            Value::Bool(_) => Self::Boolean,
            Value::Null => Self::Null,
        }
    }

    /// The token a check spells.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Object => "object",
            Self::Array => "array",
            Self::String => "string",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Null => "null",
        }
    }
}

/// The `json_type` judgement of the value `served` read from `context`.
///
/// An absent value does not hold, while a present `null` is of the type
/// `null`.
fn json_type_judgement(context: &Expr, served: Option<&Value>, wanted: JsonType) -> Judgement {
    let expected = format!("{context} is a JSON {}", wanted.token());
    match served {
        None => Judgement {
            holds: false,
            expected,
            received: format!("{context} is absent"),
        },
        Some(value) => {
            let actual = JsonType::of(value);
            Judgement {
                holds: actual == wanted,
                expected,
                received: format!("{context} is a JSON {}", actual.token()),
            }
        }
    }
}

/// The schema a `schema` judge validates against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaSource {
    /// The schema the step's operation declares in its `-validation` OAS
    /// source.
    Oas,
}

impl SchemaSource {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[Self] = &[Self::Oas];
}

/// What a judge reads beyond the scope of its criterion.
#[derive(Debug)]
pub struct JudgeEnv<'a> {
    /// The deployment's declared signing posture, which `signature`'s
    /// `verifiable` reads.
    pub signing: Option<&'a SigningMode>,
    /// The response schemas of the document's sources, which `schema` reads.
    pub schemas: Option<&'a mut ResponseSchemas>,
    /// The step's qualified `operationId`.
    pub operation: Option<&'a str>,
}

impl Check {
    /// Parses an extension value and checks every runtime expression in it
    /// parses.
    ///
    /// # Errors
    /// [`ParseError::Check`] for a value that is not one of the judges, and
    /// the expression's [`ParseError`] for an expression that does not parse.
    pub fn parse(value: &Value) -> Result<Self, ParseError> {
        let check: Self =
            serde_json::from_value(value.clone()).map_err(|e| ParseError::Check(e.to_string()))?;
        if let Some(context) = check.context() {
            Expr::parse(context)?;
        }
        match &check {
            Self::Schema { .. } | Self::JsonType { .. } => {}
            Self::ResultSet {
                match_mode,
                rows,
                count,
                ..
            } => {
                let complete = match match_mode {
                    ResultSetMatch::Count => count.is_some(),
                    _ => rows.is_some(),
                };
                if !complete {
                    return Err(ParseError::Check(format!(
                        "`match: {}` needs `{}`",
                        match_mode_token(*match_mode),
                        if *match_mode == ResultSetMatch::Count {
                            "count"
                        } else {
                            "rows"
                        }
                    )));
                }
                if let Some(rows) = rows {
                    expression_of(rows).transpose()?;
                    if let Value::Array(literal) = rows {
                        for row in literal {
                            for cell in row.as_array().into_iter().flatten() {
                                expression_of(cell).transpose()?;
                            }
                        }
                    }
                }
            }
            Self::Equivalent { to, .. } => {
                expression_of(to).transpose()?;
            }
            Self::Signature {
                present,
                verifiable,
                equals,
                distinct_from,
                ..
            } => {
                if present.is_none()
                    && verifiable.is_none()
                    && equals.is_none()
                    && distinct_from.is_none()
                {
                    return Err(ParseError::Check(
                        "a signature check names no fact".to_owned(),
                    ));
                }
                for text in [equals, distinct_from].into_iter().flatten() {
                    if text.starts_with('$') {
                        Expr::parse(text)?;
                    }
                }
            }
        }
        Ok(check)
    }

    /// The runtime expression the judged value is read from; `None` for the
    /// `schema` judge, which always reads the step's response.
    #[must_use]
    pub fn context(&self) -> Option<&str> {
        match self {
            Self::ResultSet { context, .. }
            | Self::Equivalent { context, .. }
            | Self::Signature { context, .. }
            | Self::JsonType { context, .. } => Some(context),
            Self::Schema { .. } => None,
        }
    }

    /// Judges the check in `scope`, reading the signing posture and the
    /// response schemas from `env`.
    ///
    /// # Errors
    /// [`EvalError`] when a value cannot be read or the judge cannot decide.
    pub fn judge(&self, scope: &Scope<'_>, env: &mut JudgeEnv<'_>) -> Result<Judgement, EvalError> {
        let Some(context) = self.context() else {
            return judge_schema(scope, env);
        };
        let signing = env.signing;
        let context = Expr::parse(context)?;
        if let Self::JsonType { json_type, .. } = self {
            let served = scope.eval(&context)?;
            return Ok(json_type_judgement(&context, served.as_ref(), *json_type));
        }
        let Some(served) = scope.eval(&context)?.filter(|v| !v.is_null()) else {
            return Ok(Judgement {
                holds: false,
                expected: format!("{context} is present for the {} judge", self.name()),
                received: format!("{context} is absent"),
            });
        };
        match self {
            Self::ResultSet {
                match_mode,
                rows,
                count,
                columns,
                cells,
                ..
            } => {
                let rows = rows.as_ref().map(|v| resolve_rows(v, scope)).transpose()?;
                let rows = match &rows {
                    None => None,
                    Some(Value::Array(rows)) => Some(rows.as_slice()),
                    Some(other) => {
                        return Err(EvalError::Unjudgeable(format!(
                            "result_set: the expected rows are not an array: {other}"
                        )));
                    }
                };
                let columns = columns.as_deref().map(column_specs);
                let expected = format!(
                    "{context} matches the expected result set (match: {})",
                    match_mode_token(*match_mode)
                );
                let verdict = eval_result_set_against(
                    &served,
                    ResolvedResultSet {
                        match_mode: *match_mode,
                        rows,
                        count: *count,
                        columns: columns.as_deref(),
                        cells: cells.unwrap_or_default(),
                    },
                );
                Ok(match verdict {
                    Ok(_tolerated) => Judgement {
                        holds: true,
                        expected,
                        received: "the result set matches".to_owned(),
                    },
                    Err(failure) => Judgement {
                        holds: false,
                        expected,
                        received: failure.0,
                    },
                })
            }
            Self::Equivalent { to, ignoring, .. } => {
                let want = resolve(to, scope)?;
                Ok(equivalence(&context, &served, to, &want, ignoring))
            }
            Self::Signature {
                present,
                verifiable,
                equals,
                distinct_from,
                ..
            } => {
                let equals = equals.as_deref().map(|t| text_of(t, scope)).transpose()?;
                let distinct_from = distinct_from
                    .as_deref()
                    .map(|t| text_of(t, scope))
                    .transpose()?;
                let expected = format!("{context} carries the declared signature facts");
                signature_judgement(
                    expected,
                    eval_signature(
                        &served,
                        *present,
                        *verifiable,
                        equals.as_deref(),
                        distinct_from.as_deref(),
                        signing,
                    ),
                )
            }
            Self::Schema { .. } => judge_schema(scope, env),
            Self::JsonType { json_type, .. } => {
                Ok(json_type_judgement(&context, Some(&served), *json_type))
            }
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::ResultSet { .. } => "result_set",
            Self::Equivalent { .. } => "equivalent",
            Self::Signature { .. } => "signature",
            Self::Schema { .. } => "schema",
            Self::JsonType { .. } => "json_type",
        }
    }
}

/// The `signature` judgement of the engine's verdict.
fn signature_judgement(
    expected: String,
    verdict: Result<(), AssertionOutcome>,
) -> Result<Judgement, EvalError> {
    match verdict {
        Ok(()) => Ok(Judgement {
            holds: true,
            expected,
            received: "every signature fact holds".to_owned(),
        }),
        Err(AssertionOutcome::Mismatch(reason)) => Ok(Judgement {
            holds: false,
            expected,
            received: reason,
        }),
        Err(AssertionOutcome::Unjudgeable(reason)) => Err(EvalError::Unjudgeable(reason)),
    }
}

/// How many violations a failed `schema` judge shows.
const SHOWN_VIOLATIONS: usize = 5;

/// The `schema` judge: the step's response body against the schema its
/// operation declares for the served status and media type.
fn judge_schema(scope: &Scope<'_>, env: &mut JudgeEnv<'_>) -> Result<Judgement, EvalError> {
    let (_, response) = scope
        .exchange
        .ok_or_else(|| EvalError::NoExchange("the schema judge".to_owned()))?;
    let (Some(schemas), Some(operation)) = (env.schemas.as_deref_mut(), env.operation) else {
        return Err(EvalError::Unjudgeable(
            "schema: the step names no operation whose source is loaded".to_owned(),
        ));
    };
    let Some(content_type) = response.headers.get("content-type") else {
        return Err(EvalError::Unjudgeable(
            "schema: the response names no media type".to_owned(),
        ));
    };
    if !is_json_media(content_type) {
        return Err(EvalError::Unjudgeable(format!(
            "schema: a {content_type} body is not validated against a JSON schema"
        )));
    }
    let body = match &response.body {
        Err(reason) => return Err(EvalError::UnreadableBody(reason.clone())),
        Ok(None) => {
            return Err(EvalError::Unjudgeable(
                "schema: the response has no body to validate".to_owned(),
            ));
        }
        Ok(Some(body)) => body,
    };
    let verdict = schemas
        .validate(operation, response.status, Some(content_type), Some(body))
        .map_err(|e| EvalError::Unjudgeable(format!("schema: {e}")))?;
    match verdict {
        SchemaVerdict::NotDeclared => Err(EvalError::Unjudgeable(format!(
            "schema: the operation declares no schema for status {} and {content_type}",
            response.status
        ))),
        SchemaVerdict::Valid => Ok(Judgement {
            holds: true,
            expected: "the body validates against its declared schema".to_owned(),
            received: "the body validates".to_owned(),
        }),
        SchemaVerdict::Invalid {
            citation,
            pointer,
            errors,
        } => {
            let mut received = errors
                .iter()
                .take(SHOWN_VIOLATIONS)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ");
            if errors.len() > SHOWN_VIOLATIONS {
                let more = errors.len().saturating_sub(SHOWN_VIOLATIONS);
                received = format!("{received} … and {more} more");
            }
            Ok(Judgement {
                holds: false,
                expected: format!("the body validates against {} {pointer}", citation.source),
                received,
            })
        }
    }
}

/// The engine's column expectations for a list of column names.
fn column_specs(names: &[String]) -> Vec<ColumnSpec> {
    names
        .iter()
        .map(|name| ColumnSpec { name: name.clone() })
        .collect()
}

/// The `equivalent` judgement of `served` against `want`.
fn equivalence(
    context: &Expr,
    served: &Value,
    to: &Value,
    want: &Value,
    ignoring: &[String],
) -> Judgement {
    let expected = if ignoring.is_empty() {
        format!("{context} is equivalent to {}", operand_name(to))
    } else {
        format!(
            "{context} is equivalent to {}, ignoring {}",
            operand_name(to),
            ignoring.join(", ")
        )
    };
    if assertions::equivalent(served, want, ignoring) {
        return Judgement {
            holds: true,
            expected,
            received: "the documents are equivalent".to_owned(),
        };
    }
    let diff = assertions::equivalence_mismatch(
        &assertions::strip_ignored(served, ignoring),
        &assertions::strip_ignored(want, ignoring),
    );
    Judgement {
        holds: false,
        expected,
        received: diff.0,
    }
}

fn match_mode_token(mode: ResultSetMatch) -> &'static str {
    match mode {
        ResultSetMatch::Ordered => "ordered",
        ResultSetMatch::Set => "set",
        ResultSetMatch::Count => "count",
        ResultSetMatch::Contains => "contains",
    }
}

/// The runtime expression a value carries, when it is one.
fn expression_of(value: &Value) -> Option<Result<Expr, ParseError>> {
    match value {
        Value::String(text) if text.starts_with('$') => Some(Expr::parse(text)),
        _ => None,
    }
}

/// How an operand is named in an explanation.
fn operand_name(value: &Value) -> String {
    match value {
        Value::String(text) if text.starts_with('$') => text.clone(),
        _ => "the authored document".to_owned(),
    }
}

/// A value read from the run, or the literal itself.
fn resolve(value: &Value, scope: &Scope<'_>) -> Result<Value, EvalError> {
    match expression_of(value) {
        None => Ok(value.clone()),
        Some(expr) => {
            let expr = expr?;
            scope
                .eval(&expr)?
                .ok_or_else(|| EvalError::MissingOutput(expr.to_string()))
        }
    }
}

/// The expected rows read from the run: the whole value when it is a
/// runtime expression, and otherwise every cell of a literal row that is one.
fn resolve_rows(value: &Value, scope: &Scope<'_>) -> Result<Value, EvalError> {
    match value {
        Value::Array(rows) => rows
            .iter()
            .map(|row| match row {
                Value::Array(cells) => cells
                    .iter()
                    .map(|cell| resolve(cell, scope))
                    .collect::<Result<Vec<_>, _>>()
                    .map(Value::Array),
                other => Ok(other.clone()),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        other => resolve(other, scope),
    }
}

/// A string operand read from the run, or the literal text.
fn text_of(text: &str, scope: &Scope<'_>) -> Result<String, EvalError> {
    resolve(&Value::String(text.to_owned()), scope).map(|value| value_text(&value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arazzo::expr::{RequestView, ResponseView};
    use serde_json::json;
    use std::collections::BTreeMap;

    fn judge(
        check: &Value,
        body: &Value,
        inputs: &[(&str, Value)],
    ) -> Result<Judgement, EvalError> {
        judge_signed(check, body, inputs, None)
    }

    fn judge_signed(
        check: &Value,
        body: &Value,
        inputs: &[(&str, Value)],
        signing: Option<&SigningMode>,
    ) -> Result<Judgement, EvalError> {
        let request = RequestView::default();
        let response = ResponseView {
            status: 200,
            headers: BTreeMap::new(),
            body: Ok(Some(body.clone())),
            text: body.to_string(),
        };
        let inputs: BTreeMap<String, Value> = inputs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect();
        let none = BTreeMap::new();
        Check::parse(check)?.judge(
            &Scope {
                inputs: &inputs,
                outputs: &none,
                workflows: &none,
                exchange: Some((&request, &response)),
            },
            &mut JudgeEnv {
                signing,
                schemas: None,
                operation: None,
            },
        )
    }

    fn result_set(rows: &[&[i64]]) -> Value {
        json!({ "rows": rows, "columns": [{ "name": "n", "path": "/n" }] })
    }

    #[test]
    fn result_set_applies_the_bag_and_order_rules() {
        let served = result_set(&[&[1], &[2], &[2]]);
        let bag = json!({ "judge": "result_set", "context": "$response.body",
                          "match": "set", "rows": [[2], [1], [2]] });
        assert!(judge(&bag, &served, &[]).unwrap().holds);
        let short_bag = json!({ "judge": "result_set", "context": "$response.body",
                                "match": "set", "rows": [[2], [1]] });
        assert!(
            !judge(&short_bag, &served, &[]).unwrap().holds,
            "duplicates count"
        );
        let ordered = json!({ "judge": "result_set", "context": "$response.body",
                              "match": "ordered", "rows": "$inputs.rows" });
        let reversed = Value::from(vec![json!([2]), json!([2]), json!([1])]);
        assert!(
            !judge(&ordered, &served, &[("rows", reversed)])
                .unwrap()
                .holds
        );
        let count = json!({ "judge": "result_set", "context": "$response.body",
                            "match": "count", "count": 3, "columns": ["n"] });
        assert!(judge(&count, &served, &[]).unwrap().holds);
    }

    #[test]
    fn a_result_set_cell_can_be_read_from_the_run() {
        let served = json!({ "rows": [["a::s::1", 1], ["b::s::1", 2]] });
        let check = json!({ "judge": "result_set", "context": "$response.body",
                            "match": "set", "rows": [["$inputs.second", 2], ["$inputs.first", 1]] });
        let inputs = [("first", json!("a::s::1")), ("second", json!("b::s::1"))];
        assert!(judge(&check, &served, &inputs).unwrap().holds);
        let swapped = [("first", json!("b::s::1")), ("second", json!("a::s::1"))];
        assert!(!judge(&check, &served, &swapped).unwrap().holds);
        let malformed = json!({ "judge": "result_set", "context": "$response.body",
                                "match": "set", "rows": [["$not an expression"]] });
        assert!(Check::parse(&malformed).is_err());
    }

    #[test]
    fn equivalent_ignores_the_named_paths() {
        let served =
            json!({ "_type": "EHR_STATUS", "uid": { "value": "x::1" }, "is_queryable": true });
        let check = json!({ "judge": "equivalent", "context": "$response.body",
                            "to": "$inputs.sent", "ignoring": ["uid"] });
        let sent = json!({ "_type": "EHR_STATUS", "is_queryable": true });
        assert!(judge(&check, &served, &[("sent", sent)]).unwrap().holds);
        let changed = json!({ "_type": "EHR_STATUS", "is_queryable": false });
        let judged = judge(&check, &served, &[("sent", changed)]).unwrap();
        assert!(!judged.holds);
        assert!(judged.received.contains("is_queryable"), "{judged:?}");
    }

    #[test]
    fn signature_facts_hold_or_fail_and_an_undeclared_posture_is_unjudgeable() {
        let served = json!({ "signature": "sig-1", "uid": { "value": "v::1" } });
        let check = json!({ "judge": "signature", "context": "$response.body",
                            "present": true, "equals": "$inputs.sig" });
        assert!(
            judge(&check, &served, &[("sig", Value::from("sig-1"))])
                .unwrap()
                .holds
        );
        assert!(
            !judge(&check, &served, &[("sig", Value::from("other"))])
                .unwrap()
                .holds
        );
        let verifiable = json!({ "judge": "signature", "context": "$response.body",
                                 "verifiable": true });
        assert!(matches!(
            judge(&verifiable, &served, &[]),
            Err(EvalError::Unjudgeable(_))
        ));
        let digest = SigningMode::Digest {
            algorithm: "sha256".to_owned(),
            encoding: "hex".to_owned(),
            prefix: String::new(),
        };
        let judged = judge_signed(&verifiable, &served, &[], Some(&digest)).unwrap();
        assert!(!judged.holds, "sig-1 is not the digest of the envelope");
    }

    fn json_type(wanted: &str) -> Value {
        json!({ "judge": "json_type", "context": "$response.body#/v", "type": wanted })
    }

    #[test]
    fn json_type_tells_each_of_the_six_types_apart() {
        let values = [
            ("object", json!({ "a": 1 })),
            ("array", json!(["launch-ehr"])),
            ("string", json!("launch-ehr")),
            ("number", json!(1.5)),
            ("boolean", json!(false)),
            ("null", Value::Null),
        ];
        for (wanted, _) in &values {
            for (actual, value) in &values {
                let judged = judge(&json_type(wanted), &json!({ "v": value }), &[]).unwrap();
                assert_eq!(judged.holds, wanted == actual, "{wanted} against {value}");
                assert_eq!(
                    judged.received,
                    format!("$response.body#/v is a JSON {actual}")
                );
            }
        }
        assert_eq!(
            JsonType::ALL.iter().map(|t| t.token()).collect::<Vec<_>>(),
            values.iter().map(|(t, _)| *t).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_seeded_non_array_capabilities_value_is_refused() {
        let check = json!({ "judge": "json_type", "context": "$response.body#/capabilities",
                            "type": "array" });
        let listed = json!({ "capabilities": ["launch-ehr", "context-openehr-ehr"] });
        assert!(judge(&check, &listed, &[]).unwrap().holds);
        for seeded in [
            json!({ "capabilities": "launch-ehr context-openehr-ehr" }),
            json!({ "capabilities": { "launch-ehr": true } }),
            json!({ "capabilities": null }),
            json!({}),
        ] {
            let judged = judge(&check, &seeded, &[]).unwrap();
            assert!(!judged.holds, "{seeded}");
            assert_eq!(
                judged.expected,
                "$response.body#/capabilities is a JSON array"
            );
        }
    }

    #[test]
    fn a_seeded_unknown_json_type_is_refused() {
        for bad in [
            json_type("integer"),
            json_type("Array"),
            json_type("list"),
            json!({ "judge": "json_type", "context": "$response.body" }),
            json!({ "judge": "json_type", "type": "array" }),
        ] {
            assert!(
                matches!(Check::parse(&bad), Err(ParseError::Check(_))),
                "{bad}"
            );
        }
        assert!(matches!(
            Check::parse(&json!({ "judge": "json_type", "context": "$nope", "type": "array" })),
            Err(ParseError::Expression(_))
        ));
    }

    #[test]
    fn an_absent_context_does_not_hold() {
        let check = json!({ "judge": "equivalent", "context": "$response.body#/missing",
                            "to": { "a": 1 } });
        assert!(!judge(&check, &json!({ "a": 1 }), &[]).unwrap().holds);
    }

    #[test]
    fn a_malformed_check_is_refused() {
        for bad in [
            json!({ "judge": "version", "context": "$response.body" }),
            json!({ "judge": "equivalent", "context": "$response.body", "to": 1, "extra": 1 }),
            json!({ "judge": "result_set", "context": "$response.body", "match": "set" }),
            json!({ "judge": "result_set", "context": "$response.body", "match": "count" }),
            json!({ "judge": "signature", "context": "$response.body" }),
            json!({ "judge": "schema" }),
            json!({ "judge": "schema", "against": "xsd" }),
            json!({ "judge": "schema", "against": "oas", "context": "$response.body" }),
        ] {
            assert!(
                matches!(Check::parse(&bad), Err(ParseError::Check(_))),
                "{bad}"
            );
        }
        assert_eq!(
            Check::parse(&json!({ "judge": "schema", "against": "oas" })),
            Ok(Check::Schema {
                against: SchemaSource::Oas
            })
        );
        let bad_expression = json!({ "judge": "equivalent", "context": "$nope", "to": 1 });
        assert!(matches!(
            Check::parse(&bad_expression),
            Err(ParseError::Expression(_))
        ));
    }
}
