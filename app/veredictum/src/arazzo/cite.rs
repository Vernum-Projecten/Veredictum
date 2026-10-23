// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `x-veredictum-cite` extension: its shape, its inheritance from
//! criterion to step to workflow, and the check that its quote occurs in the
//! vendored released text it names.
//!
//! No openEHR spec governs this — our own design: the extension is how every
//! expectation in the catalogue carries the released sentence that assigns
//! it.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The specification-extension key a citation is carried under.
pub const CITE_KEY: &str = "x-veredictum-cite";

/// The roots a citation may name: the vendored released openEHR text and the
/// released OpenAPI bundles.
const ORACLE_ROOTS: [&str; 2] = ["specs/openehr/", "specs/rest-oas/"];

/// The vendored tree no citation may name (owner ruling on #590: the SM has
/// no role in the Arazzo catalogue).
const EXCLUDED_ROOT: &str = "specs/openehr/SM/";

/// One `x-veredictum-cite` value: the released source, the element or section
/// inside it, and the verbatim sentence that assigns the expectation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Citation {
    /// The vendored file, relative to the repository root.
    pub source: String,
    /// The element (an OAS file) or the section heading (a docs file).
    pub section: String,
    /// The sentence, copied verbatim from the source.
    pub quote: String,
}

impl fmt::Display for Citation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}): \"{}\"", self.source, self.section, self.quote)
    }
}

/// Why a citation is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CiteError {
    /// The extension value is not a `{source, section, quote}` object.
    #[error("`{CITE_KEY}` is malformed: {0}")]
    Malformed(String),
    /// The source is outside the vendored released text, or names the SM.
    #[error("`{0}` is not a vendored released openEHR source a citation may name")]
    OutsideOracle(String),
    /// The source file cannot be read or parsed.
    #[error("`{file}` cannot be read: {reason}")]
    Unreadable {
        /// The cited source.
        file: String,
        /// The verbatim read or parse failure.
        reason: String,
    },
    /// The named element or section does not exist in the source.
    #[error("`{file}` has no element or section `{section}`")]
    SectionNotFound {
        /// The cited source.
        file: String,
        /// The section that was not found.
        section: String,
    },
    /// The quote does not occur in the named element or section.
    #[error("the quote does not occur in `{file}` `{section}`: \"{quote}\"")]
    QuoteNotFound {
        /// The cited source.
        file: String,
        /// The cited section.
        section: String,
        /// The quote that was not found.
        quote: String,
    },
}

/// Reads the citation an object's extension map carries, if any.
///
/// # Errors
/// [`CiteError::Malformed`] when the extension is present and is not a
/// `{source, section, quote}` object with non-empty strings.
pub fn citation_of(
    extensions: Option<&BTreeMap<String, Value>>,
) -> Result<Option<Citation>, CiteError> {
    let Some(value) = extensions.and_then(|map| map.get(CITE_KEY)) else {
        return Ok(None);
    };
    let citation: Citation = serde_json::from_value(value.clone())
        .map_err(|error| CiteError::Malformed(error.to_string()))?;
    if citation.source.trim().is_empty()
        || citation.section.trim().is_empty()
        || citation.quote.trim().is_empty()
    {
        return Err(CiteError::Malformed(
            "`source`, `section` and `quote` must be non-empty".to_owned(),
        ));
    }
    Ok(Some(citation))
}

/// Collapses every whitespace run to one space, the normalization a quote and
/// its source are compared under.
#[must_use]
pub fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Checks citations against the vendored files under one repository root,
/// reading each file once.
#[derive(Debug)]
pub struct QuoteChecker<'a> {
    repo_root: &'a Path,
    texts: BTreeMap<String, Result<String, String>>,
}

impl<'a> QuoteChecker<'a> {
    /// A checker over the repository whose root is `repo_root`.
    #[must_use]
    pub fn new(repo_root: &'a Path) -> Self {
        Self {
            repo_root,
            texts: BTreeMap::new(),
        }
    }

