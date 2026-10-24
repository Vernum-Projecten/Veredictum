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

use serde::Deserialize;

use crate::arazzo::cite::Citation;
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
    /// The file does not match the capabilities shape.
    #[error("`{path}` is not a capabilities file: {reason}")]
    Shape {
        /// The file.
        path: std::path::PathBuf,
        /// The deserializer's verbatim reason.
        reason: String,
    },
}

impl Capabilities {
    /// Loads the capabilities file at `path`.
    ///
    /// # Errors
    /// [`CapabilitiesError`] when the file does not load or has another shape.
    pub fn load(path: &Path) -> Result<Self, CapabilitiesError> {
        let value = crate::load::yaml_file_to_value(path)?;
        serde_json::from_value(value).map_err(|error| CapabilitiesError::Shape {
            path: path.to_owned(),
            reason: error.to_string(),
        })
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
    fn an_unknown_key_is_refused() {
        let bad = serde_json::json!({ "option": {} });
        assert!(serde_json::from_value::<Capabilities>(bad).is_err());
    }
}
