// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! `catalogue/capabilities.yaml`: the options, the document needs and the
//! capability folder marks.
//!
//! Each option is answered by a statement, with the arms the released text
//! allows and the documents that test each arm. A document listed under an arm runs only when the statement selects that
//! arm; otherwise every one of its rows is skipped with
//! `option-not-selected` and the option's citation. A document's `applies`
//! ranges and `formats` decide the `version-out-of-range` and
//! `format-not-claimed` skips, and the `floors` and `extension-only` entries
//! are what `validate` holds each capability folder to. No openEHR spec
//! governs the file's shape — our own design; each option and each range
//! cites the released sentence it rests on.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;
use std::str::FromStr;
use std::sync::LazyLock;

use serde::Deserialize;

use crate::arazzo::cite::Citation;
use crate::arazzo::gates::published::capabilities_schema;
use crate::arazzo::gates::surface::Element;
use crate::arazzo::results::{Skip, SkipReason, Why};
use crate::vocab::{FormatName, SpecComponent};

/// The file name under a catalogue root.
pub const CAPABILITIES_FILE: &str = "capabilities.yaml";

/// The capabilities file.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    /// The options, by name.
    #[serde(default)]
    pub options: BTreeMap<String, OptionDecl>,
    /// What a document needs before it runs, by document id.
    #[serde(default)]
    pub documents: BTreeMap<String, DocumentNeeds>,
    /// The workflow count each capability folder keeps, by folder name.
    #[serde(default)]
    pub floors: BTreeMap<String, usize>,
    /// The capability folders verified only over the routes of sources
    /// marked with [`NON_OPENEHR_KEY`](crate::arazzo::source::NON_OPENEHR_KEY).
    #[serde(default, rename = "extension-only")]
    pub extension_only: BTreeSet<String>,
}

/// What one document needs from the statement before it runs.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentNeeds {
    /// The specification versions the document applies to, by component.
    #[serde(default)]
    pub applies: BTreeMap<SpecComponent, VersionRange>,
    /// The wire formats the server must serve or process for the document.
    #[serde(default)]
    pub formats: Vec<FormatName>,
}

/// A range of release versions of one specification component, both ends
/// inclusive, with the released sentence that bounds it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionRange {
    /// The first release the document applies to.
    #[serde(default)]
    pub min: Option<SpecVersion>,
    /// The last release the document applies to.
    #[serde(default)]
    pub max: Option<SpecVersion>,
    /// The released sentence that bounds the range.
    #[serde(rename = "x-veredictum-cite")]
    pub cite: Citation,
}

impl VersionRange {
    /// Whether `version` lies inside the range.
    #[must_use]
    pub fn contains(&self, version: SpecVersion) -> bool {
        self.min.is_none_or(|min| min <= version) && self.max.is_none_or(|max| version <= max)
    }
}

/// A release version of a specification component, `MAJOR.MINOR.PATCH`, the
/// form the released components name their releases in (ITS-REST
/// `docs/overview/Amendment_record.md` names `Release-1.1.0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(try_from = "String")]
pub struct SpecVersion {
    major: u64,
    minor: u64,
    patch: u64,
}

/// A text that is not a `MAJOR.MINOR.PATCH` release version.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{0}` is not a release version of the form MAJOR.MINOR.PATCH")]
pub struct VersionError(pub String);

impl FromStr for SpecVersion {
    type Err = VersionError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let refuse = || VersionError(text.to_owned());
        let parts: Vec<u64> = text
            .split('.')
            .map(|part| {
                if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(refuse());
                }
                part.parse::<u64>().map_err(|_overflow| refuse())
            })
            .collect::<Result<_, _>>()?;
        let &[major, minor, patch] = parts.as_slice() else {
            return Err(refuse());
        };
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

