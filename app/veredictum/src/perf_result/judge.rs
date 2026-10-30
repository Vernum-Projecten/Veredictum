// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Re-judging a `perf-result`: derive again from the raw part and compare,
//! field by field and byte by byte.

#![expect(
    clippy::disallowed_types,
    reason = "the judge walks the recorded and re-derived documents as JSON trees to name the \
              pointer of every difference"
)]

use serde_json::Value;

use crate::perf_result::{PerfResult, PerfResultError, derive};
use crate::schema::perf_result_schema;

/// One derived field whose recorded value differs from its re-derivation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    /// The field's JSON pointer in the record.
    pub pointer: String,
    /// The recorded value, as JSON text; absent when the record lacks it.
    pub recorded: Option<String>,
    /// The re-derived value, as JSON text; absent when re-derivation has no
    /// such field.
    pub rederived: Option<String>,
}

/// The outcome of re-judging one record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejudgement {
    /// Every derived field that differs, in document order.
    pub mismatches: Vec<Mismatch>,
    /// Whether the file is byte for byte the canonical rendering of its raw
    /// part and the re-derived part.
    pub canonical: bool,
    /// The instrument version that wrote the record.
    pub recorded_version: String,
}

impl Rejudgement {
    /// Whether the record reproduces byte for byte.
    #[must_use]
    pub fn reproduces(&self) -> bool {
        self.mismatches.is_empty() && self.canonical
    }
}

/// Escapes one JSON pointer reference token (RFC 6901 §3).
fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

fn diff(
    pointer: &str,
    recorded: Option<&Value>,
    rederived: Option<&Value>,
    out: &mut Vec<Mismatch>,
) {
    match (recorded, rederived) {
        (Some(Value::Object(a)), Some(Value::Object(b))) => {
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                diff(
                    &format!("{pointer}/{}", escape(key)),
                    a.get(key),
                    b.get(key),
                    out,
                );
            }
        }
        (Some(Value::Array(a)), Some(Value::Array(b))) => {
            for index in 0..a.len().max(b.len()) {
                diff(
                    &format!("{pointer}/{index}"),
                    a.get(index),
                    b.get(index),
                    out,
                );
            }
        }
        (a, b) if a == b => {}
        (a, b) => out.push(Mismatch {
            pointer: pointer.to_owned(),
            recorded: a.map(Value::to_string),
            rederived: b.map(Value::to_string),
        }),
    }
}

/// Re-judges the record in `text`.
///
/// The document is checked against the published schema, read, derived
/// again from its raw part, and compared: every derived field that differs
/// is named by its JSON pointer, and the file must equal the canonical
/// rendering of the re-derived record byte for byte.
///
/// # Errors
/// [`PerfResultError::Json`] for text that is not JSON,
/// [`PerfResultError::Schema`] for a document outside the schema, and every
/// error of [`derive::derive`] for a raw part that does not re-derive.
pub fn rejudge(text: &str) -> Result<Rejudgement, PerfResultError> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| PerfResultError::Json(e.to_string()))?;
    let validator = jsonschema::validator_for(&perf_result_schema())
        .map_err(|e| PerfResultError::Schema(format!("the schema does not compile: {e}")))?;
    let violations: Vec<String> = validator
        .iter_errors(&value)
        .map(|e| format!("{}: {e}", e.instance_path()))
        .collect();
    if !violations.is_empty() {
        return Err(PerfResultError::Schema(violations.join("; ")));
    }
    let recorded: PerfResult =
        serde_json::from_value(value).map_err(|e| PerfResultError::Json(e.to_string()))?;
    let rederived = PerfResult {
        family: recorded.family,
        raw: recorded.raw.clone(),
        derived: derive::derive(&recorded.raw)?,
    };
    let as_value = |derived: &derive::Derived| {
        serde_json::to_value(derived).map_err(|e| PerfResultError::Serialize(e.to_string()))
    };
    let mut mismatches = Vec::new();
    diff(
        "/derived",
        Some(&as_value(&recorded.derived)?),
        Some(&as_value(&rederived.derived)?),
        &mut mismatches,
    );
    Ok(Rejudgement {
        mismatches,
        canonical: rederived.render()? == text,
        recorded_version: recorded.raw.instrument.version,
    })
}
