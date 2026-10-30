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

use crate::arazzo::maturity::{Maturities, Maturity, MaturityError, SpecStatus};

/// The specification-extension key a citation is carried under.
pub const CITE_KEY: &str = "x-veredictum-cite";

/// The roots a citation may name: the vendored released openEHR text, the
/// released OpenAPI bundles, and the ITS-JSON and ITS-XML schema bundles.
pub const ORACLE_ROOTS: [&str; 4] = [
    "specs/openehr/",
    "specs/rest-oas/",
    "specs/its-json-schemas/",
    "specs/its-xml-schemas/",
];

/// The tree no citation may name: the SM was never released and is not part
/// of the oracle (owner ruling on #590), so a citation to it is refused even
/// if the tree is vendored again.
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
    /// The requirement level the citation states for the quoted sentence;
    /// `None` leaves it at the default, `must`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<Level>,
}

impl Citation {
    /// The requirement level of the quoted sentence: the stated one, or
    /// `must` when the citation states none.
    #[must_use]
    pub fn level(&self) -> Level {
        self.level.unwrap_or(Level::Must)
    }
}

/// The requirement level of a quoted sentence (RFC 2119 §1–§5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// MUST, SHALL, REQUIRED, their negations, or a definition in the
    /// indicative.
    Must,
    /// SHOULD, RECOMMENDED, or SHOULD NOT.
    Should,
    /// MAY or OPTIONAL, or a sentence that only says what can be done.
    May,
}

impl Level {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[Self] = &[Self::Must, Self::Should, Self::May];

    /// Whether a criterion citing this level only advises: a failure against
    /// a sentence that recommends never counts against a capability (RFC 2119
    /// §3). A `may` sentence is asserted like a `must` one, because RFC 2119
    /// §5 obliges an implementation to interoperate with one that takes the
    /// option, and a choice the release leaves to the server is an option a
    /// statement selects.
    #[must_use]
    pub fn is_advisory(self) -> bool {
        self == Self::Should
    }

    /// The token a citation spells.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Must => "must",
            Self::Should => "should",
            Self::May => "may",
        }
    }
}

/// Which requirement levels one set of words in a quote carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Words {
    requires: bool,
    recommends: bool,
    permits: bool,
}

impl Words {
    fn any(self) -> bool {
        self.requires || self.recommends || self.permits
    }

    /// The level the words call for: `must` for a sentence with none, which
    /// is a definition in the indicative.
    fn level(self) -> Level {
        if self.requires || !(self.recommends || self.permits) {
            Level::Must
        } else if self.recommends {
            Level::Should
        } else {
            Level::May
        }
    }

    /// Whether the words carry `level`.
    fn carry(self, level: Level) -> bool {
        match level {
            Level::Must => self.requires || !(self.recommends || self.permits),
            Level::Should => self.recommends,
            Level::May => self.permits,
        }
    }
}

/// An apostrophe a word may carry (`can't`), straight or typographic.
const APOSTROPHES: [char; 2] = ['\'', '\u{2019}'];

/// The level words of a quote: the RFC 2119 keywords as upper-case words,
/// and the same words in any other case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Keywords {
    upper: Words,
    lower: Words,
}

impl Keywords {
    fn of(quote: &str) -> Self {
        let mut out = Self::default();
        for word in quote.split(|c: char| !c.is_ascii_alphabetic() && !APOSTROPHES.contains(&c)) {
            let word = word.trim_matches(APOSTROPHES);
            match word {
                "MUST" | "SHALL" | "REQUIRED" => out.upper.requires = true,
                "SHOULD" | "RECOMMENDED" => out.upper.recommends = true,
                "MAY" | "OPTIONAL" => out.upper.permits = true,
                _ => match word.to_ascii_lowercase().as_str() {
                    "must" | "shall" | "required" => out.lower.requires = true,
                    "should" => out.lower.recommends = true,
                    "can" | "may" => out.lower.permits = true,
                    _ => {}
                },
            }
        }
        out
    }
}