impl TryFrom<String> for SpecVersion {
    type Error = VersionError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl fmt::Display for SpecVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
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
    pub fn decided(&self) -> BTreeSet<Element> {
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

    /// The skip every row of `document` carries when the statement declares,
    /// in `declared`, a version of a component outside the document's range
    /// for it; `None` when the document runs, which it does for every
    /// component the statement does not declare.
    #[must_use]
    pub fn version_skip(
        &self,
        document: &str,
        declared: &BTreeMap<SpecComponent, SpecVersion>,
        repo_root: &Path,
    ) -> Option<Skip> {
        let needs = self.documents.get(document)?;
        needs.applies.iter().find_map(|(component, range)| {
            let version = declared.get(component)?;
            (!range.contains(*version)).then(|| Skip {
                reason: SkipReason::VersionOutOfRange,
                why: Why::of(&range.cite, repo_root),
            })
        })
    }

    /// The first format `document` needs that `claimed` does not list.
    #[must_use]
    pub fn unclaimed_format(
        &self,
        document: &str,
        claimed: &BTreeSet<FormatName>,
    ) -> Option<FormatName> {
        let needs = self.documents.get(document)?;
        needs
            .formats
            .iter()
            .find(|format| !claimed.contains(format))
            .copied()
    }

    /// Every component an `applies` range names.
    #[must_use]
    pub fn ranged_components(&self) -> BTreeSet<SpecComponent> {
        self.documents
            .values()
            .flat_map(|needs| needs.applies.keys().copied())
            .collect()
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

    fn version(text: &str) -> SpecVersion {
        text.parse().unwrap()
    }

    fn ranged() -> Capabilities {
        serde_json::from_value(serde_json::json!({
            "documents": {
                "from_1_1": {
                    "applies": { "its_rest": {
                        "min": "1.1.0",
                        "x-veredictum-cite": { "source": "specs/openehr/x.md", "section": "s", "quote": "q" }
                    } }
                },
                "xml": { "formats": ["canonical-xml"] }
            }
        }))
        .unwrap()
    }

    #[test]
    fn a_release_version_is_three_numbers_and_orders_numerically() {
        assert!(version("1.10.0") > version("1.9.3"));
        assert_eq!(version("1.1.0").to_string(), "1.1.0");
        for refused in ["1.1", "1.1.0.0", "v1.1.0", "1..0", "1.1.x", "", "1.1.0-rc1"] {
            assert_eq!(
                refused.parse::<SpecVersion>(),
                Err(VersionError(refused.to_owned())),
                "{refused}"
            );
        }
        let range = VersionRange {
            min: Some(version("1.0.2")),
            max: Some(version("1.1.0")),
            cite: ranged().documents["from_1_1"].applies[&SpecComponent::ItsRest]
                .cite
                .clone(),
        };
        assert!(range.contains(version("1.0.2")));
        assert!(range.contains(version("1.1.0")));
        assert!(!range.contains(version("1.0.1")));
        assert!(!range.contains(version("1.2.0")));
    }

    #[test]
    fn a_declared_version_outside_a_range_skips_the_document_with_its_citation() {
        let caps = ranged();
        let root = Path::new(".");
        let declared = |v: &str| BTreeMap::from([(SpecComponent::ItsRest, version(v))]);
        assert_eq!(
            caps.version_skip("from_1_1", &declared("1.1.0"), root),
            None
        );
        assert_eq!(caps.version_skip("from_1_1", &BTreeMap::new(), root), None);
        let rm_only = BTreeMap::from([(SpecComponent::Rm, version("1.0.0"))]);
        assert_eq!(caps.version_skip("from_1_1", &rm_only, root), None);
        assert_eq!(caps.version_skip("xml", &declared("1.0.0"), root), None);
        let skip = caps
            .version_skip("from_1_1", &declared("1.0.2"), root)
            .unwrap();
        assert_eq!(skip.reason, SkipReason::VersionOutOfRange);
        assert_eq!(skip.why.source, "specs/openehr/x.md");
        assert_eq!(skip.why.quote, "q");
        assert_eq!(
            caps.ranged_components(),
            BTreeSet::from([SpecComponent::ItsRest])
        );
    }

    #[test]
    fn a_document_needing_an_unclaimed_format_names_it() {
        let caps = ranged();
        let json = BTreeSet::from([FormatName::CanonicalJson]);
        let both = BTreeSet::from([FormatName::CanonicalJson, FormatName::CanonicalXml]);
        assert_eq!(
            caps.unclaimed_format("xml", &json),
            Some(FormatName::CanonicalXml)
        );
        assert_eq!(caps.unclaimed_format("xml", &both), None);
        assert_eq!(caps.unclaimed_format("from_1_1", &BTreeSet::new()), None);
    }

    #[test]
    fn the_schema_refuses_an_unknown_component_a_bad_version_and_an_empty_range() {
        use assert_fs::prelude::*;
        let dir = assert_fs::TempDir::new().unwrap();
        let file = dir.child("capabilities.yaml");
        let cite = "x-veredictum-cite: { source: s, section: t, quote: q }";
        for bad in [
            format!("documents:\n  d:\n    applies:\n      itsrest: {{ min: 1.1.0, {cite} }}\n"),
            format!("documents:\n  d:\n    applies:\n      its_rest: {{ min: '1.1', {cite} }}\n"),
            format!("documents:\n  d:\n    applies:\n      its_rest: {{ {cite} }}\n"),
            "documents:\n  d:\n    applies:\n      its_rest: { min: 1.1.0 }\n".to_owned(),
            "documents:\n  d:\n    formats: [canonical-yaml]\n".to_owned(),
            "documents:\n  d: {}\n".to_owned(),
            "floors:\n  ehr: 0\n".to_owned(),
            "extension-only: [ehr, ehr]\n".to_owned(),
        ] {
            file.write_str(&bad).unwrap();
            assert!(
                matches!(
                    Capabilities::load(file.path()),
                    Err(CapabilitiesError::Shape { .. })
                ),
                "{bad}"
            );
        }
        file.write_str(&format!(
            "documents:\n  d:\n    applies:\n      its_rest: {{ min: 1.0.2, max: 1.1.0, {cite} }}\n    formats: [canonical-xml]\nfloors:\n  ehr: 3\nextension-only: [ehr]\n"
        ))
        .unwrap();
        let loaded = Capabilities::load(file.path()).unwrap();
        assert_eq!(loaded.floors["ehr"], 3);
        assert!(loaded.extension_only.contains("ehr"));
    }

    #[test]
    fn an_applies_range_keys_exactly_the_components_a_statement_declares() {
        let statement = crate::schema::statement_schema();
        let declared: BTreeSet<String> = statement
            .pointer("/properties/spec_versions/properties")
            .and_then(serde_json::Value::as_object)
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let ranged: BTreeSet<String> = capabilities_schema()
            .pointer(
                "/properties/documents/additionalProperties/properties/applies/propertyNames/enum",
            )
            .and_then(serde_json::Value::as_array)
            .unwrap()
            .iter()
            .filter_map(|token| token.as_str().map(str::to_owned))
            .collect();
        assert_eq!(declared, ranged);
    }
}
