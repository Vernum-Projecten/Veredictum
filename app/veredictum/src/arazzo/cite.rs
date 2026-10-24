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
use std::num::NonZeroU64;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The specification-extension key a citation is carried under.
pub const CITE_KEY: &str = "x-veredictum-cite";

/// The roots a citation may name: the vendored released openEHR text and the
/// released OpenAPI bundles.
pub const ORACLE_ROOTS: [&str; 2] = ["specs/openehr/", "specs/rest-oas/"];

/// The vendored tree no citation may name (owner ruling on #590: the SM has
/// no role in the Arazzo catalogue).
pub const EXCLUDED_ROOT: &str = "specs/openehr/SM/";

/// One `x-veredictum-cite` value.
///
/// It names the released source, the element or section inside it, the
/// verbatim sentence that assigns the expectation, and the `upstream-report`
/// issue when that sentence is defective.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Citation {
    /// The vendored file, relative to the repository root.
    pub source: String,
    /// The element (an OAS file) or the section heading (a docs file).
    pub section: String,
    /// The sentence, copied verbatim from the source.
    pub quote: String,
    /// The `upstream-report` issue in this repository that records a defect,
    /// contradiction or silence in the cited text. Serde refuses zero, a
    /// negative number and a string, so a present value is always an issue
    /// number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<NonZeroU64>,
    /// The requirement level of the quoted sentence: `must` unless the
    /// sentence only recommends.
    #[serde(default, skip_serializing_if = "Level::is_must")]
    pub level: Level,
}

/// The requirement level of a quoted sentence (RFC 2119 §1–§3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// MUST, SHALL, REQUIRED, their negations, or a definition in the
    /// indicative.
    #[default]
    Must,
    /// SHOULD, RECOMMENDED, or SHOULD NOT.
    Should,
}

impl Level {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[Self] = &[Self::Must, Self::Should];

    /// Whether this is the default level, which a citation leaves unwritten.
    #[must_use]
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's `skip_serializing_if` passes the field by reference"
    )]
    pub fn is_must(&self) -> bool {
        *self == Self::Must
    }

    /// The token a citation spells.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Must => "must",
            Self::Should => "should",
        }
    }
}

/// The RFC 2119 keywords a quote carries, read as upper-case words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Keywords {
    requires: bool,
    recommends: bool,
    permits: bool,
}

impl Keywords {
    fn of(quote: &str) -> Self {
        let mut out = Self::default();
        for word in quote.split(|c: char| !c.is_ascii_alphabetic()) {
            match word {
                "MUST" | "SHALL" | "REQUIRED" => out.requires = true,
                "SHOULD" | "RECOMMENDED" => out.recommends = true,
                "MAY" | "OPTIONAL" => out.permits = true,
                _ => {}
            }
        }
        out
    }
}

/// Checks that a citation's `level` agrees with the keywords of its quote.
///
/// A quote that recommends and does not require needs `level: should`; a
/// `level: should` quote must recommend; a quote whose only keyword permits
/// is never asserted. RFC 2119 keywords are the upper-case words; a sentence
/// with none is a definition and stays `must`.
///
/// # Errors
/// [`CiteError::LevelMismatch`] or [`CiteError::PermissionOnly`].
pub fn check_level(citation: &Citation) -> Result<(), CiteError> {
    let keywords = Keywords::of(&citation.quote);
    if keywords.permits && !keywords.requires && !keywords.recommends {
        return Err(CiteError::PermissionOnly(citation.quote.clone()));
    }
    let needed = match citation.level {
        Level::Should if !keywords.recommends => Some(Level::Must),
        Level::Must if keywords.recommends && !keywords.requires => Some(Level::Should),
        _ => None,
    };
    match needed {
        Some(needed) => Err(CiteError::LevelMismatch {
            level: citation.level.token(),
            needed: needed.token(),
            quote: citation.quote.clone(),
        }),
        None => Ok(()),
    }
}

