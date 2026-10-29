// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The rows gate.
//!
//! Every rows file sits beside the Arazzo document whose workflow it feeds,
//! as `<workflowId>.rows.yaml`, validates against the published rows schema,
//! and binds every input that workflow requires. A file that says `server`
//! may carry no rows, and is then the only rows file a workflow that
//! declares no inputs takes.
//!
//! A rows file may declare `literals`, the columns whose values are
//! decision-table literals: each such value parses against the literal
//! grammar of [`crate::literal`], and one that does not is refused with its
//! row, its column and the character offset of the production that fails. A
//! `recipe` then turns the literals of each row into the values of one or
//! two more inputs ([`crate::arazzo::recipe`]), and the row it completes is
//! what the inputs schema validates.
//!
//! No openEHR spec governs this — our own design: Arazzo 1.1.0 defines a
//! workflow's `inputs` as a JSON Schema but not how a set of values for them
//! is supplied, so the rows file is this catalogue's own format.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::LazyLock;

use roas_arazzo::v1_1::Workflow;
use serde::Deserialize;
use serde_json::Value;

use crate::arazzo::gates::published::rows_schema;
use crate::arazzo::recipe::RowRecipe;
use crate::literal::{Literal, LiteralError};
use crate::vocab::Iteration;

/// The suffix every rows file carries.
pub const ROWS_SUFFIX: &str = ".rows.yaml";

/// The value a generated `format: uuid` input stands in with when a row is
/// validated; the evaluator draws a fresh one per row.
const GENERATED_UUID: &str = "00000000-0000-4000-8000-000000000000";

/// One rows file.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowsFile {
    /// The case id of the workflow the rows feed, `<document>/<workflowId>`.
    pub workflow: String,
    /// Whether every row gets fresh preconditions or all share one state.
    pub iteration: Iteration,
    /// The columns whose values are decision-table literals.
    #[serde(default)]
    pub literals: Vec<String>,
    /// The recipe that binds more inputs from each row's literals.
    #[serde(default)]
    pub recipe: Option<RowRecipe>,
    /// The inputs bound from a value the party declares, by input name.
    #[serde(default)]
    pub declared: BTreeMap<String, Declared>,
    /// What the workflow needs the server under test to be, beyond what
    /// every workflow needs.
    #[serde(default)]
    pub server: Option<ServerOwnership>,
    /// The properties of the whole run, judged once after the last row.
    #[serde(default)]
    pub aggregate: Vec<Aggregate>,
    /// The rows, each binding input names to values; empty when the file
    /// only says what the server must be, and the workflow then runs once
    /// with no authored values.
    #[serde(default)]
    pub rows: Vec<BTreeMap<String, Value>>,
}

/// Where a declared input's value comes from: a dotted path into the
/// party's IXIT or into its statement.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct Declared {
    /// Which declaration the value is read from.
    pub document: Declaration,
    /// The member names leading to the value.
    pub path: Vec<String>,
}

/// The two declarations a party supplies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Declaration {
    /// The IXIT, `ixit.json`.
    Ixit,
    /// The statement, `statement.json`.
    Statement,
}

impl TryFrom<String> for Declared {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let (head, rest) = text
            .split_once('.')
            .ok_or_else(|| format!("`{text}` is not `ixit.<path>` or `statement.<path>`"))?;
        let document = match head {
            "ixit" => Declaration::Ixit,
            "statement" => Declaration::Statement,
            _ => return Err(format!("`{text}` names neither `ixit` nor `statement`")),
        };
        let path: Vec<String> = rest.split('.').map(str::to_owned).collect();
        if path.iter().any(String::is_empty) {
            return Err(format!("`{text}` has an empty member name"));
        }
        Ok(Self { document, path })
    }
}

impl Declared {
    /// The declared value in `declaration`, if the party declares it.
    #[must_use]
    pub fn read<'v>(&self, declaration: &'v Value) -> Option<&'v Value> {
        self.path
            .iter()
            .try_fold(declaration, |value, member| value.get(member))
            .filter(|value| !value.is_null())
    }
}

/// What a workflow needs the server under test to be.
///
/// No openEHR spec governs this, our own design: a workflow whose operation
/// changes state the workflow did not create runs only on a server the party
/// declares as exclusively its run's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerOwnership {
    /// The party's IXIT declares `environment.exclusive_server: true`, so no
    /// other client holds data on the server while the run lasts.
    Exclusive,
}

