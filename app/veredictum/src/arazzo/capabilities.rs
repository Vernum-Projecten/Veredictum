// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! `catalogue/capabilities.yaml`: the options a statement answers, each with
//! the arms the released text allows and the documents that test each arm.
//!
//! A document listed under an arm runs only when the statement selects that
//! arm; otherwise every one of its rows is skipped with
//! `option-not-selected` and the option's citation. No openEHR spec governs
//! the file's shape — our own design; each option cites the released
//! sentence that leaves the behaviour to the implementation.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::LazyLock;

use serde::Deserialize;

use crate::arazzo::cite::Citation;
use crate::arazzo::gates::published::capabilities_schema;
use crate::arazzo::gates::surface::Element;
use crate::arazzo::results::{Skip, SkipReason, Why};

/// The file name under a catalogue root.
pub const CAPABILITIES_FILE: &str = "capabilities.yaml";

/// The capabilities file.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    /// The options, by name.
    #[serde(default)]
    pub options: BTreeMap<String, OptionDecl>,
}

/// One option: behaviour the released text leaves to the implementation.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionDecl {
    /// What the option decides, in plain words.
    pub description: String,
    /// The arms, by the token a statement's `options` list selects one with.
    pub arms: BTreeMap<String, Arm>,
    /// The released sentence that leaves the choice to the implementation.
    #[serde(rename = "x-veredictum-cite")]
    pub cite: Citation,
    /// The response branches the arms are the mutually exclusive answers to.
    #[serde(default)]
    pub decides: Vec<Decides>,
}

/// The response statuses of one operation that the arms of an option divide
/// between them, each arm asserting the answer it names.
///
/// No openEHR spec governs this, our own design: the surface gate credits a
/// listed status when one arm asserts it and every arm calls the operation.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decides {
    /// The API document, as `cases/surface.yaml` names it.
    pub api: String,
    /// The `operationId`.
    pub operation: String,
    /// The response statuses, as the OAS spells them.
    pub statuses: Vec<String>,
    /// The released sentence that makes these statuses alternative answers
    /// of the operation.
    #[serde(rename = "x-veredictum-cite")]
    pub cite: Citation,
}

impl OptionDecl {
    /// The response elements the option decides.
    #[must_use]
    pub fn decided(&self) -> std::collections::BTreeSet<Element> {
        self.decides
            .iter()
            .flat_map(|decides| {
                decides.statuses.iter().map(|status| Element::Response {
                    api: decides.api.clone(),
                    operation: decides.operation.clone(),
                    status: status.clone(),
                })
            })
            .collect()
    }
}

/// One arm of an option.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Arm {
    /// The documents that test this arm, by document id.
    #[serde(default)]
    pub documents: Vec<String>,
}

/// Why the capabilities file cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum CapabilitiesError {
    /// The file does not read or parse as YAML.
    #[error(transparent)]
    Load(#[from] crate::load::LoadError),
    /// The file does not match the published capabilities schema, or the
    /// typed shape behind it.
    #[error("`{path}` is not a capabilities file: {}", reasons.join("; "))]
    Shape {
        /// The file.
        path: std::path::PathBuf,
        /// Every violation, verbatim.
        reasons: Vec<String>,
    },
}

static VALIDATOR: LazyLock<Result<jsonschema::Validator, String>> =
    LazyLock::new(|| jsonschema::validator_for(&capabilities_schema()).map_err(|e| e.to_string()));

impl Capabilities {
    /// Loads the capabilities file at `path` through the published schema.
    ///
    /// # Errors
    /// [`CapabilitiesError`] when the file does not load, does not validate
    /// against the schema, or has another shape.
    pub fn load(path: &Path) -> Result<Self, CapabilitiesError> {
        let value = crate::load::yaml_file_to_value(path)?;
        let shape = |reasons| CapabilitiesError::Shape {
            path: path.to_owned(),
            reasons,
        };
        let validator = VALIDATOR
            .as_ref()
            .map_err(|error| shape(vec![format!("the schema does not compile: {error}")]))?;
        let violations: Vec<String> = validator
            .iter_errors(&value)
            .map(|error| format!("#{}: {error}", error.instance_path()))
            .collect();
        if !violations.is_empty() {
            return Err(shape(violations));
        }
        serde_json::from_value(value).map_err(|error| shape(vec![error.to_string()]))
    }

    /// The skip every row of `document` carries when the statement, whose
    /// `options` are `selected`, does not select the arm the document tests;
    /// `None` when the document runs.
    #[must_use]
    pub fn skip_for(&self, document: &str, selected: &[String], repo_root: &Path) -> Option<Skip> {
        for option in self.options.values() {
            let arm = option
                .arms
                .iter()
                .find(|(_, arm)| arm.documents.iter().any(|d| d == document));
            if let Some((name, _)) = arm
                && !selected.contains(name)
            {
                return Some(Skip {
                    reason: SkipReason::OptionNotSelected,
                    why: Why::of(&option.cite, repo_root),
                });
            }
        }
        None
    }

    /// The arms each option's documents fall in, as `(option, arm, document)`.
    pub fn arm_documents(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        self.options.iter().flat_map(|(option, decl)| {
            decl.arms.iter().flat_map(move |(arm, documents)| {
                documents
                    .documents
                    .iter()
                    .map(move |document| (option.as_str(), arm.as_str(), document.as_str()))
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capabilities() -> Capabilities {
        serde_json::from_value(serde_json::json!({
            "options": {
                "ehr-xml-read": {
                    "description": "d",
                    "arms": {
                        "ehr-xml-supported": { "documents": ["ehr_xml"] },
                        "ehr-xml-unsupported": { "documents": ["ehr_xml_not_acceptable"] }
                    },
                    "x-veredictum-cite": { "source": "specs/openehr/x.md", "section": "s", "quote": "q" }
                }
            }
        }))
        .unwrap()
    }

    #[test]
    fn a_document_of_an_unselected_arm_is_skipped_and_others_run() {
        let caps = capabilities();
        let selected = vec!["ehr-xml-supported".to_owned()];
        let root = Path::new(".");
        assert_eq!(caps.skip_for("ehr_xml", &selected, root), None);
        assert_eq!(caps.skip_for("ehr", &selected, root), None);
        let skip = caps
            .skip_for("ehr_xml_not_acceptable", &selected, root)
            .unwrap();
        assert_eq!(skip.reason, SkipReason::OptionNotSelected);
        assert!(caps.skip_for("ehr_xml", &[], root).is_some());
    }

    #[test]
    fn an_unknown_key_and_a_one_armed_option_are_refused_by_the_schema() {
        use assert_fs::prelude::*;
        let dir = assert_fs::TempDir::new().unwrap();
        let file = dir.child("capabilities.yaml");
        for bad in [
            "option: {}\n",
            "options:\n  o:\n    description: d\n    arms: { a: { documents: [x] } }\n    x-veredictum-cite: { source: s, section: t, quote: q }\n",
            "options:\n  o:\n    description: d\n    arms: { a: {}, b: { documents: [x, x] } }\n    x-veredictum-cite: { source: s, section: t, quote: q }\n",
        ] {
            file.write_str(bad).unwrap();
            assert!(
                matches!(
                    Capabilities::load(file.path()),
                    Err(CapabilitiesError::Shape { .. })
                ),
                "{bad}"
            );
        }
        file.write_str("options:\n  o:\n    description: d\n    arms: { a: {}, b: { documents: [x] } }\n    x-veredictum-cite: { source: s, section: t, quote: q }\n")
            .unwrap();
        assert!(Capabilities::load(file.path()).is_ok());
    }
}
