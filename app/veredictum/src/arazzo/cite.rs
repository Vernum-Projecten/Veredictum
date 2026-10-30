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
use crate::arazzo::spec_profile::{DEVELOPMENT_ROOT, SpecProfile};

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
    /// Where the development snapshot states the same requirement in other
    /// words. Read under the development profile only, in place of
    /// `section` and `quote`; the released profile never reads it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub development: Option<Box<DevelopmentText>>,
}

/// The development snapshot's wording of a cited requirement, for a
/// citation whose section the snapshot rewords.
///
/// No openEHR spec governs this — our own design: a case cites the text of
/// the profile it runs under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentText {
    /// The snapshot file, relative to the repository root, when it is not
    /// the file the citation's `source` maps to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The element or the section heading in the snapshot.
    pub section: String,
    /// The sentence, copied verbatim from the snapshot.
    pub quote: String,
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

    /// Whether a failing criterion citing this level yields a warning: a failure against
    /// a sentence that recommends never counts against a capability (RFC 2119
    /// §3). A `may` sentence is asserted like a `must` one, because RFC 2119
    /// §5 obliges an implementation to interoperate with one that takes the
    /// option, and a choice the release leaves to the server is an option a
    /// statement selects.
    #[must_use]
    pub fn is_warning(self) -> bool {
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
    /// The profile vendors no tree for the root the source is under.
    #[error("the {profile} specification profile holds no tree for `{file}`")]
    NotInProfile {
        /// The cited source.
        file: String,
        /// The profile.
        profile: SpecProfile,
    },
    /// A citation's `development` wording does not occur in the snapshot.
    #[error("the development wording of `{file}` does not hold: {reason}")]
    DevelopmentTextNotFound {
        /// The cited source.
        file: String,
        /// Why the wording does not hold.
        reason: String,
    },
    /// A citation's `development` wording is stale: the released quote
    /// still occurs unchanged in the snapshot.
    #[error(
        "the released quote of `{file}` `{section}` occurs unchanged in the development snapshot, so its `development` wording is stale"
    )]
    StaleDevelopmentText {
        /// The cited source.
        file: String,
        /// The cited section.
        section: String,
    },
    /// The profile's tree has no file where the source maps to.
    #[error("the {profile} specification profile has no file `{path}` for `{file}`")]
    AbsentFromProfile {
        /// The cited source.
        file: String,
        /// Where the source maps to under the profile.
        path: String,
        /// The profile.
        profile: SpecProfile,
    },
}