impl ServerOwnership {
    /// Every value, in vocabulary order (schema emission derives from this).
    pub const ALL: &[Self] = &[Self::Exclusive];

    /// The token a rows file spells the value with.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Exclusive => "exclusive",
        }
    }

    /// Whether the party's IXIT declares what this value needs; an absent
    /// IXIT, environment or member declares nothing.
    #[must_use]
    pub fn declared_by(self, ixit: Option<&Value>) -> bool {
        match self {
            Self::Exclusive => ixit
                .and_then(|ixit| ixit.pointer("/environment/exclusive_server"))
                .and_then(Value::as_bool)
                .is_some_and(|exclusive| exclusive),
        }
    }
}

/// A property of the whole run.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Aggregate {
    /// The workflow output whose value no two rows may share.
    pub distinct: String,
    /// The released sentence that assigns the property.
    #[serde(rename = "x-veredictum-cite")]
    pub cite: crate::arazzo::cite::Citation,
}

/// The value a declared input stands in with when a row is validated; the
/// evaluator reads the party's own value.
const DECLARED: &str = "declared";

static VALIDATOR: LazyLock<Result<jsonschema::Validator, String>> =
    LazyLock::new(|| jsonschema::validator_for(&rows_schema()).map_err(|e| e.to_string()));

/// Reads a rows file's parsed YAML through the published rows schema.
///
/// # Errors
/// One message per schema violation, or the typed-parse failure.
pub fn parse(value: &Value) -> Result<RowsFile, Vec<String>> {
    let validator = VALIDATOR
        .as_ref()
        .map_err(|error| vec![format!("the rows schema does not compile: {error}")])?;
    let violations: Vec<String> = validator
        .iter_errors(value)
        .map(|error| format!("#{}: {error}", error.instance_path()))
        .collect();
    if !violations.is_empty() {
        return Err(violations);
    }
    serde_json::from_value(value.clone()).map_err(|error| vec![error.to_string()])
}

/// The input names a workflow declares, the ones it requires, and the ones
/// the evaluator generates when no row supplies them (`format: uuid`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputShape {
    /// Every property `inputs.properties` names.
    pub declared: Vec<String>,
    /// Every name in `inputs.required`.
    pub required: Vec<String>,
    /// Every declared property of `format: uuid`.
    pub generated: Vec<String>,
}