/// Checks that a citation's `level` agrees with the level words of its
/// quote.
///
/// The RFC 2119 keywords are the upper-case words, and a quote that carries
/// any is judged by them alone: one that recommends and does not require
/// needs `level: should`, one whose only keyword permits needs `level: may`,
/// and a `level: should` or `level: may` quote carries its keyword.
///
/// A quote with no upper-case keyword is read by the same words in any other
/// case ("must", "shall", "required", "should", "can", "may"), because the
/// openEHR text often writes a requirement in lower case and RFC 2119 cannot
/// decide it. A citation that leaves its level at the default `must` while
/// its quote says "should", "can" or "may" and no requiring word is refused,
/// so its author states the level: `should` or `may` when the sentence says
/// so, or `must` when it requires in other words. A stated `should` or `may`
/// still needs its word. A sentence with none of these words is a definition
/// and stays `must`.
///
/// # Errors
/// [`CiteError::LevelMismatch`], [`CiteError::PermissionOnly`] or
/// [`CiteError::UnmarkedLevel`].
pub fn check_level(citation: &Citation) -> Result<(), CiteError> {
    let keywords = Keywords::of(&citation.quote);
    let level = citation.level();
    let quote = || citation.quote.clone();
    let mismatch = |needed: Level| CiteError::LevelMismatch {
        level: level.token(),
        needed: needed.token(),
        quote: quote(),
    };
    if keywords.upper.any() {
        let upper = keywords.upper;
        return match upper.level() {
            _ if upper.carry(level) => Ok(()),
            Level::May => Err(CiteError::PermissionOnly(quote())),
            needed => Err(mismatch(needed)),
        };
    }
    let lower = keywords.lower;
    match citation.level {
        _ if lower.carry(level) => Ok(()),
        None => Err(CiteError::UnmarkedLevel {
            needed: lower.level().token(),
            quote: quote(),
        }),
        Some(Level::Must) => Ok(()),
        Some(Level::Should | Level::May) => Err(mismatch(lower.level())),
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
    /// The quote only permits (MAY or OPTIONAL), and the citation does not
    /// say so with `level: may`.
    #[error(
        "the quote only permits (MAY or OPTIONAL), so its citation needs `level: may`: \"{0}\""
    )]
    PermissionOnly(String),
    /// A `level: must` quote has no upper-case keyword, says "should", "can"
    /// or "may", and carries no requiring word.
    #[error(
        "the quote says \"should\", \"can\" or \"may\" and nothing requires, so `level: must` overstates it: state `level: {needed}`, or quote the sentence that requires: \"{quote}\""
    )]
    UnmarkedLevel {
        /// The level the quote's words call for.
        needed: &'static str,
        /// The quote.
        quote: String,
    },
    /// The source belongs to a document the release has retired.
    #[error("`{file}` belongs to `{document}`, which the release marks RETIRED")]
    Retired {
        /// The cited source.
        file: String,
        /// The retired document.
        document: String,
    },
    /// The release states no status for the document the source belongs to.
    #[error("the cited text has no release status: {0}")]
    NoStatus(String),
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
/// reading each file once and resolving each cited section once.
#[derive(Debug)]
pub struct QuoteChecker<'a> {
    repo_root: &'a Path,
    texts: BTreeMap<String, Result<String, String>>,
    /// What each `(source, section)` resolved so far holds, or why it does
    /// not resolve.
    sections: BTreeMap<(String, String), Result<Resolved, CiteError>>,
    /// The status the release gives each cited document.
    maturities: Maturities,
}

/// What a cited section holds.
#[derive(Debug, Clone, PartialEq)]
enum Resolved {
    /// Text, whitespace-normalized, that a quote must occur in.
    Text(String),
    /// A number, boolean or `null` leaf of a YAML or JSON source, which a
    /// quote must equal as JSON.
    Leaf(Value),
}

/// Whether `quote`, read as JSON text, is the value `leaf`.
///
/// Two numbers are equal when they are the same JSON number, so `1` and
/// `1.0` both match a leaf `1`, while a quote that does not parse as JSON
/// matches nothing.
fn leaf_matches(leaf: &Value, quote: &str) -> bool {
    let Ok(quoted) = serde_json::from_str::<Value>(quote.trim()) else {
        return false;
    };
    match (leaf, &quoted) {
        (Value::Number(a), Value::Number(b)) if a.is_f64() || b.is_f64() => {
            match (a.as_f64(), b.as_f64()) {
                (Some(a), Some(b)) => a.total_cmp(&b).is_eq(),
                _ => false,
            }
        }
        _ => *leaf == quoted,
    }
}