impl CiteError {
    /// Whether the refusal says the cited text is absent from the text read:
    /// no tree or no file for the source, no such section, or no such quote
    /// in it, or a document that text retires.
    #[must_use]
    pub fn is_absence(&self) -> bool {
        matches!(
            self,
            Self::NotInProfile { .. }
                | Self::AbsentFromProfile { .. }
                | Self::SectionNotFound { .. }
                | Self::AmbiguousSection { .. }
                | Self::QuoteNotFound { .. }
                | Self::Retired { .. }
        )
    }
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
    if let Some(wording) = &citation.development
        && (wording.section.trim().is_empty()
            || wording.quote.trim().is_empty()
            || wording
                .source
                .as_deref()
                .is_some_and(|s| s.trim().is_empty()))
    {
        return Err(CiteError::Malformed(
            "`development` needs a non-empty `section` and `quote`, and a non-empty `source` when it names one".to_owned(),
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
    /// The profile whose text every citation is read against.
    profile: SpecProfile,
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
        Self::in_profile(repo_root, SpecProfile::Released)
    }

    /// A checker over the repository whose root is `repo_root`, reading every
    /// citation against `profile`'s text: the source names the released path,
    /// and [`SpecProfile::file_of`] gives the file read.
    #[must_use]
    pub fn in_profile(repo_root: &'a Path, profile: SpecProfile) -> Self {
        Self {
            repo_root,
            profile,
            texts: BTreeMap::new(),
            sections: BTreeMap::new(),
            maturities: Maturities::in_profile(repo_root, profile),
        }
    }

    /// The profile the checker reads.
    #[must_use]
    pub fn profile(&self) -> SpecProfile {
        self.profile
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
        match (&citation.development, self.profile) {
            (Some(wording), SpecProfile::Development) => {
                if self
                    .check_text(source, None, &citation.section, &citation.quote)
                    .is_ok()
                {
                    return Err(CiteError::StaleDevelopmentText {
                        file: source.to_owned(),
                        section: citation.section.clone(),
                    });
                }
                self.check_text(
                    source,
                    wording.source.as_deref(),
                    &wording.section,
                    &wording.quote,
                )
                .map_err(|error| CiteError::DevelopmentTextNotFound {
                    file: source.to_owned(),
                    reason: error.to_string(),
                })
            }
            _ => self.check_text(source, None, &citation.section, &citation.quote),
        }
    }

    /// Checks `quote` under `section` of `source`, read from the file this
    /// checker's profile gives `source`, or from `file` when one is named.
    fn check_text(
        &mut self,
        source: &str,
        file: Option<&str>,
        section: &str,
        quote: &str,
    ) -> Result<(), CiteError> {
        let escapes = Path::new(source)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)));
        if escapes
            || !ORACLE_ROOTS.iter().any(|root| source.starts_with(root))
            || source.starts_with(EXCLUDED_ROOT)
        {
            return Err(CiteError::OutsideOracle(source.to_owned()));
        }
        let named = match file {
            Some(file)
                if file.starts_with(&format!("{DEVELOPMENT_ROOT}/"))
                    && Path::new(file)
                        .components()
                        .all(|c| matches!(c, Component::Normal(_))) =>
            {
                Some(file.to_owned())
            }
            Some(file) => return Err(CiteError::OutsideOracle(file.to_owned())),
            None => self.profile.file_of(self.repo_root, source),
        };
        let Some(path) = named else {
            return Err(CiteError::NotInProfile {
                file: source.to_owned(),
                profile: self.profile,
            });
        };
        // NOTE: no openEHR spec governs this — our own design; the released profile keeps
        // reporting a missing file as unreadable, and only another profile names the absence.
        if self.profile != SpecProfile::Released && !self.repo_root.join(&path).is_file() {
            return Err(CiteError::AbsentFromProfile {
                file: source.to_owned(),
                path,
                profile: self.profile,
            });
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
        let key = (path.clone(), section.to_owned());
        let resolved = if let Some(resolved) = self.sections.get(&key) {
            resolved.clone()?
        } else {
            let resolved = self.resolve(source, &path, section);
            self.sections.insert(key, resolved.clone());
            resolved?
        };
        let found = match &resolved {
            Resolved::Text(haystack) => {
                let quote = normalize_whitespace(quote);
                if self.profile == SpecProfile::Released {
                    haystack.contains(&quote)
                } else {
                    haystack.contains(&rendered_text(&quote))
                }
            }
            Resolved::Leaf(leaf) => leaf_matches(leaf, quote),
        };
        if found {
            Ok(())
        } else {
            Err(CiteError::QuoteNotFound {
                file: source.to_owned(),
                section: section.to_owned(),
                quote: quote.to_owned(),
            })
        }
    }

    /// What `section` in `source`, read from `path`, holds.
    fn resolve(&mut self, source: &str, path: &str, section: &str) -> Result<Resolved, CiteError> {
        let text = self.read(source, path)?;
        let extension = Path::new(path)
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
        let haystack = normalize_whitespace(&haystack);
        // NOTE: no openEHR spec governs this — our own design; only a non-released profile
        // reads the text as rendered, so the released profile's matching stays as strict as it was.
        Ok(Resolved::Text(if self.profile == SpecProfile::Released {
            haystack
        } else {
            rendered_text(&haystack)
        }))
    }

