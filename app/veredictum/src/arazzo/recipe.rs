// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Row recipes: the committed, deterministic code that turns a row's
//! decision-table literals into an input value the workflow sends.
//!
//! A rows file names at most one recipe in its header. The recipe reads the
//! row's literal-typed columns and binds its result to one workflow input,
//! which a step then sends, typically as the whole request body. The content
//! recipe is the engine's own [`content_instance`]: it builds the RM data
//! value from the row's literals and injects it into the minimal carrier
//! composition stamped with the context template's id, exactly as a content
//! decision table is committed.
//!
//! No openEHR spec governs this — our own design: Arazzo 1.1.0 defines a
//! workflow's inputs but not how they are generated, so the recipe set is
//! this catalogue's own.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::arazzo::rows::Row;
use crate::exec::recipes::content_instance;
use crate::model::case::MatrixCell;

/// The RM data-value classes the content recipe builds an instance of.
///
/// These are the `DV_*` classes the content decision tables constrain at an
/// `ELEMENT.value`; each is built at its RM `data_types` shape. A structural
/// class needs a per-row template, which this recipe does not synthesize, so
/// it is not in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[expect(
    clippy::enum_variant_names,
    reason = "each variant spells its RM class name, and every RM data-value class starts with DV_"
)]
pub enum ContentClass {
    /// `DV_BOOLEAN`.
    DvBoolean,
    /// `DV_CODED_TEXT`.
    DvCodedText,
    /// `DV_COUNT`.
    DvCount,
    /// `DV_DATE`.
    DvDate,
    /// `DV_DATE_TIME`.
    DvDateTime,
    /// `DV_DURATION`.
    DvDuration,
    /// `DV_EHR_URI`.
    DvEhrUri,
    /// `DV_IDENTIFIER`.
    DvIdentifier,
    /// `DV_INTERVAL`, whose limit type the template id names.
    DvInterval,
    /// `DV_MULTIMEDIA`.
    DvMultimedia,
    /// `DV_ORDINAL`.
    DvOrdinal,
    /// `DV_PARSABLE`.
    DvParsable,
    /// `DV_PROPORTION`.
    DvProportion,
    /// `DV_QUANTITY`.
    DvQuantity,
    /// `DV_SCALE`.
    DvScale,
    /// `DV_TEXT`.
    DvText,
    /// `DV_TIME`.
    DvTime,
    /// `DV_URI`.
    DvUri,
}

impl ContentClass {
    /// Every class, in declaration order.
    pub const ALL: &[Self] = &[
        Self::DvBoolean,
        Self::DvCodedText,
        Self::DvCount,
        Self::DvDate,
        Self::DvDateTime,
        Self::DvDuration,
        Self::DvEhrUri,
        Self::DvIdentifier,
        Self::DvInterval,
        Self::DvMultimedia,
        Self::DvOrdinal,
        Self::DvParsable,
        Self::DvProportion,
        Self::DvQuantity,
        Self::DvScale,
        Self::DvText,
        Self::DvTime,
        Self::DvUri,
    ];

    /// The RM class name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::DvBoolean => "DV_BOOLEAN",
            Self::DvCodedText => "DV_CODED_TEXT",
            Self::DvCount => "DV_COUNT",
            Self::DvDate => "DV_DATE",
            Self::DvDateTime => "DV_DATE_TIME",
            Self::DvDuration => "DV_DURATION",
            Self::DvEhrUri => "DV_EHR_URI",
            Self::DvIdentifier => "DV_IDENTIFIER",
            Self::DvInterval => "DV_INTERVAL",
            Self::DvMultimedia => "DV_MULTIMEDIA",
            Self::DvOrdinal => "DV_ORDINAL",
            Self::DvParsable => "DV_PARSABLE",
            Self::DvProportion => "DV_PROPORTION",
            Self::DvQuantity => "DV_QUANTITY",
            Self::DvScale => "DV_SCALE",
            Self::DvText => "DV_TEXT",
            Self::DvTime => "DV_TIME",
            Self::DvUri => "DV_URI",
        }
    }
}