impl<'a> QuoteChecker<'a> {
    /// A checker over the repository whose root is `repo_root`.
    #[must_use]
    pub fn new(repo_root: &'a Path) -> Self {
        Self {
            repo_root,
            texts: BTreeMap::new(),
            sections: BTreeMap::new(),
            maturities: Maturities::new(repo_root),
        }
    }

    /// Checks that `citation` names an oracle file, that its section exists
    /// there, and that its quote occurs verbatim in that section after
    /// whitespace normalization.
    ///
    /// In a YAML or JSON source (an OAS or a published JSON Schema),
    /// `section` is a top-level key, or a JSON Pointer when it starts with
    /// `/`. The quote must occur in the string content of that element, or,
    /// when the element is a number, a boolean or `null`, the quote read as
    /// JSON must be that value. Any other source is docs text:
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
        match self.maturities.of(source) {
            Ok(Maturity::Document {
                document,
                status: SpecStatus::Retired,
                ..
            }) => {
                return Err(CiteError::Retired {
                    file: source.to_owned(),
                    document,
                });
            }
            // NOTE: no openEHR spec governs this — our own design; a tree without the vendored
            // manifests has no status to judge, and the committed tree always carries them.
            Ok(_) | Err(MaturityError::NoManifest(_)) => {}
            Err(error) => return Err(CiteError::NoStatus(error.to_string())),
        }
        let key = (source.to_owned(), citation.section.clone());
        let resolved = if let Some(resolved) = self.sections.get(&key) {
            resolved.clone()?
        } else {
            let resolved = self.resolve(source, &citation.section);
            self.sections.insert(key, resolved.clone());
            resolved?
        };
        let found = match &resolved {
            Resolved::Text(haystack) => haystack.contains(&normalize_whitespace(&citation.quote)),
            Resolved::Leaf(leaf) => leaf_matches(leaf, &citation.quote),
        };
        if found {
            Ok(())
        } else {
            Err(CiteError::QuoteNotFound {
                file: source.to_owned(),
                section: citation.section.clone(),
                quote: citation.quote.clone(),
            })
        }
    }

    /// What `section` in `source` holds.
    fn resolve(&mut self, source: &str, section: &str) -> Result<Resolved, CiteError> {
        let text = self.read(source)?;
        let extension = Path::new(source)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .map(str::to_ascii_lowercase);
        let structured = match extension.as_deref() {
            Some("yaml" | "yml") => Some(
                crate::load::yaml_str_to_value(&text, Path::new(source)).map_err(|error| {
                    CiteError::Unreadable {
                        file: source.to_owned(),
                        reason: error.detail(),
                    }
                })?,
            ),
            Some("json") => Some(serde_json::from_str::<Value>(&text).map_err(|error| {
                CiteError::Unreadable {
                    file: source.to_owned(),
                    reason: error.to_string(),
                }
            })?),
            _ => None,
        };
        let haystack = if let Some(document) = structured {
            match element(&document, source, section)? {
                leaf @ (Value::Null | Value::Bool(_) | Value::Number(_)) => {
                    return Ok(Resolved::Leaf(leaf.clone()));
                }
                element => {
                    let mut out = String::new();
                    collect_strings(element, &mut out);
                    out
                }
            }
        } else {
            heading_section_text(&text, section).map_err(|miss| match miss {
                SectionMiss::NotFound => CiteError::SectionNotFound {
                    file: source.to_owned(),
                    section: section.to_owned(),
                },
                SectionMiss::Ambiguous(headings) => CiteError::AmbiguousSection {
                    file: source.to_owned(),
                    section: section.to_owned(),
                    headings,
                },
            })?
        };
        Ok(Resolved::Text(normalize_whitespace(&haystack)))
    }

    fn read(&mut self, source: &str) -> Result<String, CiteError> {
        let root = self.repo_root;
        let cached = self.texts.entry(source.to_owned()).or_insert_with(|| {
            let text = std::fs::read_to_string(root.join(source)).map_err(|e| e.to_string())?;
            let is_adoc = Path::new(source)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("adoc"));
            Ok(if is_adoc {
                expand_listing_includes(&text, &root.join(source))
            } else {
                text
            })
        });
        cached.clone().map_err(|reason| CiteError::Unreadable {
            file: source.to_owned(),
            reason,
        })
    }
}