    /// Checks that `citation` names an oracle file, that its section exists
    /// there, and that its quote occurs verbatim in that section after
    /// whitespace normalization.
    ///
    /// A YAML source is an OAS citation: `section` is a top-level key, or a
    /// JSON Pointer when it starts with `/`, and the quote must occur in the
    /// string content of that element. Any other source is docs text:
    /// `section` must be the title of one of its headings, and the quote must
    /// occur under that heading, before the next heading of the same or a
    /// higher level.
    ///
    /// # Errors
    /// The [`CiteError`] naming the first property that does not hold.
    pub fn check(&mut self, citation: &Citation) -> Result<(), CiteError> {
        let source = citation.source.as_str();
        let escapes = Path::new(source)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)));
        if escapes
            || !ORACLE_ROOTS.iter().any(|root| source.starts_with(root))
            || source.starts_with(EXCLUDED_ROOT)
        {
            return Err(CiteError::OutsideOracle(source.to_owned()));
        }
        let text = self.read(source)?;
        let quote = normalize_whitespace(&citation.quote);
        let is_yaml = Path::new(source)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|ext| ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml"));
        let haystack = if is_yaml {
            yaml_section_text(&text, source, &citation.section)?
        } else {
            heading_section_text(&text, &citation.section).ok_or_else(|| {
                CiteError::SectionNotFound {
                    file: source.to_owned(),
                    section: citation.section.clone(),
                }
            })?
        };
        if normalize_whitespace(&haystack).contains(&quote) {
            Ok(())
        } else {
            Err(CiteError::QuoteNotFound {
                file: source.to_owned(),
                section: citation.section.clone(),
                quote: citation.quote.clone(),
            })
        }
    }

    fn read(&mut self, source: &str) -> Result<String, CiteError> {
        let root = self.repo_root;
        let cached = self.texts.entry(source.to_owned()).or_insert_with(|| {
            std::fs::read_to_string(root.join(source)).map_err(|e| e.to_string())
        });
        cached.clone().map_err(|reason| CiteError::Unreadable {
            file: source.to_owned(),
            reason,
        })
    }
}

/// The string content of one element of a YAML source.
fn yaml_section_text(text: &str, source: &str, section: &str) -> Result<String, CiteError> {
    let document = crate::load::yaml_str_to_value(text, Path::new(source)).map_err(|error| {
        CiteError::Unreadable {
            file: source.to_owned(),
            reason: error.detail(),
        }
    })?;
    let element = if section.starts_with('/') {
        document.pointer(section)
    } else {
        document.get(section)
    };
    let Some(element) = element else {
        return Err(CiteError::SectionNotFound {
            file: source.to_owned(),
            section: section.to_owned(),
        });
    };
    let mut out = String::new();
    collect_strings(element, &mut out);
    Ok(out)
}

