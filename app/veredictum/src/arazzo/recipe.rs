// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Row recipes: the committed, deterministic code that turns a row's
//! decision-table literals into an input value the workflow sends.
//!
//! A rows file names at most one recipe in its header. The recipe reads the
//! row's literal-typed columns and binds its result to a workflow input,
//! which a step then sends, typically as the whole request body. The content
//! recipe is the engine's own [`content_instance`]: it builds the RM data
//! value from the row's literals and injects it into the minimal carrier
//! composition stamped with the context template's id, exactly as a content
//! decision table is committed. Where the rows vary the template itself, the
//! synthesized recipe also builds each row's operational template
//! ([`synthesize_opt`]) and binds it to a second input, which a step uploads
//! before the commit.
//!
//! No openEHR spec governs this — our own design: Arazzo 1.1.0 defines a
//! workflow's inputs but not how they are generated, so the recipe set is
//! this catalogue's own.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::arazzo::rows::Row;
use crate::exec::content_synth::synthesize_opt;
use crate::exec::recipes::content_instance;
use crate::model::case::MatrixCell;

/// The RM data-value classes the content recipe builds an instance of.
///
/// These are the `DV_*` classes the content decision tables constrain at an
/// `ELEMENT.value`; each is built at its RM `data_types` shape. A structural
/// class needs a per-row template, which [`RowRecipe::ContentSynthesized`]
/// builds, so it is not in this list.
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

/// The RM classes whose rows each carry their own operational template.
///
/// A content decision table whose columns vary the template's constraint
/// row by row cannot share one uploaded template: the value classes listed
/// here vary an `ELEMENT.value` domain constraint, and the structural classes
/// vary the cardinality, existence or type of a carrier attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SynthesizedClass {
    /// `CLUSTER`, its `items` cardinality.
    Cluster,
    /// `COMPOSITION`, its `content` cardinality and `context` existence.
    Composition,
    /// `DV_CODED_TEXT`, its `C_CODE_PHRASE`.
    DvCodedText,
    /// `DV_DATE`, its `C_DATE` validity and range.
    DvDate,
    /// `DV_DATE_TIME`, its `C_DATE_TIME` validity and range.
    DvDateTime,
    /// `DV_DURATION`, its `C_DURATION` pattern and range.
    DvDuration,
    /// `DV_IDENTIFIER`, the `C_STRING` on one attribute.
    DvIdentifier,
    /// `DV_INTERVAL`, the constraints on its limits.
    DvInterval,
    /// `DV_MULTIMEDIA`, its media type and size.
    DvMultimedia,
    /// `DV_PARSABLE`, the `C_STRING`s on its value and formalism.
    DvParsable,
    /// `DV_TEXT`, its `C_STRING`.
    DvText,
    /// `DV_TIME`, its `C_TIME` validity and range.
    DvTime,
    /// `DV_URI`, its `C_STRING`.
    DvUri,
    /// `ELEMENT`, its `value` and `null_flavour` existence.
    Element,
    /// `EVENT`, its `state` existence or its narrowed type.
    Event,
    /// `HISTORY`, its `events` cardinality and `summary` existence.
    History,
    /// `ITEM`, the narrowed type of a container member.
    Item,
    /// `ITEM_LIST`, its `items` cardinality.
    ItemList,
    /// `ITEM_STRUCTURE`, the narrowed type of a data slot.
    ItemStructure,
    /// `ITEM_TABLE`, its `rows` cardinality.
    ItemTable,
    /// `ITEM_TREE`, its `items` cardinality.
    ItemTree,
    /// `OBSERVATION`, its `state` and `protocol` existence.
    Observation,
}

impl SynthesizedClass {
    /// Every class, in declaration order.
    pub const ALL: &[Self] = &[
        Self::Cluster,
        Self::Composition,
        Self::DvCodedText,
        Self::DvDate,
        Self::DvDateTime,
        Self::DvDuration,
        Self::DvIdentifier,
        Self::DvInterval,
        Self::DvMultimedia,
        Self::DvParsable,
        Self::DvText,
        Self::DvTime,
        Self::DvUri,
        Self::Element,
        Self::Event,
        Self::History,
        Self::Item,
        Self::ItemList,
        Self::ItemStructure,
        Self::ItemTable,
        Self::ItemTree,
        Self::Observation,
    ];