/// `text` of the `AsciiDoc` file at `path`, with each `include::` directive
/// that names a file other than `AsciiDoc` by a plain relative path replaced
/// by that file's content, the way the release renders it.
///
/// QUERY `docs/AQL/master07-grammar.adoc` publishes the AQL grammar as two
/// such includes (`include::grammar/AqlLexer.g4[]`), so a lexer or parser
/// rule is quotable under the chapter heading that renders it. An include of
/// another `AsciiDoc` file, one whose target names a document attribute, one
/// that leaves the including file's directory, or one that does not read
/// stays the directive line it is: each would change the headings a section
/// resolves against, or name text the vendored tree does not hold.
fn expand_listing_includes(text: &str, path: &Path) -> String {
    let Some(dir) = path.parent() else {
        return text.to_owned();
    };
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let included = line
            .trim_end()
            .strip_prefix("include::")
            .and_then(|rest| rest.strip_suffix(']'))
            .and_then(|rest| rest.split_once('['))
            .map(|(target, _)| Path::new(target))
            .filter(|target| {
                !target.as_os_str().is_empty()
                    && target
                        .components()
                        .all(|c| matches!(c, Component::Normal(_)))
                    && !target.to_string_lossy().contains('{')
                    && !target
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("adoc"))
            })
            .and_then(|target| std::fs::read_to_string(dir.join(target)).ok());
        // NOTE: no openEHR spec governs this — our own design; an include that does not read
        // stays its directive line, so a quote of the included text is refused as not found.
        match included {
            Some(content) => out.push_str(&content),
            None => out.push_str(line),
        }
        out.push('\n');
    }
    out
}