/// Appends every string leaf under `value`, space-separated, in document order.
fn collect_strings(value: &Value, out: &mut String) {
    match value {
        Value::String(text) => {
            out.push(' ');
            out.push_str(text);
        }
        Value::Array(items) => items.iter().for_each(|item| collect_strings(item, out)),
        Value::Object(map) => map.values().for_each(|item| collect_strings(item, out)),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// The level and title of an `AsciiDoc` (`=`) or Markdown (`#`) heading line.
fn heading(line: &str) -> Option<(usize, String)> {
    let title = line.trim_start_matches(['=', '#']);
    let level = line.len().saturating_sub(title.len());
    (level > 0 && title.starts_with(' ')).then(|| (level, normalize_whitespace(title)))
}

/// The text under the heading titled `section`, up to the next heading of the
/// same or a higher level; `None` when no heading has that title.
fn heading_section_text(text: &str, section: &str) -> Option<String> {
    let wanted = normalize_whitespace(section);
    let mut lines = text.lines();
    let level = lines.by_ref().find_map(|line| {
        heading(line).and_then(|(level, title)| (title == wanted).then_some(level))
    })?;
    let body: Vec<&str> = lines
        .take_while(|line| heading(line).is_none_or(|(next, _)| next > level))
        .collect();
    Some(body.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cite(source: &str, section: &str, quote: &str) -> Citation {
        Citation {
            source: source.to_owned(),
            section: section.to_owned(),
            quote: quote.to_owned(),
        }
    }

    fn repo() -> assert_fs::TempDir {
        use assert_fs::prelude::*;
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("specs/openehr/ITS-REST/specifications/responses/409.yaml")
            .write_str(
                "description: |\n  `409 Conflict` Unable to create.\n  Can happen when the supplied\n  `ehr_id` is already used.\n",
            )
            .unwrap();
        dir.child("specs/openehr/RM/docs/ehr.adoc")
            .write_str("== EHR Class\n\nThe unique identifier of this EHR.\n")
            .unwrap();
        dir.child("specs/openehr/SM/docs/x.adoc")
            .write_str("== Any\n\nanything\n")
            .unwrap();
        dir
    }

    #[test]
    fn a_quote_spanning_lines_is_found_after_normalization() {
        let dir = repo();
        let mut checker = QuoteChecker::new(dir.path());
        let ok = cite(
            "specs/openehr/ITS-REST/specifications/responses/409.yaml",
            "description",
            "Can happen when the supplied `ehr_id` is already used.",
        );
        assert_eq!(checker.check(&ok), Ok(()));
    }

    #[test]
    fn a_seeded_quote_that_does_not_occur_is_refused() {
        let dir = repo();
        let mut checker = QuoteChecker::new(dir.path());
        let seeded = cite(
            "specs/openehr/ITS-REST/specifications/responses/409.yaml",
            "description",
            "Can happen when the supplied `ehr_id` is already deleted.",
        );
        assert!(matches!(
            checker.check(&seeded),
            Err(CiteError::QuoteNotFound { .. })
        ));
    }

    #[test]
    fn a_missing_element_is_refused() {
        let dir = repo();
        let mut checker = QuoteChecker::new(dir.path());
        let missing = cite(
            "specs/openehr/ITS-REST/specifications/responses/409.yaml",
            "summary",
            "Can happen",
        );
        assert!(matches!(
            checker.check(&missing),
            Err(CiteError::SectionNotFound { .. })
        ));
    }

    #[test]
    fn a_docs_citation_needs_its_heading() {
        let dir = repo();
        let mut checker = QuoteChecker::new(dir.path());
        let good = cite(
            "specs/openehr/RM/docs/ehr.adoc",
            "EHR Class",
            "The unique identifier of this EHR.",
        );
        assert_eq!(checker.check(&good), Ok(()));
        let wrong_section = cite(
            "specs/openehr/RM/docs/ehr.adoc",
            "EHR_STATUS Class",
            "The unique identifier of this EHR.",
        );
        assert!(matches!(
            checker.check(&wrong_section),
            Err(CiteError::SectionNotFound { .. })
        ));
    }

    #[test]
    fn a_docs_quote_must_sit_under_its_own_heading() {
        use assert_fs::prelude::*;
        let dir = repo();
        dir.child("specs/openehr/ITS-REST/specifications/docs/overview/rr.md")
            .write_str(
                "# HTTP headers\n\nThe semantic header.\n\n# HTTP status codes\n\n| Code | Meaning |\n| ---- | ------- |\n| 422  | The request was well-formed but was unable to be followed due to semantic errors   |\n\n## A subsection\n\nStill in status codes.\n\n# Representation details\n\nSomething else.\n",
            )
            .unwrap();
        let mut checker = QuoteChecker::new(dir.path());
        let source = "specs/openehr/ITS-REST/specifications/docs/overview/rr.md";
        let row = cite(
            source,
            "HTTP status codes",
            "The request was well-formed but was unable to be followed due to semantic errors",
        );
        assert_eq!(checker.check(&row), Ok(()));
        let nested = cite(source, "HTTP status codes", "Still in status codes.");
        assert_eq!(
            checker.check(&nested),
            Ok(()),
            "a subsection belongs to its parent"
        );
        for (section, quote) in [
            ("HTTP status codes", "Something else."),
            ("HTTP headers", "semantic errors"),
        ] {
            assert!(
                matches!(
                    checker.check(&cite(source, section, quote)),
                    Err(CiteError::QuoteNotFound { .. })
                ),
                "{section}: {quote}"
            );
        }
    }

    #[test]
    fn sources_outside_the_oracle_are_refused() {
        let dir = repo();
        let mut checker = QuoteChecker::new(dir.path());
        for source in [
            "specs/openehr/SM/docs/x.adoc",
            "artifacts/registers/ambiguities.yaml",
            "specs/openehr/../../README.md",
        ] {
            assert!(
                matches!(
                    checker.check(&cite(source, "Any", "anything")),
                    Err(CiteError::OutsideOracle(_))
                ),
                "{source}"
            );
        }
    }

    #[test]
    fn a_malformed_extension_is_refused() {
        let mut ext = BTreeMap::new();
        ext.insert(
            CITE_KEY.to_owned(),
            serde_json::json!({ "source": "specs/openehr/x", "quote": "q" }),
        );
        assert!(matches!(
            citation_of(Some(&ext)),
            Err(CiteError::Malformed(_))
        ));
        ext.insert(
            CITE_KEY.to_owned(),
            serde_json::json!({ "source": "s", "section": "d", "quote": "q", "page": 1 }),
        );
        assert!(matches!(
            citation_of(Some(&ext)),
            Err(CiteError::Malformed(_))
        ));
        assert_eq!(citation_of(None), Ok(None));
    }
}