    fn read(&mut self, source: &str, path: &str) -> Result<String, CiteError> {
        let file = self.repo_root.join(path);
        let cached = self.texts.entry(path.to_owned()).or_insert_with(|| {
            let text = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
            let is_adoc = Path::new(source)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("adoc"));
            Ok(if is_adoc {
                expand_listing_includes(&text, &file)
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

/// `text` as its `AsciiDoc` markup renders it, so a quote taken from one
/// markup of a sentence matches another markup of the same visible sentence.
///
/// The rules, and nothing else:
///
/// - `link:<target>[<label>]` and `link:<target>[<label>^]` become `<label>`
///   (<https://docs.asciidoctor.org/asciidoc/latest/macros/link-macro/>,
///   where `^` opens the link in a new window);
/// - `xref:<target>[<label>]` becomes `<label>`
///   (<https://docs.antora.org/antora/latest/page/xref/>);
/// - `<<id,label>>` becomes `label`
///   (<https://docs.asciidoctor.org/asciidoc/latest/macros/xref/>);
/// - a straight double quote `"` becomes a backtick, so `"word"` and
///   `` `word` `` read as the same emphasis.
///
/// A macro with an empty label, a target holding whitespace, a label
/// holding `[`, or a `<<id>>` with no label is kept as written, because its
/// rendering would need the target's own title. No word is ever changed:
/// only macro syntax around a label and the two delimiter characters are.
#[must_use]
pub fn rendered_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        if let Some((label, after)) = macro_label(rest, out.chars().next_back()) {
            out.push_str(label);
            rest = after;
            continue;
        }
        let mut chars = rest.chars();
        if let Some(c) = chars.next() {
            out.push(if c == '"' { '`' } else { c });
        }
        rest = chars.as_str();
    }
    out
}

/// The label of the cross-reference or link macro `rest` starts with, and
/// the text after the macro; `None` when `rest` starts with none, or with one
/// [`rendered_text`] keeps as written. `before` is the character before
/// `rest`: a macro name only starts a macro after a non-alphanumeric one.
fn macro_label(rest: &str, before: Option<char>) -> Option<(&str, &str)> {
    if let Some(inner) = rest.strip_prefix("<<") {
        let (body, after) = inner.split_once(">>")?;
        let (id, label) = body.split_once(',')?;
        let label = label.trim();
        if id.is_empty() || label.is_empty() || body.contains('<') {
            return None;
        }
        return Some((label, after));
    }
    if before.is_some_and(char::is_alphanumeric) {
        return None;
    }
    let macro_body = rest
        .strip_prefix("link:")
        .or_else(|| rest.strip_prefix("xref:"))?;
    let (target, tail) = macro_body.split_once('[')?;
    if target.is_empty() || target.chars().any(char::is_whitespace) {
        return None;
    }
    let (label, after) = tail.split_once(']')?;
    let label = label.strip_suffix('^').unwrap_or(label);
    if label.is_empty() || label.contains('[') {
        return None;
    }
    Some((label, after))
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
            development: None,
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
        assert!(Level::Should.is_warning());
        assert!(!Level::Must.is_warning() && !Level::May.is_warning());
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

    #[test]
    fn the_development_profile_reads_the_snapshot_at_the_same_paths() {
        use assert_fs::prelude::*;
        let dir = assert_fs::TempDir::new().unwrap();
        let source = "specs/openehr/RM/docs/common/master06-change_control_package.adoc";
        dir.child(source)
            .write_str("== Logical Deletion\n\nReleased text only.\n")
            .unwrap();
        dir.child(
            "specs/openehr-development/specifications-RM/docs/common/master06-change_control_package.adoc",
        )
        .write_str("== Logical Deletion\n\nA deleted version is marked.\n\n== Next\n\nelsewhere\n")
        .unwrap();
        let mut released = QuoteChecker::new(dir.path());
        let mut development = QuoteChecker::in_profile(dir.path(), SpecProfile::Development);
        assert_eq!(
            released.check(&cite(source, "Logical Deletion", "Released text only.")),
            Ok(())
        );
        let absent = development.check(&cite(source, "Logical Deletion", "Released text only."));
        assert!(
            matches!(&absent, Err(e @ CiteError::QuoteNotFound { .. }) if e.is_absence()),
            "{absent:?}"
        );
        assert_eq!(
            development.check(&cite(
                source,
                "Logical Deletion",
                "A deleted version is marked."
            )),
            Ok(())
        );
        assert!(
            development
                .check(&cite(source, "Logical Deletion", "elsewhere"))
                .is_err(),
            "the next heading still ends the section"
        );
        let missing = development.check(&cite(
            "specs/openehr/RM/docs/ehr/master04-ehr_package.adoc",
            "EHR",
            "q",
        ));
        assert!(
            matches!(&missing, Err(e @ CiteError::AbsentFromProfile { path, .. })
                if e.is_absence() && path == "specs/openehr-development/specifications-RM/docs/ehr/master04-ehr_package.adoc"),
            "{missing:?}"
        );
        let unmirrored = development.check(&cite(
            "specs/its-xml-schemas/its-xml-1.0.2-nsv1/x.xsd",
            "a",
            "q",
        ));
        assert!(
            matches!(&unmirrored, Err(e @ CiteError::NotInProfile { .. }) if e.is_absence()),
            "{unmirrored:?}"
        );
    }

    #[test]
    fn a_link_macro_renders_as_its_label() {
        assert_eq!(
            rendered_text(
                "`link:/releases/BASE/{base_release}/foundation_types.html#_real_class[Real^]`"
            ),
            "`Real`"
        );
        assert_eq!(
            rendered_text("see link:https://x.org/a[the page] now"),
            "see the page now"
        );
    }

    #[test]
    fn an_xref_macro_renders_as_its_label() {
        assert_eq!(
            rendered_text(
                "|*data*: `xref:{class_history}[HISTORY]<xref:{class_item_structure}[ITEM_STRUCTURE]>`"
            ),
            "|*data*: `HISTORY<ITEM_STRUCTURE>`"
        );
    }

    #[test]
    fn a_labelled_cross_reference_renders_as_its_label() {
        assert_eq!(
            rendered_text("<<_code_phrase_class,CODE_PHRASE>>"),
            "CODE_PHRASE"
        );
        assert_eq!(rendered_text("<<a, spaced label >>"), "spaced label");
    }

    #[test]
    fn the_three_markups_of_one_reference_render_alike() {
        let release = "|*defining_code*: `<<_code_phrase_class,CODE_PHRASE>>`";
        let development = "|*defining_code*: `xref:{class_code_phrase}[CODE_PHRASE]`";
        let linked = "|*defining_code*: `link:/releases/RM/{rm_release}/x.html#_c[CODE_PHRASE^]`";
        assert_eq!(rendered_text(release), rendered_text(development));
        assert_eq!(rendered_text(linked), rendered_text(development));
    }

    #[test]
    fn a_double_quote_and_a_backtick_are_the_same_emphasis() {
        assert_eq!(
            rendered_text(r#"the code for "deleted""#),
            rendered_text("the code for `deleted`")
        );
        assert_eq!(
            rendered_text(r#"say "deleted" twice"#),
            "say `deleted` twice"
        );
    }

    #[test]
    fn what_a_rendering_needs_the_target_for_is_kept_as_written() {
        for kept in [
            "<<_code_phrase_class>>",
            "xref:{class_x}[]",
            "link:a b[label]",
            "text with no macro, 1..1 and [brackets]",
            "unlink:x[y]",
            "<<,label>>",
        ] {
            assert_eq!(rendered_text(kept), kept, "{kept}");
        }
    }

    #[test]
    fn a_different_sentence_never_renders_alike() {
        let differ = |a: &str, b: &str| assert_ne!(rendered_text(a), rendered_text(b), "{a} | {b}");
        differ(
            "h|*1..1* |*magnitude*: `link:/r/x.html#_real_class[Real^]`",
            "h|*0..1* |*magnitude*: `xref:{class_real}[Real]`",
        );
        differ(
            "|*data*: `xref:{class_history}[HISTORY]`",
            "|*data*: `xref:{class_history}[EVENT]`",
        );
        differ("<<_x,DV_TEXT>>", "<<_x,DV_CODED_TEXT>>");
        differ(r#"the code for "deleted""#, "the code for `inactive`");
        differ(
            "The possible values are complete",
            "The possible values are incomplete",
        );
    }

    #[test]
    fn the_released_profile_never_reads_the_rendered_text() {
        use assert_fs::prelude::*;
        let dir = assert_fs::TempDir::new().unwrap();
        let source = "specs/openehr/RM/docs/common/master06-change_control_package.adoc";
        dir.child(source)
            .write_str("== Logical Deletion\n\nthe code for `deleted`\n")
            .unwrap();
        dir.child("specs/openehr-development/specifications-RM/docs/common/master06-change_control_package.adoc")
            .write_str("== Logical Deletion\n\nthe code for `deleted`\n")
            .unwrap();
        let quote = cite(source, "Logical Deletion", r#"the code for "deleted""#);
        assert!(matches!(
            QuoteChecker::new(dir.path()).check(&quote),
            Err(CiteError::QuoteNotFound { .. })
        ));
        assert_eq!(
            QuoteChecker::in_profile(dir.path(), SpecProfile::Development).check(&quote),
            Ok(())
        );
    }

    /// A repository with one released section and its reworded snapshot.
    fn reworded_repo() -> assert_fs::TempDir {
        use assert_fs::prelude::*;
        let dir = assert_fs::TempDir::new().unwrap();
        dir.child("specs/openehr/RM/docs/common/master06-change_control_package.adoc")
            .write_str("== Version Lifecycle\n\nThe possible values are A and B.\n")
            .unwrap();
        dir.child(
            "specs/openehr-development/specifications-RM/docs/common/master06-change_control_package.adoc",
        )
        .write_str("== Version Lifecycle\n\nThe possible values are A, B and C.\n")
        .unwrap();
        dir
    }

    const LIFECYCLE: &str = "specs/openehr/RM/docs/common/master06-change_control_package.adoc";

    fn reworded(quote: &str) -> Citation {
        Citation {
            development: Some(Box::new(DevelopmentText {
                source: None,
                section: "Version Lifecycle".to_owned(),
                quote: quote.to_owned(),
            })),
            ..cite(
                LIFECYCLE,
                "Version Lifecycle",
                "The possible values are A and B.",
            )
        }
    }

    #[test]
    fn a_development_wording_is_read_under_the_development_profile() {
        let dir = reworded_repo();
        let citation = reworded("The possible values are A, B and C.");
        let mut development = QuoteChecker::in_profile(dir.path(), SpecProfile::Development);
        assert_eq!(development.check(&citation), Ok(()));
        let plain = cite(
            LIFECYCLE,
            "Version Lifecycle",
            "The possible values are A and B.",
        );
        assert!(
            development.check(&plain).is_err_and(|e| e.is_absence()),
            "without the wording the released quote is absent from the snapshot"
        );
    }

    #[test]
    fn a_development_wording_is_never_read_under_the_released_profile() {
        let dir = reworded_repo();
        let mut released = QuoteChecker::new(dir.path());
        assert_eq!(
            released.check(&reworded("a wording that occurs nowhere")),
            Ok(()),
            "the released profile checks the released quote alone"
        );
    }

    #[test]
    fn a_development_wording_the_snapshot_lacks_is_refused() {
        let dir = reworded_repo();
        let mut development = QuoteChecker::in_profile(dir.path(), SpecProfile::Development);
        let refused = development.check(&reworded("The possible values are A, B and D."));
        assert!(
            matches!(&refused, Err(e @ CiteError::DevelopmentTextNotFound { .. }) if !e.is_absence()),
            "{refused:?}"
        );
        let elsewhere = Citation {
            development: Some(Box::new(DevelopmentText {
                source: Some(
                    "specs/openehr/RM/docs/common/master06-change_control_package.adoc".to_owned(),
                ),
                section: "Version Lifecycle".to_owned(),
                quote: "The possible values are A and B.".to_owned(),
            })),
            ..cite(LIFECYCLE, "Version Lifecycle", "q")
        };
        assert!(
            matches!(
                development.check(&elsewhere),
                Err(CiteError::DevelopmentTextNotFound { .. })
            ),
            "a development source outside the snapshot is refused"
        );
    }

    #[test]
    fn a_development_wording_whose_released_quote_still_occurs_is_stale() {
        use assert_fs::prelude::*;
        let dir = reworded_repo();
        dir.child(
            "specs/openehr-development/specifications-RM/docs/common/master06-change_control_package.adoc",
        )
        .write_str("== Version Lifecycle\n\nThe possible values are A and B. Also C.\n")
        .unwrap();
        let mut development = QuoteChecker::in_profile(dir.path(), SpecProfile::Development);
        assert_eq!(
            development.check(&reworded("Also C.")),
            Err(CiteError::StaleDevelopmentText {
                file: LIFECYCLE.to_owned(),
                section: "Version Lifecycle".to_owned(),
            })
        );
    }

    #[test]
    fn a_development_wording_needs_its_section_and_quote() {
        let mut extensions = BTreeMap::new();
        extensions.insert(
            CITE_KEY.to_owned(),
            serde_json::json!({ "source": LIFECYCLE, "section": "s", "quote": "q",
                "development": { "section": " ", "quote": "q" } }),
        );
        assert!(matches!(
            citation_of(Some(&extensions)),
            Err(CiteError::Malformed(_))
        ));
    }
}