/// The element of a YAML or JSON `document` that `section` names: a JSON
/// Pointer when it starts with `/`, a top-level key otherwise.
fn element<'v>(document: &'v Value, source: &str, section: &str) -> Result<&'v Value, CiteError> {
    let element = if section.starts_with('/') {
        document.pointer(section)
    } else {
        document.get(section)
    };
    element.ok_or_else(|| CiteError::SectionNotFound {
        file: source.to_owned(),
        section: section.to_owned(),
    })
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
            level: None,
        }
    }

    #[test]
    fn a_level_must_agree_with_the_keywords_of_its_quote() {
        let should = |quote: &str| Citation {
            level: Some(Level::Should),
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
        assert!(matches!(
            check_level(&must("a lower-case should is no RFC 2119 keyword")),
            Err(CiteError::UnmarkedLevel {
                needed: "should",
                ..
            })
        ));
    }

    #[test]
    fn a_may_citation_carries_a_permission() {
        let at = |level: Level, quote: &str| Citation {
            level: Some(level),
            ..cite("specs/openehr/x.md", "s", quote)
        };
        let signing =
            "At the time of committal of a Version, a digital signature of the object can be made.";
        assert_eq!(check_level(&at(Level::May, signing)), Ok(()));
        assert_eq!(
            check_level(&at(Level::May, "The response MAY include it.")),
            Ok(())
        );
        assert_eq!(
            check_level(&at(Level::May, "The header is OPTIONAL.")),
            Ok(())
        );
        assert!(matches!(
            check_level(&at(Level::May, "It MUST be sent, and it can be weak.")),
            Err(CiteError::LevelMismatch { needed: "must", .. })
        ));
        assert!(matches!(
            check_level(&at(Level::May, "A definition, in the indicative.")),
            Err(CiteError::LevelMismatch { needed: "must", .. })
        ));
        assert!(matches!(
            check_level(&at(Level::Should, signing)),
            Err(CiteError::LevelMismatch { needed: "may", .. })
        ));
        assert!(Level::Should.is_advisory());
        assert!(!Level::Must.is_advisory() && !Level::May.is_advisory());
    }

    #[test]
    fn a_seeded_must_citation_of_a_lower_case_permission_is_refused() {
        let must = |quote: &str| cite("specs/openehr/x.md", "s", quote);
        for (quote, needed) in [
            (
                "At the time of committal of a Version, a digital signature of the object can be made.",
                "may",
            ),
            ("The id may be supplied by the client.", "may"),
            ("Can happen when the supplied id is already used.", "may"),
            ("The endpoint should be always available.", "should"),
        ] {
            assert_eq!(
                check_level(&must(quote)),
                Err(CiteError::UnmarkedLevel {
                    needed,
                    quote: quote.to_owned()
                }),
                "{quote}"
            );
        }
        for requiring in [
            "The endpoint must be available, and it can be cached.",
            "The id shall be unique; a client may supply it.",
            "`baseUrl`: Absolute URL to the root of the API `*(required)*`",
            "The cannot-form is no permission, and nor is can't.",
        ] {
            assert_eq!(check_level(&must(requiring)), Ok(()), "{requiring}");
        }
        let stated = |level: Level, quote: &str| Citation {
            level: Some(level),
            ..must(quote)
        };
        let available = "The endpoint should be always available.";
        assert_eq!(check_level(&stated(Level::Should, available)), Ok(()));
        let written = "The `EHR_STATUS` object itself can always be written to.";
        assert_eq!(
            check_level(&stated(Level::Must, written)),
            Ok(()),
            "a stated `must` records that the sentence requires in other words"
        );
        assert!(matches!(
            check_level(&stated(Level::May, available)),
            Err(CiteError::LevelMismatch {
                needed: "should",
                ..
            })
        ));
        assert!(matches!(
            check_level(&stated(Level::Must, "The response MAY include it.")),
            Err(CiteError::PermissionOnly(_))
        ));
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
    fn a_section_resolved_once_judges_every_later_quote_against_it() {
        let dir = repo();
        let mut checker = QuoteChecker::new(dir.path());
        let file = "specs/openehr/ITS-REST/specifications/responses/409.yaml";
        let found = cite(file, "description", "Can happen when");
        let not_found = cite(file, "description", "Can happen when deleted");
        let missing = cite(file, "summary", "Can happen");
        for _ in 0..2 {
            assert_eq!(checker.check(&found), Ok(()));
            assert!(matches!(
                checker.check(&not_found),
                Err(CiteError::QuoteNotFound { .. })
            ));
            assert!(matches!(
                checker.check(&missing),
                Err(CiteError::SectionNotFound { .. })
            ));
        }
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
            "catalogue/id-map.yaml",
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

    const SCHEMA: &str = "specs/its-json-schemas/x_all.json";

    fn schema_repo() -> assert_fs::TempDir {
        use assert_fs::prelude::*;
        let dir = repo();
        dir.child(SCHEMA)
            .write_str(
                r#"{ "definitions": { "DV_PARAGRAPH": { "properties": { "items": {
                     "type": "array", "minItems": 1, "maxItems": 2.5,
                     "uniqueItems": false, "default": null } } } } }"#,
            )
            .unwrap();
        dir
    }

    #[test]
    fn a_numeric_leaf_is_matched_by_value() {
        let dir = schema_repo();
        let mut checker = QuoteChecker::new(dir.path());
        let items = "/definitions/DV_PARAGRAPH/properties/items";
        for (leaf, quote) in [
            ("minItems", "1"),
            ("minItems", " 1.0 "),
            ("maxItems", "2.5"),
            ("uniqueItems", "false"),
            ("default", "null"),
        ] {
            let at = format!("{items}/{leaf}");
            assert_eq!(checker.check(&cite(SCHEMA, &at, quote)), Ok(()), "{at}");
        }
        assert_eq!(
            checker.check(&cite(SCHEMA, &format!("{items}/type"), "array")),
            Ok(()),
            "a string leaf keeps the substring rule"
        );
    }

    #[test]
    fn a_seeded_numeric_leaf_that_differs_is_refused() {
        let dir = schema_repo();
        let mut checker = QuoteChecker::new(dir.path());
        let min_items = "/definitions/DV_PARAGRAPH/properties/items/minItems";
        for quote in ["0", "2", "\"1\"", "true", "one", "minItems: 1", "10"] {
            assert!(
                matches!(
                    checker.check(&cite(SCHEMA, min_items, quote)),
                    Err(CiteError::QuoteNotFound { .. })
                ),
                "{quote}"
            );
        }
        let unique = "/definitions/DV_PARAGRAPH/properties/items/uniqueItems";
        assert!(matches!(
            checker.check(&cite(SCHEMA, unique, "0")),
            Err(CiteError::QuoteNotFound { .. })
        ));
    }

    #[test]
    fn the_released_dv_paragraph_items_floor_is_quotable() {
        let root = vendored();
        let mut checker = QuoteChecker::new(&root);
        let source = "specs/its-json-schemas/openehr_rm_1.1.0_all.json";
        let min_items = "/definitions/DV_PARAGRAPH/properties/items/minItems";
        assert_eq!(checker.check(&cite(source, min_items, "1")), Ok(()));
        assert!(matches!(
            checker.check(&cite(source, min_items, "0")),
            Err(CiteError::QuoteNotFound { .. })
        ));
    }

    #[test]
    fn a_seeded_schema_source_outside_the_roots_is_refused() {
        use assert_fs::prelude::*;
        let dir = schema_repo();
        dir.child("specs/its-json-schemas-copy/x_all.json")
            .write_str(r#"{ "minItems": 1 }"#)
            .unwrap();
        let mut checker = QuoteChecker::new(dir.path());
        for source in [
            "specs/its-json-schemas-copy/x_all.json",
            "schemas/arazzo-extensions.schema.json",
            "specs/its-json-schemas/../its-json-schemas-copy/x_all.json",
        ] {
            assert!(
                matches!(
                    checker.check(&cite(source, "/minItems", "1")),
                    Err(CiteError::OutsideOracle(_))
                ),
                "{source}"
            );
        }
        assert!(matches!(
            checker.check(&cite("specs/its-xml-schemas/missing.xsd", "x", "y")),
            Err(CiteError::Unreadable { .. })
        ));
    }

    #[test]
    fn a_citation_into_retired_text_is_refused_and_released_text_is_not() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut checker = QuoteChecker::new(&root);
        let retired = cite(
            "specs/openehr/ITS-REST/docs/simplified_data_template/master01-preface.adoc",
            "Purpose",
            "This document describes a 'simplified template' (ST) format",
        );
        assert_eq!(
            checker.check(&retired),
            Err(CiteError::Retired {
                file: retired.source.clone(),
                document: "simplified_data_template".to_owned(),
            })
        );
        let released = cite(
            "specs/openehr/ITS-REST/specifications/docs/overview/Resources.md",
            "Data representation",
            "Additionally, the **Simplified Formats** format SHOULD be supported",
        );
        assert_eq!(checker.check(&released), Ok(()));
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
    fn a_grammar_listing_include_is_quotable_under_its_heading() {
        use assert_fs::prelude::*;
        let dir = repo();
        let docs = "specs/openehr/QUERY/docs/AQL";
        dir.child(format!("{docs}/grammar/Lexer.g4"))
            .write_str("lexer grammar L;\nWS: [ \\t]+ -> skip;\n")
            .unwrap();
        dir.child(format!("{docs}/chapter.adoc"))
            .write_str("== Hidden\n\nnot to be pulled in\n")
            .unwrap();
        dir.child("specs/openehr/QUERY/outside.g4")
            .write_str("OUTSIDE: 'x';\n")
            .unwrap();
        dir.child(format!("{docs}/grammar.adoc"))
            .write_str(
                "= Syntax\n\n.Lexer\n----\ninclude::grammar/Lexer.g4[]\n----\n\ninclude::chapter.adoc[]\ninclude::../outside.g4[]\ninclude::{attr}/Lexer.g4[]\n",
            )
            .unwrap();
        let mut checker = QuoteChecker::new(dir.path());
        let source = format!("{docs}/grammar.adoc");
        assert_eq!(
            checker.check(&cite(&source, "Syntax", "WS: [ \\t]+ -> skip;")),
            Ok(())
        );
        for quote in ["not to be pulled in", "OUTSIDE: 'x';"] {
            assert!(
                matches!(
                    checker.check(&cite(&source, "Syntax", quote)),
                    Err(CiteError::QuoteNotFound { .. })
                ),
                "{quote}"
            );
        }
        assert!(matches!(
            checker.check(&cite(&source, "Hidden", "not to be pulled in")),
            Err(CiteError::SectionNotFound { .. })
        ));
    }

    #[test]
    fn the_released_aql_lexer_rules_are_quotable_from_the_grammar_chapter() {
        let root = vendored();
        let mut checker = QuoteChecker::new(&root);
        let chapter = "specs/openehr/QUERY/docs/AQL/master07-grammar.adoc";
        for quote in [
            "WS: [ \\t\\r\\n]+ -> skip;",
            ") -> channel(COMMENT_CHANNEL);",
        ] {
            assert_eq!(
                checker.check(&cite(chapter, "AQL Syntax Specification", quote)),
                Ok(()),
                "{quote}"
            );
        }
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
