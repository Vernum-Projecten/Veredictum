// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The results of one Arazzo document's run: one row per workflow row, each
//! in one of four statuses, a red row carrying its four-part explanation.
//!
//! The document is published as `schemas/arazzo-results.schema.json`. It is a
//! pure function of the recording and the catalogue: nothing in it is read
//! from a clock, so re-judging a recording writes the same bytes. No openEHR
//! spec governs the format — our own design.

use std::fmt;
use std::num::NonZeroU64;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::arazzo::cite::Citation;
use crate::arazzo::rows::Row;

/// The four statuses a row ends in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Every criterion held.
    Passed,
    /// A criterion did not hold: the server must change.
    Failed,
    /// The instrument could not judge; this never counts against the server.
    Errored,
    /// The row was not run, for the reason its skip record gives.
    Skipped,
}

impl Status {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[Self] = &[Self::Passed, Self::Failed, Self::Errored, Self::Skipped];
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Passed => "PASSED",
            Self::Failed => "FAILED",
            Self::Errored => "ERRORED",
            Self::Skipped => "SKIPPED",
        })
    }
}

/// Why a row was not run: the closed list a skipped row carries one of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkipReason {
    /// The statement does not claim the case's capability.
    NotClaimed,
    /// The statement selects the other branch of an option.
    OptionNotSelected,
    /// The case applies to spec versions the statement does not declare.
    VersionOutOfRange,
    /// The case needs a format outside the claimed tech profile.
    FormatNotClaimed,
    /// The case drives an extension route the vendor does not declare.
    ExtensionNotOffered,
    /// A value the case needs from the party's IXIT or statement is not
    /// declared.
    NotDeclared,
}

impl SkipReason {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[Self] = &[
        Self::NotClaimed,
        Self::OptionNotSelected,
        Self::VersionOutOfRange,
        Self::FormatNotClaimed,
        Self::ExtensionNotOffered,
        Self::NotDeclared,
    ];
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotClaimed => "not-claimed",
            Self::OptionNotSelected => "option-not-selected",
            Self::VersionOutOfRange => "version-out-of-range",
            Self::FormatNotClaimed => "format-not-claimed",
            Self::ExtensionNotOffered => "extension-not-offered",
            Self::NotDeclared => "not-declared",
        })
    }
}

/// Whose problem a red row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Whose {
    /// The server: a failed row counts against the claimed capability.
    Server,
    /// The instrument could not judge: the row does not count against the
    /// vendor.
    Instrument,
    /// The server did not follow a SHOULD: the row is failed and advisory,
    /// and never counts against a capability.
    Advisory,
}

impl Whose {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[Self] = &[Self::Server, Self::Instrument, Self::Advisory];
}

impl fmt::Display for Whose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Server => "the server: a failed row counts against the claimed capability",
            Self::Instrument => {
                "the instrument could not judge: this row does not count against the vendor"
            }
            Self::Advisory => {
                "advisory: the server did not follow a SHOULD; this row never counts against a capability"
            }
        })
    }
}

/// The released text a citation names, with a link to it upstream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Why {
    /// The vendored file, relative to the repository root.
    pub source: String,
    /// The element or section the quote is in.
    pub section: String,
    /// The sentence that assigns the expectation.
    pub quote: String,
    /// The file at the released commit the vendored copy was taken from,
    /// when its provenance names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// The `upstream-report` issue recording a defect, contradiction or
    /// silence in the cited text, when the citation names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<NonZeroU64>,
}

impl Why {
    /// The citation with its upstream link, read from the provenance of the
    /// vendored tree under `repo_root`.
    #[must_use]
    pub fn of(citation: &Citation, repo_root: &Path) -> Self {
        Self {
            source: citation.source.clone(),
            section: citation.section.clone(),
            quote: citation.quote.clone(),
            link: upstream_link(&citation.source, repo_root),
            report: citation.report,
        }
    }
}

/// The exchange a red row was judged on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExchangeRef {
    /// The 0-based entry of the HAR recording.
    pub entry: usize,
    /// The method sent.
    pub method: String,
    /// The URL sent.
    pub url: String,
    /// The status received; 0 when no response arrived.
    pub status: u16,
}

/// The four-part explanation of a failed or errored row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Explanation {
    /// The step the row went red at.
    pub step: String,
    /// What was checked: the step's description.
    pub what: String,
    /// Why it is required, when a citation resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<Why>,
    /// What the criterion expects.
    pub expected: String,
    /// What the exchange delivered, or why it could not be judged.
    pub received: String,
    /// The exchange judged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exchange: Option<ExchangeRef>,
    /// Whose problem it is.
    pub whose: Whose,
}

/// Why a skipped row was not run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skip {
    /// The reason, from the closed list.
    pub reason: SkipReason,
    /// The citation that makes the case not apply.
    pub why: Why,
}

/// One row of the results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultRow {
    /// The case id, `<document>/<workflowId>`.
    pub case: String,
    /// The 0-based row of the case.
    pub row: usize,
    /// The status.
    pub status: Status,
    /// The inputs the row ran with, generated ones included.
    pub inputs: Row,
    /// The names of the inputs the instrument generated.
    pub generated: Vec<String>,
    /// The skip record, on a skipped row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip: Option<Skip>,
    /// The explanation, on a failed or errored row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<Explanation>,
}

/// The instrument that produced the results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Engine {
    /// Its name.
    pub name: String,
    /// Its version.
    pub version: String,
}

/// The results of one document's run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Results {
    /// The document's name, the prefix of every case id.
    pub document: String,
    /// The instrument.
    pub engine: Engine,
    /// The base URL the run was driven against.
    pub base_url: String,
    /// Every row, in document order.
    pub rows: Vec<ResultRow>,
}

