// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `x-veredictum-derive` extension: a step derives the parts of an
//! `OBJECT_VERSION_ID` from one of its own outputs.
//!
//! BASE `object_version_id.adoc` defines the identifier as "Globally unique
//! identifier for one version of a versioned object; lexical form:
//! `object_id  '::' creating_system_id  '::' version_tree_id`", with the
//! functions `object_id()`, `creating_system_id()` and `version_tree_id()`.
//! Those three functions are the whole vocabulary: a workflow that created a
//! resource and learned its `version_uid` can address the version container
//! by the `object_id` part, which no standard Arazzo runtime expression or
//! selector can take from a string.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The specification-extension key of a step's derived outputs.
pub const DERIVE_KEY: &str = "x-veredictum-derive";

/// The separator of the `OBJECT_VERSION_ID` lexical form.
const SEPARATOR: &str = "::";

/// One function of BASE `OBJECT_VERSION_ID`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[expect(
    clippy::enum_variant_names,
    reason = "the variants spell the three BASE OBJECT_VERSION_ID functions, each of which ends in id"
)]
pub enum DeriveFunction {
    /// `object_id()`: the identifier of the version container.
    ObjectId,
    /// `creating_system_id()`: the system that created the version.
    CreatingSystemId,
    /// `version_tree_id()`: the version's place in its version tree.
    VersionTreeId,
}

impl DeriveFunction {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[Self] = &[Self::ObjectId, Self::CreatingSystemId, Self::VersionTreeId];

    /// The token a derivation spells.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::ObjectId => "object_id",
            Self::CreatingSystemId => "creating_system_id",
            Self::VersionTreeId => "version_tree_id",
        }
    }

    /// Reads a token; an unknown one is refused, never defaulted.
    ///
    /// # Errors
    /// [`DeriveError::UnknownFunction`] for a token outside the three.
    pub fn parse(token: &str) -> Result<Self, DeriveError> {
        Self::ALL
            .iter()
            .copied()
            .find(|function| function.token() == token)
            .ok_or_else(|| DeriveError::UnknownFunction(token.to_owned()))
    }

    /// Applies the function to `value`.
    ///
    /// # Errors
    /// [`NotObjectVersionId`] when `value` is not of the lexical form
    /// `object_id::creating_system_id::version_tree_id` with three non-empty
    /// parts.
    pub fn apply(self, value: &str) -> Result<String, NotObjectVersionId> {
        let mut parts = value.split(SEPARATOR);
        let (Some(object_id), Some(system_id), Some(tree_id), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(NotObjectVersionId::of(value));
        };
        if object_id.is_empty() || system_id.is_empty() || tree_id.is_empty() {
            return Err(NotObjectVersionId::of(value));
        }
        Ok(match self {
            Self::ObjectId => object_id,
            Self::CreatingSystemId => system_id,
            Self::VersionTreeId => tree_id,
        }
        .to_owned())
    }
}

/// A value that is not of the `OBJECT_VERSION_ID` lexical form.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "`{value}` is not an OBJECT_VERSION_ID: it has {parts} `::`-separated part(s), {empty} of them empty, where the lexical form has three non-empty parts"
)]
pub struct NotObjectVersionId {
    /// The value as captured.
    pub value: String,
    /// How many `::`-separated parts it has.
    pub parts: usize,
    /// How many of those parts are empty.
    pub empty: usize,
}

impl NotObjectVersionId {
    fn of(value: &str) -> Self {
        let parts: Vec<&str> = value.split(SEPARATOR).collect();
        Self {
            value: value.to_owned(),
            parts: parts.len(),
            empty: parts.iter().filter(|part| part.is_empty()).count(),
        }
    }
}

/// One derived output: the function and the step output it reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Derivation {
    /// The `OBJECT_VERSION_ID` function applied.
    pub function: DeriveFunction,
    /// The name of the step's own output the function reads.
    pub of: String,
}

/// Why an `x-veredictum-derive` value is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeriveError {
    /// The value is not a map of names to `{ function, of }`.
    #[error("`{DERIVE_KEY}` is malformed: {0}")]
    Malformed(String),
    /// A function outside `object_id`, `creating_system_id` and `version_tree_id`.
    #[error("`{DERIVE_KEY}` names the unknown function `{0}`")]
    UnknownFunction(String),
    /// An `of` that names no output the step declares.
    #[error("`{DERIVE_KEY}` output `{name}` reads `{of}`, which the step does not declare")]
    UndeclaredOutput {
        /// The derived output.
        name: String,
        /// The output it reads.
        of: String,
    },
    /// A derived name that is already one of the step's declared outputs.
    #[error("`{DERIVE_KEY}` output `{0}` collides with an output the step declares")]
    Collides(String),
}