impl fmt::Display for Citation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}): \"{}\"", self.source, self.section, self.quote)?;
        if let Some(report) = self.report {
            write!(f, " (upstream report #{report})")?;
        }
        Ok(())
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
    /// The section names more than one heading of a docs source.
    #[error(
        "`{file}` has {headings} headings `{section}`; name one with a `Parent > Child` heading path"
    )]
    AmbiguousSection {
        /// The cited source.
        file: String,
        /// The cited section.
        section: String,
        /// How many headings it matches.
        headings: usize,
    },
    /// The citation's `level` disagrees with the keywords of its quote.
    #[error(
        "`level: {level}` disagrees with the quote, which needs `level: {needed}`: \"{quote}\""
    )]
    LevelMismatch {
        /// The level the citation states.
        level: &'static str,
        /// The level the quote's keywords call for.
        needed: &'static str,
        /// The quote.
        quote: String,
    },
    /// The quote only permits, and a permission is never asserted.
    #[error(
        "the quote only permits (MAY or OPTIONAL), and a permission is never asserted: \"{0}\""
    )]
    PermissionOnly(String),
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
    /// `section` must name exactly one of its headings, by title or by a
    /// `Parent > Child` path of titles, and the quote must occur under that
    /// heading, before the next heading of the same or a higher level.
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
            heading_section_text(&text, &citation.section).map_err(|miss| match miss {
                SectionMiss::NotFound => CiteError::SectionNotFound {
                    file: source.to_owned(),
                    section: citation.section.clone(),
                },
                SectionMiss::Ambiguous(headings) => CiteError::AmbiguousSection {
                    file: source.to_owned(),
                    section: citation.section.clone(),
                    headings,
                },
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

/// The separator of a heading path in a docs citation's `section`:
/// `Parent > Child` names the heading `Child` directly under `Parent`.
pub const SECTION_PATH_SEPARATOR: &str = " > ";

/// One heading of a docs file.
struct Heading {
    /// The zero-based line the heading is on.
    line: usize,
    /// `=` or `#` count.
    level: usize,
    /// The whitespace-normalized title.
    title: String,
    /// The index, in the heading list, of the nearest preceding heading of a
    /// higher level.
    parent: Option<usize>,
}

/// Why a docs `section` names no single heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SectionMiss {
    /// No heading, or no heading path, matches.
    NotFound,
    /// This many headings match.
    Ambiguous(usize),
}

/// The text under the one heading `section` names, up to the next heading of
/// the same or a higher level.
///
/// `section` is a heading title, or a path of titles joined by
/// [`SECTION_PATH_SEPARATOR`] in which each title is a direct child of the one
/// before it. A section that matches more than one heading is refused rather
/// than resolved to the first, because the quote would then be checked
/// against a section the author may not have meant.
fn heading_section_text(text: &str, section: &str) -> Result<String, SectionMiss> {
    let lines: Vec<&str> = text.lines().collect();
    let mut headings: Vec<Heading> = Vec::new();
    for (line, content) in lines.iter().enumerate() {
        if let Some((level, title)) = heading(content) {
            let parent = headings.iter().rposition(|h| h.level < level);
            headings.push(Heading {
                line,
                level,
                title,
                parent,
            });
        }
    }
    let mut candidates: Option<Vec<usize>> = None;
    for part in section.split(SECTION_PATH_SEPARATOR) {
        let wanted = normalize_whitespace(part);
        let matched = headings
            .iter()
            .enumerate()
            .filter(|(_, h)| h.title == wanted)
            .filter(|(_, h)| match &candidates {
                None => true,
                Some(parents) => h.parent.is_some_and(|p| parents.contains(&p)),
            })
            .map(|(index, _)| index)
            .collect();
        candidates = Some(matched);
    }
    let found = match candidates.as_deref().unwrap_or_default() {
        [] => return Err(SectionMiss::NotFound),
        [one] => headings.get(*one).ok_or(SectionMiss::NotFound)?,
        many => return Err(SectionMiss::Ambiguous(many.len())),
    };
    let body: Vec<&str> = lines
        .iter()
        .skip(found.line.saturating_add(1))
        .take_while(|line| heading(line).is_none_or(|(next, _)| next > found.level))
        .copied()
        .collect();
    Ok(body.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cite(source: &str, section: &str, quote: &str) -> Citation {
        Citation {
            source: source.to_owned(),
            section: section.to_owned(),
            quote: quote.to_owned(),
            report: None,
            level: Level::Must,
        }
    }

    #[test]
    fn a_level_must_agree_with_the_keywords_of_its_quote() {
        let should = |quote: &str| Citation {
            level: Level::Should,
            ..cite("specs/openehr/x.md", "s", quote)
        };
        let must = |quote: &str| cite("specs/openehr/x.md", "s", quote);
        assert_eq!(check_level(&must("It MUST be weak.")), Ok(()));
        assert_eq!(
            check_level(&must("A definition, in the indicative.")),
            Ok(())
        );
        assert_eq!(check_level(&should("It SHOULD be sent.")), Ok(()));
        assert_eq!(
            check_level(&must("It MUST be sent, and it SHOULD be weak.")),
            Ok(())
        );
        assert!(matches!(
            check_level(&must("It SHOULD be sent.")),
            Err(CiteError::LevelMismatch {
                needed: "should",
                ..
            })
        ));
        assert!(matches!(
            check_level(&should("It MUST be sent.")),
            Err(CiteError::LevelMismatch { needed: "must", .. })
        ));
        assert!(matches!(
            check_level(&must("The response MAY include it.")),
            Err(CiteError::PermissionOnly(_))
        ));
        assert!(matches!(
            check_level(&should("The header is OPTIONAL.")),
            Err(CiteError::PermissionOnly(_))
        ));
        assert_eq!(
            check_level(&must("a lower-case should is no keyword")),
            Ok(())
        );
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

    /// The vendored RM composition package carries `Overview` three times:
    /// under `Composition Package`, `Context Model of Recording` and
    /// `Event Context`.
    const COMPOSITION: &str = "specs/openehr/RM/docs/ehr/master05-composition_package.adoc";

    fn vendored() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn a_seeded_repeated_heading_title_is_refused_as_ambiguous() {
        let root = vendored();
        let mut checker = QuoteChecker::new(&root);
        let seeded = cite(
            COMPOSITION,
            "Overview",
            "The openEHR EHR model takes account of a systematic analysis of \"context\".",
        );
        assert_eq!(
            checker.check(&seeded),
            Err(CiteError::AmbiguousSection {
                file: COMPOSITION.to_owned(),
                section: "Overview".to_owned(),
                headings: 3,
            })
        );
    }

    #[test]
    fn a_heading_path_names_one_of_the_repeated_headings() {
        let root = vendored();
        let mut checker = QuoteChecker::new(&root);
        let context =
            "The openEHR EHR model takes account of a systematic analysis of \"context\".";
        let top = "The Composition is the primary 'data container' in the openEHR EHR and is the root point of clinical content.";
        let event = "The optional event_context in the `COMPOSITION` class is used to document the healthcare event causing new or changed content in the record.";
        for (section, quote) in [
            ("Context Model of Recording > Overview", context),
            ("Composition Package > Overview", top),
            (
                "Context Model of Recording > Event Context > Overview",
                event,
            ),
        ] {
            assert_eq!(
                checker.check(&cite(COMPOSITION, section, quote)),
                Ok(()),
                "{section}"
            );
        }
        assert!(
            matches!(
                checker.check(&cite(
                    COMPOSITION,
                    "Composition Package > Overview",
                    context
                )),
                Err(CiteError::QuoteNotFound { .. })
            ),
            "the path selects the section, so a quote from a sibling section is refused"
        );
        for missing in [
            "Composition Package > Event Context",
            "Class Descriptions > Overview",
            "Composition Package > Nowhere",
        ] {
            assert!(
                matches!(
                    checker.check(&cite(COMPOSITION, missing, context)),
                    Err(CiteError::SectionNotFound { .. })
                ),
                "{missing}: a path step must be a direct child"
            );
        }
    }

    #[test]
    fn a_repeated_heading_in_markdown_is_refused_too() {
        use assert_fs::prelude::*;
        let dir = repo();
        let source = "specs/openehr/ITS-REST/specifications/docs/overview/x.md";
        dir.child(source)
            .write_str("# A\n\n## Notes\n\nfirst\n\n# B\n\n## Notes\n\nsecond\n")
            .unwrap();
        let mut checker = QuoteChecker::new(dir.path());
        assert!(matches!(
            checker.check(&cite(source, "Notes", "second")),
            Err(CiteError::AmbiguousSection { headings: 2, .. })
        ));
        assert_eq!(checker.check(&cite(source, "B > Notes", "second")), Ok(()));
        assert!(matches!(
            checker.check(&cite(source, "A > Notes", "second")),
            Err(CiteError::QuoteNotFound { .. })
        ));
    }

    #[test]
    fn a_report_is_a_positive_issue_number() {
        let with = |report: Value| {
            let mut ext = BTreeMap::new();
            ext.insert(
                CITE_KEY.to_owned(),
                serde_json::json!({ "source": "s", "section": "d", "quote": "q", "report": report }),
            );
            citation_of(Some(&ext))
        };
        let citation = with(serde_json::json!(618)).unwrap().unwrap();
        assert_eq!(citation.report.map(NonZeroU64::get), Some(618));
        assert!(citation.to_string().ends_with("(upstream report #618)"));
        for seeded in [
            serde_json::json!(0),
            serde_json::json!(-3),
            serde_json::json!("618"),
            serde_json::json!(6.5),
        ] {
            assert!(
                matches!(with(seeded.clone()), Err(CiteError::Malformed(_))),
                "{seeded}"
            );
        }
    }
}