impl Results {
    /// The document as pretty-printed JSON with a trailing newline: the bytes
    /// a re-judgement must reproduce.
    ///
    /// # Errors
    /// The serializer's failure, which a well-formed value never yields.
    pub fn to_text(&self) -> Result<String, serde_json::Error> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        Ok(text)
    }
}

impl fmt::Display for ResultRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = self.status.to_string();
        if let Some(skip) = &self.skip {
            writeln!(f, "{status:<8} {}   {}", self.case, skip.reason)?;
            return write_why(f, Some(&skip.why));
        }
        let Some(why) = &self.explanation else {
            return write!(f, "{status:<8} {}", self.case);
        };
        writeln!(f, "{status:<8} {}   step {}", self.case, why.step)?;
        writeln!(f, "What     {}", why.what)?;
        write_why(f, why.why.as_ref())?;
        writeln!(f)?;
        writeln!(f, "Expected {}", why.expected)?;
        writeln!(f, "Received {}", why.received)?;
        if let Some(exchange) = &why.exchange {
            writeln!(
                f,
                "Exchange {} {} -> {}   (recording entry {})",
                exchange.method, exchange.url, exchange.status, exchange.entry
            )?;
        }
        write!(f, "Whose    {}", why.whose)?;
        match why.why.as_ref().and_then(|w| w.report) {
            Some(report) => write!(
                f,
                "\n         the cited text has a known spec gap: upstream report #{report}"
            ),
            None => Ok(()),
        }
    }
}

fn write_why(f: &mut fmt::Formatter<'_>, why: Option<&Why>) -> fmt::Result {
    match why {
        Some(why) => {
            write!(
                f,
                "Why      {}, {}:\n         \"{}\"",
                why.source, why.section, why.quote
            )?;
            if let Some(link) = &why.link {
                write!(f, "\n         {link}")?;
            }
            Ok(())
        }
        None => write!(f, "Why      (no citation resolved)"),
    }
}

/// The upstream URL of a vendored file at the commit its tree was taken from.
///
/// The tree is `specs/openehr/<component>/` or `specs/rest-oas/`, and its
/// `PROVENANCE.md` names the source repository and the commit. `None` when
/// the file is outside those trees or the provenance names neither.
#[must_use]
pub fn upstream_link(source: &str, repo_root: &Path) -> Option<String> {
    let (tree, inner, upstream_prefix) = if let Some(rest) = source.strip_prefix("specs/openehr/") {
        let (component, inner) = rest.split_once('/')?;
        (format!("specs/openehr/{component}"), inner, String::new())
    } else {
        let inner = source.strip_prefix("specs/rest-oas/")?;
        (
            "specs/rest-oas".to_owned(),
            inner,
            "computable/OAS/".to_owned(),
        )
    };
    // NOTE: no openEHR spec governs this — our own design; a tree without a
    // readable PROVENANCE.md has no upstream to link, so the link is absent.
    let provenance = std::fs::read_to_string(repo_root.join(&tree).join("PROVENANCE.md")).ok()?;
    let field = |names: &[&str]| {
        provenance.lines().find_map(|line| {
            let line = line.trim_start_matches("- ");
            names
                .iter()
                .find_map(|name| line.strip_prefix(name))
                .map(str::trim)
        })
    };
    let repository = field(&["Source repo:", "Source:"])?;
    let commit = field(&["Pinned commit:", "Commit:"])?
        .split('`')
        .nth(1)?
        .to_owned();
    Some(format!(
        "{repository}/blob/{commit}/{upstream_prefix}{inner}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn a_vendored_file_links_to_its_released_commit() {
        assert_eq!(
            upstream_link(
                "specs/openehr/ITS-REST/specifications/responses/409_EHR_with_id.yaml",
                &root()
            )
            .as_deref(),
            Some(
                "https://github.com/openEHR/specifications-ITS-REST/blob/24058992d5fa96e8dfbd855d9c133f328387fc09/specifications/responses/409_EHR_with_id.yaml"
            )
        );
        assert_eq!(
            upstream_link("specs/rest-oas/ehr-validation.openapi.yaml", &root()).as_deref(),
            Some(
                "https://github.com/openEHR/specifications-ITS-REST/blob/24058992d5fa96e8dfbd855d9c133f328387fc09/computable/OAS/ehr-validation.openapi.yaml"
            )
        );
        assert_eq!(upstream_link("artifacts/x.yaml", &root()), None);
    }

    #[test]
    fn a_red_row_renders_its_four_parts() {
        let row = ResultRow {
            case: "ehr/w".to_owned(),
            row: 0,
            status: Status::Failed,
            inputs: Row::new(),
            generated: Vec::new(),
            skip: None,
            explanation: Some(Explanation {
                step: "create-again".to_owned(),
                what: "Create it again.".to_owned(),
                why: Some(Why {
                    source: "s".to_owned(),
                    section: "d".to_owned(),
                    quote: "q".to_owned(),
                    link: None,
                    report: None,
                }),
                expected: "status 409".to_owned(),
                received: "status 201".to_owned(),
                exchange: Some(ExchangeRef {
                    entry: 1,
                    method: "PUT".to_owned(),
                    url: "http://sut/ehr/1".to_owned(),
                    status: 201,
                }),
                whose: Whose::Server,
            }),
        };
        let text = row.to_string();
        assert!(text.starts_with("FAILED   ehr/w   step create-again\nWhat     Create it again.\nWhy      s, d:\n         \"q\"\nExpected status 409\nReceived status 201\nExchange PUT http://sut/ehr/1 -> 201   (recording entry 1)\nWhose    the server"), "{text}");
    }
}