/// The derived outputs of a step, by name, read from its extension map and
/// checked against the outputs the step declares, which `is_declared` answers.
///
/// # Errors
/// The [`DeriveError`] that refuses the extension.
pub fn derivations_of(
    extensions: Option<&BTreeMap<String, Value>>,
    is_declared: impl Fn(&str) -> bool,
) -> Result<BTreeMap<String, Derivation>, DeriveError> {
    let Some(value) = extensions.and_then(|e| e.get(DERIVE_KEY)) else {
        return Ok(BTreeMap::new());
    };
    let Value::Object(map) = value else {
        return Err(DeriveError::Malformed(
            "not a map of output names".to_owned(),
        ));
    };
    let mut out = BTreeMap::new();
    for (name, entry) in map {
        let Value::Object(fields) = entry else {
            return Err(DeriveError::Malformed(format!("`{name}` is not an object")));
        };
        if let Some(extra) = fields.keys().find(|k| *k != "function" && *k != "of") {
            return Err(DeriveError::Malformed(format!(
                "`{name}` carries the unknown field `{extra}`"
            )));
        }
        let (Some(Value::String(function)), Some(Value::String(of))) =
            (fields.get("function"), fields.get("of"))
        else {
            return Err(DeriveError::Malformed(format!(
                "`{name}` needs a string `function` and a string `of`"
            )));
        };
        let function = DeriveFunction::parse(function)?;
        if is_declared(name) {
            return Err(DeriveError::Collides(name.clone()));
        }
        if !is_declared(of) {
            return Err(DeriveError::UndeclaredOutput {
                name: name.clone(),
                of: of.clone(),
            });
        }
        out.insert(
            name.clone(),
            Derivation {
                function,
                of: of.clone(),
            },
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const UID: &str = "8849182c-82ad-4088-a07f-48ead4180515::openEHRSys.example.com::2.1.4";

    fn extensions(value: Value) -> BTreeMap<String, Value> {
        BTreeMap::from([(DERIVE_KEY.to_owned(), value)])
    }

    #[test]
    fn each_function_takes_its_part_of_a_well_formed_value() {
        assert_eq!(
            DeriveFunction::ObjectId.apply(UID).unwrap(),
            "8849182c-82ad-4088-a07f-48ead4180515"
        );
        assert_eq!(
            DeriveFunction::CreatingSystemId.apply(UID).unwrap(),
            "openEHRSys.example.com"
        );
        assert_eq!(DeriveFunction::VersionTreeId.apply(UID).unwrap(), "2.1.4");
    }

    #[test]
    fn a_value_not_of_the_lexical_form_is_refused_with_its_shape() {
        for (value, parts, empty) in [
            ("8849182c-82ad-4088-a07f-48ead4180515", 1, 0),
            ("a::b", 2, 0),
            ("a::b::c::d", 4, 0),
            ("a::::1", 3, 1),
            ("", 1, 1),
        ] {
            let error = DeriveFunction::ObjectId.apply(value).unwrap_err();
            assert_eq!((error.parts, error.empty), (parts, empty), "{value}");
        }
    }

    #[test]
    fn the_tokens_round_trip_and_an_unknown_one_is_refused() {
        for function in DeriveFunction::ALL {
            assert_eq!(DeriveFunction::parse(function.token()), Ok(*function));
            assert_eq!(
                serde_json::to_value(function).unwrap(),
                json!(function.token())
            );
        }
        assert_eq!(
            DeriveFunction::parse("is_branch"),
            Err(DeriveError::UnknownFunction("is_branch".to_owned()))
        );
    }

    #[test]
    fn a_derivation_reads_a_declared_output_under_a_new_name() {
        let declared = |name: &str| name == "version_uid";
        let ext = extensions(json!({ "vo_uid": { "function": "object_id", "of": "version_uid" } }));
        let derived = derivations_of(Some(&ext), declared).unwrap();
        assert_eq!(
            derived.get("vo_uid"),
            Some(&Derivation {
                function: DeriveFunction::ObjectId,
                of: "version_uid".to_owned()
            })
        );
        assert!(derivations_of(None, declared).unwrap().is_empty());
    }

    #[test]
    fn the_three_refusals() {
        let declared = |name: &str| name == "version_uid";
        let unknown = extensions(json!({ "x": { "function": "is_branch", "of": "version_uid" } }));
        assert_eq!(
            derivations_of(Some(&unknown), declared),
            Err(DeriveError::UnknownFunction("is_branch".to_owned()))
        );
        let undeclared = extensions(json!({ "x": { "function": "object_id", "of": "uid" } }));
        assert_eq!(
            derivations_of(Some(&undeclared), declared),
            Err(DeriveError::UndeclaredOutput {
                name: "x".to_owned(),
                of: "uid".to_owned()
            })
        );
        let collides =
            extensions(json!({ "version_uid": { "function": "object_id", "of": "version_uid" } }));
        assert_eq!(
            derivations_of(Some(&collides), declared),
            Err(DeriveError::Collides("version_uid".to_owned()))
        );
    }

    #[test]
    fn a_malformed_extension_is_refused() {
        let declared = |name: &str| name == "version_uid";
        for value in [
            json!(["object_id"]),
            json!({ "x": "object_id" }),
            json!({ "x": { "function": "object_id" } }),
            json!({ "x": { "function": "object_id", "of": "version_uid", "as": "y" } }),
        ] {
            let ext = extensions(value.clone());
            assert!(
                matches!(
                    derivations_of(Some(&ext), declared),
                    Err(DeriveError::Malformed(_))
                ),
                "{value}"
            );
        }
    }
}