    /// The RM class name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Cluster => "CLUSTER",
            Self::Composition => "COMPOSITION",
            Self::DvCodedText => "DV_CODED_TEXT",
            Self::DvDate => "DV_DATE",
            Self::DvDateTime => "DV_DATE_TIME",
            Self::DvDuration => "DV_DURATION",
            Self::DvIdentifier => "DV_IDENTIFIER",
            Self::DvInterval => "DV_INTERVAL",
            Self::DvMultimedia => "DV_MULTIMEDIA",
            Self::DvParsable => "DV_PARSABLE",
            Self::DvText => "DV_TEXT",
            Self::DvTime => "DV_TIME",
            Self::DvUri => "DV_URI",
            Self::Element => "ELEMENT",
            Self::Event => "EVENT",
            Self::History => "HISTORY",
            Self::Item => "ITEM",
            Self::ItemList => "ITEM_LIST",
            Self::ItemStructure => "ITEM_STRUCTURE",
            Self::ItemTable => "ITEM_TABLE",
            Self::ItemTree => "ITEM_TREE",
            Self::Observation => "OBSERVATION",
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
    /// The content-chapter recipe for a table whose rows vary the template:
    /// one operational template and one composition per row, both built
    /// from the row's literals.
    ///
    /// The row's template id is `template_id` followed by a digest of the
    /// template the row's cells build, so rows with the same constraint share
    /// a template and a changed template is uploaded under a new id.
    ContentSynthesized {
        /// The class whose constraint the rows vary.
        rm_class: SynthesizedClass,
        /// The template family; each row's template id starts with it.
        template_id: String,
        /// The workflow input the composition is bound to.
        input: String,
        /// The workflow input the operational template, as OPT 1.4 XML
        /// text, is bound to.
        template_input: String,
    },
}

/// Why a recipe cannot build a row's value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecipeError {
    /// A literal cell does not read as a decision-table cell.
    #[error("column `{column}` is not a decision-table cell: {reason}")]
    Cell {
        /// The literal column.
        column: String,
        /// The conversion failure.
        reason: String,
    },
    /// The row's operational template cannot be built from its cells.
    #[error("the template `{template_id}` cannot be built from the row: {reason}")]
    Template {
        /// The row's template id.
        template_id: String,
        /// The synthesis failure.
        reason: String,
    },
}

impl RowRecipe {
    /// The workflow input the recipe binds.
    #[must_use]
    pub fn input(&self) -> &str {
        match self {
            Self::ContentInstance { input, .. } | Self::ContentSynthesized { input, .. } => input,
        }
    }

    /// The workflow input the recipe binds the row's operational template
    /// to, when it builds one.
    #[must_use]
    pub fn template_input(&self) -> Option<&str> {
        match self {
            Self::ContentInstance { .. } => None,
            Self::ContentSynthesized { template_input, .. } => Some(template_input),
        }
    }

    /// Every workflow input the recipe binds.
    #[must_use]
    pub fn inputs(&self) -> Vec<&str> {
        let mut inputs = vec![self.input()];
        inputs.extend(self.template_input());
        inputs
    }

    /// Builds the value the recipe binds to [`RowRecipe::input`] for `row`,
    /// reading only the `literals` columns; a column the row leaves out is an
    /// absent cell.
    ///
    /// The result is a pure function of the recipe and the row, so the same
    /// row yields the same bytes on every run.
    ///
    /// # Errors
    /// [`RecipeError`] when a literal cell does not read as a decision-table
    /// cell.
    pub fn apply(&self, literals: &[String], row: &Row) -> Result<Value, RecipeError> {
        let cells = cells(literals, row)?;
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
            Self::ContentSynthesized {
                rm_class,
                template_id,
                ..
            } => Ok(content_instance(
                rm_class.name(),
                &synthesize_row(template_id, *rm_class, literals, &cells)?.0,
                literals,
                &cells,
            )),
        }
    }

    /// Builds every value the recipe binds for `row`, each with the input it
    /// is bound to: the composition, and the operational template when the
    /// recipe builds one.
    ///
    /// # Errors
    /// [`RecipeError`] when a literal cell does not read as a decision-table
    /// cell, or the row's template cannot be built from its cells.
    pub fn bindings(
        &self,
        literals: &[String],
        row: &Row,
    ) -> Result<Vec<(String, Value)>, RecipeError> {
        match self {
            Self::ContentInstance { input, .. } => {
                Ok(vec![(input.clone(), self.apply(literals, row)?)])
            }
            Self::ContentSynthesized {
                rm_class,
                template_id,
                input,
                template_input,
            } => {
                let cells = cells(literals, row)?;
                let (id, xml) = synthesize_row(template_id, *rm_class, literals, &cells)?;
                let composition = content_instance(rm_class.name(), &id, literals, &cells);
                Ok(vec![
                    (input.clone(), composition),
                    (template_input.clone(), Value::String(xml)),
                ])
            }
        }
    }
}

/// Builds the operational template of one synthesized row, and returns its
/// template id with its OPT 1.4 XML text.
///
/// The id is the family id, then the first eight hex digits of a version-5
/// UUID over the template the row's cells build under the family id. Rows
/// whose constraint cells agree share a template, and a template whose
/// content changes is uploaded under a new id, so a server that already
/// holds an earlier one never validates against it.
fn synthesize_row(
    family: &str,
    rm_class: SynthesizedClass,
    literals: &[String],
    cells: &[MatrixCell],
) -> Result<(String, String), RecipeError> {
    // NOTE: no openEHR spec governs this — our own design; the family id names the
    // interval limit type the synthesizer reads, which the digest suffix cannot spell.
    let build = |id: &str| {
        synthesize_opt(family, rm_class.name(), id, literals, cells).map_err(|error| {
            RecipeError::Template {
                template_id: id.to_owned(),
                reason: error.to_string(),
            }
        })
    };
    let id = row_template_id(family, &build(family)?);
    let xml = build(&id)?;
    Ok((id, xml))
}