/// The recipe a rows file names, with its arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "name", rename_all = "snake_case", deny_unknown_fields)]
pub enum RowRecipe {
    /// The content-chapter recipe: one composition per row, carrying the
    /// value of `rm_class` built from the row's literals.
    ContentInstance {
        /// The class of the value under test.
        rm_class: ContentClass,
        /// The id of the template the composition is committed against.
        template_id: String,
        /// The workflow input the composition is bound to.
        input: String,
    },
}

/// Why a recipe cannot build a row's value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("column `{column}` is not a decision-table cell: {reason}")]
pub struct RecipeError {
    /// The literal column.
    pub column: String,
    /// The conversion failure.
    pub reason: String,
}

impl RowRecipe {
    /// The workflow input the recipe binds.
    #[must_use]
    pub fn input(&self) -> &str {
        match self {
            Self::ContentInstance { input, .. } => input,
        }
    }

    /// Builds the value the recipe binds for `row`, reading only the
    /// `literals` columns; a column the row leaves out is an absent cell.
    ///
    /// The result is a pure function of the recipe and the row, so the same
    /// row yields the same bytes on every run.
    ///
    /// # Errors
    /// [`RecipeError`] when a literal cell does not read as a decision-table
    /// cell.
    pub fn apply(&self, literals: &[String], row: &Row) -> Result<Value, RecipeError> {
        let cells = literals
            .iter()
            .map(|column| match row.get(column) {
                None => Ok(MatrixCell::Absent),
                Some(value) => {
                    serde_json::from_value::<MatrixCell>(value.clone()).map_err(|error| {
                        RecipeError {
                            column: column.clone(),
                            reason: error.to_string(),
                        }
                    })
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        match self {
            Self::ContentInstance {
                rm_class,
                template_id,
                ..
            } => Ok(content_instance(
                rm_class.name(),
                template_id,
                literals,
                &cells,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn recipe() -> RowRecipe {
        RowRecipe::ContentInstance {
            rm_class: ContentClass::DvCount,
            template_id: "cnf.tpl.count_range".to_owned(),
            input: "composition".to_owned(),
        }
    }

    fn row(value: &Value) -> Row {
        serde_json::from_value(value.clone()).unwrap()
    }

    #[test]
    fn every_class_name_is_its_serde_token() {
        for class in ContentClass::ALL {
            assert_eq!(serde_json::to_value(class).unwrap(), json!(class.name()));
        }
    }

    #[test]
    fn the_same_row_yields_byte_identical_compositions() {
        let literals = vec!["magnitude".to_owned()];
        let one = recipe()
            .apply(&literals, &row(&json!({ "magnitude": 15 })))
            .unwrap();
        let two = recipe()
            .apply(&literals, &row(&json!({ "magnitude": 15 })))
            .unwrap();
        assert_eq!(one.to_string(), two.to_string());
        assert_eq!(
            one.pointer("/content/0/data/events/0/data/items/0/value"),
            Some(&json!({ "_type": "DV_COUNT", "magnitude": 15 }))
        );
        assert_eq!(
            one.pointer("/archetype_details/template_id/value"),
            Some(&json!("cnf.tpl.count_range"))
        );
    }

    #[test]
    fn a_null_or_missing_literal_leaves_the_attribute_out() {
        let literals = vec!["magnitude".to_owned()];
        for authored in [json!({ "magnitude": null }), json!({})] {
            let built = recipe().apply(&literals, &row(&authored)).unwrap();
            assert_eq!(
                built.pointer("/content/0/data/events/0/data/items/0/value"),
                Some(&json!({ "_type": "DV_COUNT" })),
                "{authored}"
            );
        }
    }

    #[test]
    fn an_unknown_recipe_or_class_is_refused() {
        for seeded in [
            json!({ "name": "content", "rm_class": "DV_COUNT", "template_id": "t", "input": "c" }),
            json!({ "name": "content_instance", "rm_class": "DV_COUNTS", "template_id": "t", "input": "c" }),
            json!({ "name": "content_instance", "rm_class": "COMPOSITION", "template_id": "t", "input": "c" }),
            json!({ "name": "content_instance", "rm_class": "DV_COUNT", "template_id": "t", "input": "c", "x": 1 }),
        ] {
            assert!(
                serde_json::from_value::<RowRecipe>(seeded.clone()).is_err(),
                "{seeded}"
            );
        }
    }
}
