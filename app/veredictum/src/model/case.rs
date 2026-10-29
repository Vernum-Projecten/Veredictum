// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! One cell of a decision-table row, as a rows file's literal columns carry
//! it.

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT"
)]

use serde::{Deserialize, Deserializer};

/// A decision-table cell: a reserved sentinel or a literal.
#[derive(Debug, Clone, PartialEq)]
pub enum MatrixCell {
    /// Omit the field entirely.
    Absent,
    /// Synthesize a valid value via the case's recipe.
    Provided,
    /// JSON null.
    Null,
    /// A literal value (sentinel words are reserved and never literals).
    Literal(serde_json::Value),
}

impl<'de> Deserialize<'de> for MatrixCell {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(match &value {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::String(s) if s == "absent" => Self::Absent,
            serde_json::Value::String(s) if s == "provided" => Self::Provided,
            _ => Self::Literal(value),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::MatrixCell;

    #[test]
    fn the_sentinels_are_reserved_words_and_everything_else_is_a_literal() {
        let cell = |text: &str| serde_json::from_str::<MatrixCell>(text).ok();
        assert_eq!(cell("\"absent\""), Some(MatrixCell::Absent));
        assert_eq!(cell("\"provided\""), Some(MatrixCell::Provided));
        assert_eq!(cell("null"), Some(MatrixCell::Null));
        assert_eq!(
            cell("\"Absent\""),
            Some(MatrixCell::Literal(serde_json::json!("Absent")))
        );
        assert_eq!(cell("12"), Some(MatrixCell::Literal(serde_json::json!(12))));
    }
}