/// The row's literal cells, in `literals` order; a column the row leaves out
/// is an absent cell.
fn cells(literals: &[String], row: &Row) -> Result<Vec<MatrixCell>, RecipeError> {
    literals
        .iter()
        .map(|column| match row.get(column) {
            None => Ok(MatrixCell::Absent),
            Some(value) => serde_json::from_value::<MatrixCell>(value.clone()).map_err(|error| {
                RecipeError::Cell {
                    column: column.clone(),
                    reason: error.to_string(),
                }
            }),
        })
        .collect()
}

/// The template id of one synthesized row: the family id, then the first
/// eight hex digits of a version-5 UUID over `draft`, the template the row
/// builds under the family id.
fn row_template_id(family: &str, draft: &str) -> String {
    const NAMESPACE: uuid::Uuid = uuid::Uuid::from_bytes([
        0x3b, 0x1e, 0x6a, 0x52, 0x0c, 0x4f, 0x5d, 0x2e, 0x9a, 0x77, 0x41, 0x0d, 0xc3, 0x58, 0x2f,
        0x96,
    ]);
    let digest = uuid::Uuid::new_v5(&NAMESPACE, draft.as_bytes())
        .simple()
        .to_string();
    let short: String = digest.chars().take(8).collect();
    format!("{family}.{short}")
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

    fn synthesized() -> RowRecipe {
        RowRecipe::ContentSynthesized {
            rm_class: SynthesizedClass::Composition,
            template_id: "cnf.tpl.content_cardinality".to_owned(),
            input: "composition".to_owned(),
            template_input: "template".to_owned(),
        }
    }

    #[test]
    fn every_synthesized_class_name_is_its_serde_token() {
        for class in SynthesizedClass::ALL {
            assert_eq!(serde_json::to_value(class).unwrap(), json!(class.name()));
        }
    }

    #[test]
    fn a_synthesized_row_binds_its_template_and_a_composition_stamped_with_it() {
        let literals = vec!["cardinality".to_owned(), "content_count".to_owned()];
        let authored = row(&json!({ "cardinality": "1plus", "content_count": 3 }));
        let bound = synthesized().bindings(&literals, &authored).unwrap();
        let names: Vec<&str> = bound.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["composition", "template"]);
        let composition = &bound[0].1;
        let template = bound[1].1.as_str().unwrap();
        let stamped = composition
            .pointer("/archetype_details/template_id/value")
            .and_then(Value::as_str)
            .unwrap();
        assert!(
            stamped.starts_with("cnf.tpl.content_cardinality."),
            "{stamped}"
        );
        assert_eq!(stamped.len(), "cnf.tpl.content_cardinality.".len() + 8);
        assert!(
            template.contains(&format!(
                "<template_id><value>{stamped}</value></template_id>"
            )),
            "the template is uploaded under the id the composition names"
        );
        assert_eq!(
            composition
                .pointer("/content")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(3)
        );
    }

    #[test]
    fn the_row_template_id_follows_the_template_the_cells_build() {
        let literals = vec!["cardinality".to_owned(), "content_count".to_owned()];
        let id = |value: &Value| {
            synthesized()
                .apply(&literals, &row(value))
                .unwrap()
                .pointer("/archetype_details/template_id/value")
                .cloned()
                .unwrap()
        };
        let one = id(&json!({ "cardinality": "1plus", "content_count": 3 }));
        assert_eq!(
            one,
            id(&json!({ "cardinality": "1plus", "content_count": 3 }))
        );
        assert_eq!(
            one,
            id(&json!({ "cardinality": "1plus", "content_count": 1 })),
            "an instance column leaves the template, and so its id, alone"
        );
        assert_ne!(
            one,
            id(&json!({ "cardinality": "opt", "content_count": 3 }))
        );
    }

    #[test]
    fn a_row_the_synthesizer_cannot_build_is_a_template_error() {
        let literals = vec!["cardinality".to_owned(), "content_count".to_owned()];
        let refused = synthesized()
            .bindings(
                &literals,
                &row(&json!({ "cardinality": "sometimes", "content_count": 1 })),
            )
            .unwrap_err();
        assert!(matches!(refused, RecipeError::Template { .. }), "{refused}");
    }

    #[test]
    fn an_unknown_synthesized_class_or_a_missing_template_input_is_refused() {
        for seeded in [
            json!({ "name": "content_synthesized", "rm_class": "DV_COUNT", "template_id": "t", "input": "c", "template_input": "o" }),
            json!({ "name": "content_synthesized", "rm_class": "COMPOSITION", "template_id": "t", "input": "c" }),
        ] {
            assert!(
                serde_json::from_value::<RowRecipe>(seeded.clone()).is_err(),
                "{seeded}"
            );
        }
    }
}