impl InputShape {
    /// Reads the shape of `workflow.inputs`.
    #[must_use]
    pub fn of(workflow: &Workflow) -> Self {
        let Some(inputs) = &workflow.inputs else {
            return Self::default();
        };
        let properties = inputs.get("properties").and_then(Value::as_object);
        let declared = properties
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default();
        let generated = properties
            .map(|map| {
                map.iter()
                    .filter(|(_, schema)| {
                        schema.get("format").and_then(Value::as_str) == Some("uuid")
                    })
                    .map(|(name, _)| name.clone())
                    .collect()
            })
            .unwrap_or_default();
        // NOTE: a non-string `required` entry is not a JSON Schema, which the Arazzo schema gate
        // already refuses through §Workflow Object `inputs`; here it names no input.
        let required = inputs
            .get("required")
            .and_then(Value::as_array)
            .map(|names| {
                names
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            declared,
            required,
            generated,
        }
    }

    /// The required inputs no row-less run can bind: required and not
    /// generated.
    #[must_use]
    pub fn unbound_without_rows(&self) -> Vec<&str> {
        self.required
            .iter()
            .filter(|name| !self.generated.contains(name))
            .map(String::as_str)
            .collect()
    }
}

/// The refusal of a literal-typed value that does not parse: its row, its
/// column, and the character offset in the value of the production that
/// fails.
fn literal_refusal(index: usize, column: &str, value: &Value, error: &LiteralError) -> String {
    // NOTE: no openEHR spec governs this — our own design; a non-string value has no text to
    // point into, so only a string value carries an offset.
    let offset = match value {
        Value::String(text) => text
            .find(error.fragment())
            .and_then(|byte| text.get(..byte))
            .map(|before| before.chars().count()),
        _ => None,
    };
    match offset {
        Some(at) => {
            format!("row {index}, column `{column}`: {error}, at character {at} of the value")
        }
        None => format!("row {index}, column `{column}`: {error}"),
    }
}

/// Parses every literal-typed value of `rows` against the decision-table
/// literal grammar; returns one message per value that does not parse.
#[must_use]
pub fn check_literals(rows: &RowsFile) -> Vec<String> {
    let mut out = Vec::new();
    for (index, row) in rows.rows.iter().enumerate() {
        for column in &rows.literals {
            if let Some(value) = row.get(column)
                && let Err(error) = Literal::from_cell(value)
            {
                out.push(literal_refusal(index, column, value, &error));
            }
        }
    }
    out
}

/// The rows of `rows` as the workflow receives them: each authored row, plus
/// the inputs its recipe binds.
///
/// # Errors
/// One message per literal that does not parse, per row that already
/// supplies an input the recipe binds, and per row the recipe cannot build.
pub fn bind(rows: &RowsFile) -> Result<Vec<BTreeMap<String, Value>>, Vec<String>> {
    let mut problems = check_literals(rows);
    let mut bound = Vec::with_capacity(rows.rows.len());
    for (index, row) in rows.rows.iter().enumerate() {
        let mut row = row.clone();
        if let Some(recipe) = &rows.recipe {
            for input in recipe.inputs() {
                if row.contains_key(input) {
                    problems.push(format!(
                        "row {index}: `{input}` is bound by the recipe and cannot be supplied"
                    ));
                }
            }
            match recipe.bindings(&rows.literals, &row) {
                Ok(values) => row.extend(values),
                Err(error) => problems.push(format!("row {index}: {error}")),
            }
        }
        bound.push(row);
    }
    if problems.is_empty() {
        Ok(bound)
    } else {
        Err(problems)
    }
}

/// Checks the header of `rows` against the inputs `shape` declares: every
/// literal column and every input the recipe binds are declared inputs, and
/// no input the recipe binds is a literal column.
fn check_header(rows: &RowsFile, shape: &InputShape) -> Vec<String> {
    let mut out = Vec::new();
    for input in rows.declared.keys() {
        if !shape.declared.contains(input) {
            out.push(format!(
                "declared input `{input}` is not an input the workflow declares"
            ));
        }
    }
    for column in &rows.literals {
        if !shape.declared.contains(column) {
            out.push(format!(
                "literal column `{column}` is not an input the workflow declares"
            ));
        }
    }
    if let Some(recipe) = &rows.recipe {
        for input in recipe.inputs() {
            if !shape.declared.iter().any(|name| name == input) {
                out.push(format!(
                    "the recipe binds `{input}`, which is not an input the workflow declares"
                ));
            }
            if rows.literals.iter().any(|column| column == input) {
                out.push(format!(
                    "the recipe binds `{input}`, which is also a literal column"
                ));
            }
        }
    }
    out
}

/// Checks every row of `rows` against `workflow`'s inputs; returns one
/// message per problem, each naming its row.
///
/// A file with no rows stands for one run with no authored values, so its
/// generated and declared inputs alone must satisfy the inputs schema. A
/// workflow that declares no inputs refuses every row and every header
/// member that names an input.
#[must_use]
pub fn check_rows(rows: &RowsFile, workflow: &Workflow) -> Vec<String> {
    let mut out = Vec::new();
    if !rows.aggregate.is_empty() && rows.rows.is_empty() {
        out.push("`aggregate` needs rows to judge".to_owned());
    }
    let Some(inputs) = &workflow.inputs else {
        if !rows.rows.is_empty() {
            out.push(format!(
                "workflow `{}` declares no inputs, so a row has nothing to bind",
                workflow.workflow_id
            ));
        }
        out.extend(check_header(rows, &InputShape::default()));
        return out;
    };
    let shape = InputShape::of(workflow);
    out.extend(check_header(rows, &shape));
    if !rows.aggregate.is_empty() && rows.iteration != Iteration::SinglePass {
        out.push("`aggregate` needs `iteration: single_pass`".to_owned());
    }
    for aggregate in &rows.aggregate {
        if !workflow.outputs.contains_key(&aggregate.distinct) {
            out.push(format!(
                "`aggregate` names `{}`, which is not an output of the workflow",
                aggregate.distinct
            ));
        }
    }
    let validator = match jsonschema::validator_for(inputs) {
        Ok(validator) => validator,
        Err(error) => {
            out.push(format!(
                "the workflow's inputs schema does not compile: {error}"
            ));
            return out;
        }
    };
    for (index, row) in rows.rows.iter().enumerate() {
        for key in row.keys() {
            if !shape.declared.contains(key) {
                out.push(format!(
                    "row {index}: `{key}` is not an input the workflow declares"
                ));
            }
            if rows.declared.contains_key(key) {
                out.push(format!(
                    "row {index}: `{key}` is declared by the party and cannot be supplied"
                ));
            }
        }
    }
    let bound = match bind(rows) {
        Ok(bound) if bound.is_empty() => vec![BTreeMap::new()],
        Ok(bound) => bound,
        Err(problems) => {
            out.extend(problems);
            return out;
        }
    };
    for (index, row) in bound.iter().enumerate() {
        let mut instance: serde_json::Map<String, Value> =
            row.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        for name in &shape.generated {
            instance
                .entry(name.clone())
                .or_insert_with(|| Value::String(GENERATED_UUID.to_owned()));
        }
        for name in rows.declared.keys() {
            instance
                .entry(name.clone())
                .or_insert_with(|| Value::String(DECLARED.to_owned()));
        }
        let instance = Value::Object(instance);
        for error in validator.iter_errors(&instance) {
            out.push(format!("row {index}: #{}: {error}", error.instance_path()));
        }
    }
    out
}

/// The `workflowId` a rows file names by its file name,
/// `<workflowId>.rows.yaml`; `None` for any other file.
#[must_use]
pub fn workflow_id_of(path: &Path) -> Option<&str> {
    path.file_name()?.to_str()?.strip_suffix(ROWS_SUFFIX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn workflow(inputs: &Value) -> Workflow {
        serde_json::from_value(json!({
            "workflowId": "w",
            "inputs": inputs,
            "steps": [{ "stepId": "s", "operationId": "o" }]
        }))
        .unwrap()
    }

    fn inputs() -> Value {
        json!({
            "type": "object",
            "required": ["magnitude", "ehr_id"],
            "properties": {
                "magnitude": { "type": "number" },
                "units": { "type": "string" },
                "ehr_id": { "type": "string", "format": "uuid" }
            }
        })
    }

    #[test]
    fn a_well_formed_rows_file_parses_and_binds() {
        let rows = parse(&json!({
            "workflow": "content/w",
            "iteration": "reset_per_row",
            "rows": [{ "magnitude": 5.0, "units": "cm" }, { "magnitude": 11 }]
        }))
        .unwrap();
        assert_eq!(rows.iteration, Iteration::ResetPerRow);
        assert_eq!(
            check_rows(&rows, &workflow(&inputs())),
            Vec::<String>::new()
        );
    }

    #[test]
    fn seeded_schema_defects_are_refused() {
        for seeded in [
            json!({ "workflow": "w", "iteration": "reset_per_row", "rows": [{}] }),
            json!({ "workflow": "c/w", "iteration": "per_row", "rows": [{}] }),
            json!({ "workflow": "c/w", "rows": [{}] }),
            json!({ "workflow": "c/w", "iteration": "single_pass", "rows": [] }),
            json!({ "workflow": "c/w", "iteration": "single_pass", "rows": [{}], "extra": 1 }),
        ] {
            assert!(parse(&seeded).is_err(), "{seeded}");
        }
    }

    #[test]
    fn seeded_binding_defects_are_refused() {
        let rows = |row: Value| RowsFile {
            workflow: "c/w".to_owned(),
            iteration: Iteration::SinglePass,
            literals: Vec::new(),
            recipe: None,
            declared: BTreeMap::new(),
            server: None,
            aggregate: Vec::new(),
            rows: vec![serde_json::from_value(row).unwrap()],
        };
        let w = workflow(&inputs());
        let missing = check_rows(&rows(json!({ "units": "cm" })), &w);
        assert!(
            missing.iter().any(|m| m.contains("magnitude")),
            "{missing:?}"
        );
        let unknown = check_rows(&rows(json!({ "magnitude": 1, "unit": "cm" })), &w);
        assert!(
            unknown.iter().any(|m| m.contains("`unit` is not an input")),
            "{unknown:?}"
        );
        let mistyped = check_rows(&rows(json!({ "magnitude": "five" })), &w);
        assert_eq!(mistyped.len(), 1, "{mistyped:?}");
        let no_inputs: Workflow = serde_json::from_value(
            json!({ "workflowId": "w", "steps": [{ "stepId": "s", "operationId": "o" }] }),
        )
        .unwrap();
        assert_eq!(check_rows(&rows(json!({})), &no_inputs).len(), 1);
    }

    #[test]
    fn an_exclusive_server_is_declared_only_by_a_true_ixit_member() {
        let rows = parse(&json!({
            "workflow": "admin/w",
            "iteration": "single_pass",
            "server": "exclusive",
            "rows": [{}]
        }))
        .unwrap();
        assert_eq!(rows.server, Some(ServerOwnership::Exclusive));
        let exclusive = ServerOwnership::Exclusive;
        let ixit = |environment: Value| json!({ "instances": {}, "environment": environment });
        assert!(exclusive.declared_by(Some(&ixit(json!({ "exclusive_server": true })))));
        assert!(!exclusive.declared_by(Some(&ixit(json!({ "exclusive_server": false })))));
        assert!(!exclusive.declared_by(Some(&ixit(json!({})))));
        assert!(!exclusive.declared_by(Some(&json!({ "instances": {} }))));
        assert!(!exclusive.declared_by(None));
        for seeded in [json!("shared"), json!("Exclusive"), json!(true)] {
            let file = json!({ "workflow": "admin/w", "iteration": "single_pass",
                               "server": seeded, "rows": [{}] });
            assert!(parse(&file).is_err(), "{file}");
        }
    }

    #[test]
    fn an_input_less_workflow_takes_the_exclusive_marker_and_refuses_rows() {
        let no_inputs: Workflow = serde_json::from_value(
            json!({ "workflowId": "w", "steps": [{ "stepId": "s", "operationId": "o" }] }),
        )
        .unwrap();
        let marker = json!({ "workflow": "admin/w", "iteration": "reset_per_row",
                             "server": "exclusive" });
        let file = parse(&marker).unwrap();
        assert_eq!(file.server, Some(ServerOwnership::Exclusive));
        assert!(file.rows.is_empty());
        assert_eq!(check_rows(&file, &no_inputs), Vec::<String>::new());
        let with_rows = RowsFile {
            rows: vec![BTreeMap::new()],
            ..file.clone()
        };
        assert_eq!(
            check_rows(&with_rows, &no_inputs),
            vec!["workflow `w` declares no inputs, so a row has nothing to bind".to_owned()]
        );
        let mut with_declared = file.clone();
        with_declared.declared.insert(
            "system_id".to_owned(),
            Declared::try_from("ixit.system_id".to_owned()).unwrap(),
        );
        let found = check_rows(&with_declared, &no_inputs);
        assert!(
            found
                .iter()
                .any(|m| m.contains("declared input `system_id`")),
            "{found:?}"
        );
        let with_aggregate = RowsFile {
            aggregate: vec![Aggregate {
                distinct: "ehr_id".to_owned(),
                cite: serde_json::from_value(
                    json!({ "source": "s", "section": "d", "quote": "q" }),
                )
                .unwrap(),
            }],
            ..file.clone()
        };
        let found = check_rows(&with_aggregate, &no_inputs);
        assert!(
            found.iter().any(|m| m.contains("`aggregate` needs rows")),
            "{found:?}"
        );
        let required = check_rows(&file, &workflow(&inputs()));
        assert_eq!(required.len(), 1, "{required:?}");
        assert!(
            required.iter().all(|m| m.contains("magnitude")),
            "{required:?}"
        );
        for seeded in [
            json!({ "workflow": "admin/w", "iteration": "reset_per_row" }),
            json!({ "workflow": "admin/w", "iteration": "reset_per_row",
                    "server": "exclusive", "rows": [] }),
        ] {
            assert!(parse(&seeded).is_err(), "{seeded}");
        }
    }

    #[test]
    fn generated_inputs_bind_themselves_and_others_need_rows() {
        let shape = InputShape::of(&workflow(&inputs()));
        assert_eq!(shape.unbound_without_rows(), vec!["magnitude"]);
    }

    #[test]
    fn a_rows_file_name_names_its_workflow() {
        assert_eq!(
            workflow_id_of(Path::new("/a/cases/ehr/w-1.rows.yaml")),
            Some("w-1")
        );
        assert_eq!(workflow_id_of(Path::new("/a/cases/ehr/w.yaml")), None);
        assert_eq!(
            workflow_id_of(Path::new("/a/cases/ehr/ehr.arazzo.yaml")),
            None
        );
    }

    fn content_workflow() -> Workflow {
        workflow(&json!({
            "type": "object",
            "required": ["composition"],
            "properties": {
                "magnitude": { "type": ["integer", "string", "null"] },
                "composition": { "type": "object" }
            }
        }))
    }

    fn content_rows(rows: &Value) -> RowsFile {
        parse(&json!({
            "workflow": "content/w",
            "iteration": "reset_per_row",
            "literals": ["magnitude"],
            "recipe": {
                "name": "content_instance",
                "rm_class": "DV_COUNT",
                "template_id": "cnf.tpl.count_range",
                "input": "composition"
            },
            "rows": rows
        }))
        .unwrap()
    }

    #[test]
    fn literal_rows_parse_and_the_recipe_binds_its_input() {
        let rows = content_rows(&json!([{ "magnitude": 15 }, { "magnitude": null }]));
        assert_eq!(check_rows(&rows, &content_workflow()), Vec::<String>::new());
        let bound = bind(&rows).unwrap();
        assert_eq!(
            bound
                .first()
                .and_then(|row| row.get("composition"))
                .and_then(|c| c.pointer("/content/0/data/events/0/data/items/0/value")),
            Some(&json!({ "_type": "DV_COUNT", "magnitude": 15 }))
        );
        assert_eq!(
            bound.first().and_then(|row| row.get("magnitude")),
            Some(&json!(15))
        );
    }

    #[test]
    fn the_same_row_binds_byte_identical_values() {
        let rows = content_rows(&json!([{ "magnitude": 15 }, { "magnitude": 15 }]));
        let bound = bind(&rows).unwrap();
        let text = |i: usize| serde_json::to_string(&bound.get(i)).unwrap();
        assert_eq!(text(0), text(1));
        assert_eq!(
            text(0),
            serde_json::to_string(&bind(&rows).unwrap().first()).unwrap()
        );
    }

    #[test]
    fn a_seeded_unparsable_literal_names_its_row_column_and_position() {
        let rows = content_rows(&json!([{ "magnitude": 15 }, { "magnitude": "[cm 5.0..x, m]" }]));
        let found = check_rows(&rows, &content_workflow());
        assert_eq!(
            found,
            vec![
                "row 1, column `magnitude`: invalid decision-table literal \"cm 5.0..x\": range \
                 bounds must both be numbers or both ISO 8601 lexemes, at character 1 of the value"
                    .to_owned()
            ]
        );
        assert!(bind(&rows).is_err());
        let structured = content_rows(&json!([{ "magnitude": { "a": 1 } }]));
        let found = check_literals(&structured);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found
                .iter()
                .all(|m| m.starts_with("row 0, column `magnitude`: ")),
            "{found:?}"
        );
    }

    #[test]
    fn seeded_header_defects_are_refused() {
        let mut rows = content_rows(&json!([{ "magnitude": 15 }]));
        rows.literals.push("units".to_owned());
        let found = check_rows(&rows, &content_workflow());
        assert!(
            found.iter().any(|m| m.contains("literal column `units`")),
            "{found:?}"
        );
        let supplied = content_rows(&json!([{ "magnitude": 15, "composition": {} }]));
        let found = check_rows(&supplied, &content_workflow());
        assert!(
            found.iter().any(|m| m.contains("bound by the recipe")),
            "{found:?}"
        );
        let undeclared = workflow(&json!({
            "type": "object",
            "properties": { "magnitude": {} }
        }));
        let found = check_rows(&content_rows(&json!([{ "magnitude": 15 }])), &undeclared);
        assert!(
            found
                .iter()
                .any(|m| m.contains("not an input the workflow declares")),
            "{found:?}"
        );
        for seeded in [
            json!({ "workflow": "c/w", "iteration": "single_pass", "recipe": {
                "name": "content_instance", "rm_class": "DV_COUNT", "template_id": "t", "input": "c" },
                "rows": [{}] }),
            json!({ "workflow": "c/w", "iteration": "single_pass", "literals": [], "rows": [{}] }),
            json!({ "workflow": "c/w", "iteration": "single_pass", "literals": ["a", "a"], "rows": [{}] }),
            json!({ "workflow": "c/w", "iteration": "single_pass", "literals": ["a"], "recipe": {
                "name": "content_instance", "rm_class": "COMPOSITION", "template_id": "t", "input": "c" },
                "rows": [{}] }),
        ] {
            assert!(parse(&seeded).is_err(), "{seeded}");
        }
    }
}
