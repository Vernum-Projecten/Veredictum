// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The results of one Arazzo document's run: one row per workflow row, each
//! in one of four statuses, a red row carrying its four-part explanation.
//!
//! The document is published as `schemas/arazzo-results.schema.json`. It is a
//! pure function of the recording and the catalogue: nothing in it is read
//! from a clock, so re-judging a recording writes the same bytes. No openEHR
//! spec governs the format — our own design.

use std::collections::BTreeMap;
use std::fmt;
use std::num::NonZeroU64;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::arazzo::cite::{Citation, DevelopmentText};
use crate::arazzo::rows::Row;
use crate::arazzo::spec_profile::{DEVELOPMENT_ROOT, SpecProfile, Specification};
use crate::arazzo::structure::Outline;
use crate::identity::IdentityRead;
use crate::party::InstrumentStatus;

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
    /// A setup of the case calls an extension route the statement does not
    /// declare.
    ExtensionNotOffered,
    /// A value the case needs from the party's IXIT or statement is not
    /// declared.
    NotDeclared,
    /// A text the case cites is absent from the specification profile the
    /// run reads.
    CitedTextAbsent,
    /// The case rests on text only the development snapshot holds, and the
    /// run reads the released profile.
    DevelopmentOnly,
    /// The development snapshot keeps the case's cited text unchanged and
    /// contradicts it in another text, recorded by an upstream report, and
    /// the run reads the development profile.
    ContestedInDevelopment,
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
        Self::CitedTextAbsent,
        Self::DevelopmentOnly,
        Self::ContestedInDevelopment,
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
            Self::CitedTextAbsent => "cited-text-absent",
            Self::DevelopmentOnly => "development-only",
            Self::ContestedInDevelopment => "contested-in-development",
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
    /// The server did not follow a SHOULD: the row is failed and a warning,
    /// and never counts against a capability.
    Warning,
}

impl Whose {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[Self] = &[Self::Server, Self::Instrument, Self::Warning];
}

impl fmt::Display for Whose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Server => "the server: a failed row counts against the claimed capability",
            Self::Instrument => {
                "the instrument could not judge: this row does not count against the vendor"
            }
            Self::Warning => {
                "warning: the server did not follow a SHOULD; this row never counts against a capability"
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

/// Points every link of `rows` at the upstream file of the tree `profile`
/// reads, or drops it when the profile holds no such file.
///
/// A citation names the released path under every profile, so a row judged
/// under another profile links to the commit that profile's text was taken
/// from, never to the release. Under the development profile a citation of
/// `citations` that carries its `development` wording is shown in that
/// wording: the case cites the text of the profile it runs under.
pub fn relink(
    rows: &mut [ResultRow],
    repo_root: &Path,
    profile: SpecProfile,
    citations: &[Citation],
) {
    let reworded: BTreeMap<(&str, &str, &str), &DevelopmentText> = citations
        .iter()
        .filter(|_| profile == SpecProfile::Development)
        .filter_map(|c| {
            c.wording().map(|wording| {
                (
                    (c.source.as_str(), c.section.as_str(), c.quote.as_str()),
                    wording,
                )
            })
        })
        .collect();
    let set = |why: &mut Why| {
        let wording = reworded
            .get(&(
                why.source.as_str(),
                why.section.as_str(),
                why.quote.as_str(),
            ))
            .copied();
        if let Some(wording) = wording {
            if let Some(source) = &wording.source {
                why.source.clone_from(source);
            }
            why.section.clone_from(&wording.section);
            why.quote.clone_from(&wording.quote);
        }
        why.link = upstream_link_in(&why.source, repo_root, profile);
    };
    for row in rows {
        if let Some(skip) = row.skip.as_mut() {
            set(&mut skip.why);
        }
        if let Some(why) = row.explanation.as_mut().and_then(|e| e.why.as_mut()) {
            set(why);
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
    /// What the reason leaves to say: for `cited-text-absent`, the profile
    /// and what of the cited text it lacks; for `development-only`, the
    /// development citation the case rests on; for
    /// `contested-in-development`, the upstream report and the contested
    /// citation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
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

/// The results of one document's run, or of a catalogue run over every
/// document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Results {
    /// The document's name, the prefix of every case id; for a catalogue run,
    /// [`crate::arazzo::catalogue::RUN_NAME`], and each case id carries its
    /// own document's name.
    pub document: String,
    /// The instrument.
    pub engine: Engine,
    /// The approval status of the instrument that produced the results.
    pub instrument_status: InstrumentStatus,
    /// The specification text the run was judged against.
    // NOTE: no openEHR spec governs this — our own design; a record that predates the profiles
    // was judged against the release, so a record without the member reads as released.
    #[serde(default)]
    pub specification: Specification,
    /// The base URL the run was driven against.
    pub base_url: String,
    /// What the server stated about itself through System OPTIONS, on a
    /// catalogue run; a single document's run reads no identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<ServerIdentity>,
    /// What the run's static review found when it compared the statement
    /// with [`Results::server`]. A note never changes a row's status.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub review: Vec<ReviewNote>,
    /// Every row, in document order.
    pub rows: Vec<ResultRow>,
    /// The reading structure the catalogue declares, with where each case
    /// sits; absent when the catalogue declares none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<Outline>,
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

/// What the server stated about itself through ITS-REST System OPTIONS
/// (`specifications/operations/options.yaml`), and where the answer was read.
///
/// The operation says services SHOULD answer, so no outcome here is a
/// finding and none counts toward conformance. A member the server did not
/// state is absent; nothing fills it in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "read", rename_all = "snake_case")]
pub enum ServerIdentity {
    /// The run's own System OPTIONS exchange, a workflow's step.
    Exchange {
        /// The 0-based entry of the HAR recording that holds the exchange.
        entry: usize,
        /// The classified answer.
        answer: IdentityRead,
    },
    /// The read the instrument made after the catalogue ran, because the
    /// run exchanged no System OPTIONS; the recording's log carries it.
    Read {
        /// The classified answer.
        answer: IdentityRead,
    },
    /// The read the instrument made was not answered; the recording's log
    /// carries the failure.
    Unanswered {
        /// The transport's own failure.
        error: String,
    },
    /// A re-judged recording that carries no System OPTIONS answer.
    NotRecorded,
}

impl ServerIdentity {
    /// The answer, when one was read.
    #[must_use]
    pub fn answer(&self) -> Option<&IdentityRead> {
        match self {
            Self::Exchange { answer, .. } | Self::Read { answer } => Some(answer),
            Self::Unanswered { .. } | Self::NotRecorded => None,
        }
    }

    /// The `restapi_specs_version` the server stated, when it stated one
    /// that is not blank.
    #[must_use]
    pub fn restapi_specs_version(&self) -> Option<&str> {
        self.answer()
            .and_then(IdentityRead::options)
            .and_then(|options| options.restapi_specs_version.as_deref())
            .filter(|version| !version.trim().is_empty())
    }
}

/// What a static-review note compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewKind {
    /// The statement's `spec_versions.its_rest` differs from the
    /// `restapi_specs_version` the server stated.
    RestapiVersionDiffers,
}

impl ReviewKind {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[Self] = &[Self::RestapiVersionDiffers];
}

impl fmt::Display for ReviewKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::RestapiVersionDiffers => "restapi-version-differs",
        })
    }
}

/// One static-review note: the statement against what the server stated.
///
/// No openEHR spec governs the comparison — our own design. The note is a
/// record for the reader of the results; it never changes a row's status or
/// the run's exit status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewNote {
    /// What is compared.
    pub kind: ReviewKind,
    /// The value the statement declares.
    pub declared: String,
    /// The value the server stated.
    pub stated: String,
}

impl ReviewNote {
    /// Compares the statement's declared ITS-REST release with what `server`
    /// stated: one note when the server stated a `restapi_specs_version` that
    /// differs from `declared`, none otherwise.
    ///
    /// A version the server did not state, or a release the statement does
    /// not declare, leaves nothing to compare. The texts are compared as
    /// written, so `1.1` and `1.1.0` differ.
    #[must_use]
    pub fn review(declared: Option<&str>, server: &ServerIdentity) -> Vec<Self> {
        match (declared, server.restapi_specs_version()) {
            (Some(declared), Some(stated)) if declared != stated => vec![Self {
                kind: ReviewKind::RestapiVersionDiffers,
                declared: declared.to_owned(),
                stated: stated.to_owned(),
            }],
            _ => Vec::new(),
        }
    }
}

impl fmt::Display for ReviewNote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            ReviewKind::RestapiVersionDiffers => write!(
                f,
                "REVIEW   {}   the statement declares ITS-REST {}, and the server states \
                 restapi_specs_version \"{}\" through System OPTIONS; no row depends on this",
                self.kind, self.declared, self.stated
            ),
        }
    }
}

impl fmt::Display for ResultRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = self.status.to_string();
        if let Some(skip) = &self.skip {
            writeln!(f, "{status:<8} {}   {}", self.case, skip.reason)?;
            if let Some(detail) = &skip.detail {
                writeln!(f, "Missing  {detail}")?;
            }
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
/// The tree is `specs/openehr/<component>/`, `specs/rest-oas/`,
/// `specs/its-json-schemas/` or a repository directory of the development
/// snapshot, and its `PROVENANCE.md` names the source repository and the
/// commit. `None` when the file is outside those trees or the provenance
/// names neither.
#[must_use]
pub fn upstream_link(source: &str, repo_root: &Path) -> Option<String> {
    upstream_link_in(source, repo_root, SpecProfile::Released)
}

/// The upstream URL of the file `profile` reads for the cited `source`, at
/// the commit that profile's tree was taken from.
///
/// Under [`SpecProfile::Released`] it is [`upstream_link`]. Under another
/// profile the source maps onto that profile's tree
/// ([`SpecProfile::file_of`]), whose provenance names the commit; `None`
/// when the profile holds no such file. A development-only source links to
/// the snapshot under every profile.
#[must_use]
pub fn upstream_link_in(source: &str, repo_root: &Path, profile: SpecProfile) -> Option<String> {
    let development_only = source.starts_with(&format!("{DEVELOPMENT_ROOT}/"));
    if profile != SpecProfile::Released || development_only {
        let path = if development_only {
            source.to_owned()
        } else {
            profile.file_of(repo_root, source)?
        };
        if !repo_root.join(&path).is_file() {
            return None;
        }
        let (directory, inner) = path
            .strip_prefix(DEVELOPMENT_ROOT)?
            .strip_prefix('/')?
            .split_once('/')?;
        return provenance_link(
            repo_root,
            &format!("{DEVELOPMENT_ROOT}/{directory}"),
            inner,
            "",
        );
    }
    let (tree, inner, upstream_prefix) = if let Some(rest) = source.strip_prefix("specs/openehr/") {
        let (component, inner) = rest.split_once('/')?;
        (format!("specs/openehr/{component}"), inner, String::new())
    } else if let Some(inner) = source.strip_prefix("specs/its-json-schemas/") {
        (
            "specs/its-json-schemas".to_owned(),
            inner,
            "components/".to_owned(),
        )
    } else {
        let inner = source.strip_prefix("specs/rest-oas/")?;
        (
            "specs/rest-oas".to_owned(),
            inner,
            "computable/OAS/".to_owned(),
        )
    };
    provenance_link(repo_root, &tree, inner, &upstream_prefix)
}

/// The upstream URL of `inner` in the vendored `tree`, read from the tree's
/// `PROVENANCE.md`, with `upstream_prefix` before it upstream.
fn provenance_link(
    repo_root: &Path,
    tree: &str,
    inner: &str,
    upstream_prefix: &str,
) -> Option<String> {
    // NOTE: no openEHR spec governs this — our own design; a tree without a
    // readable PROVENANCE.md has no upstream to link, so the link is absent.
    let provenance = std::fs::read_to_string(repo_root.join(tree).join("PROVENANCE.md")).ok()?;
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
    use crate::arazzo::cite::Development;

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
        assert_eq!(
            upstream_link("specs/its-json-schemas/openehr_rm_1.1.0_all.json", &root()).as_deref(),
            Some(
                "https://github.com/openEHR/specifications-ITS-JSON/blob/5acae056248e917a4b4c56f7e712f4fcfeb616a6/components/openehr_rm_1.1.0_all.json"
            )
        );
        assert_eq!(upstream_link("artifacts/x.yaml", &root()), None);
    }

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
    )]
    fn a_development_only_file_links_to_the_snapshot_under_every_profile()
    -> Result<(), Box<dyn std::error::Error>> {
        let source = "specs/openehr-development/specifications-ITS-XML/components/AM/Release-1.4/Template.xsd";
        let link = upstream_link(source, &root()).ok_or("the snapshot file links")?;
        assert!(
            link.starts_with("https://github.com/openEHR/specifications-ITS-XML/blob/")
                && link.ends_with("/components/AM/Release-1.4/Template.xsd"),
            "{link}"
        );
        assert_eq!(
            upstream_link_in(source, &root(), SpecProfile::Development).as_deref(),
            Some(link.as_str())
        );
        Ok(())
    }

    fn stating(version: Option<&str>) -> ServerIdentity {
        ServerIdentity::Read {
            answer: IdentityRead::Offered {
                status: 200,
                options: crate::identity::OptionsDocument {
                    restapi_specs_version: version.map(str::to_owned),
                    ..crate::identity::OptionsDocument::default()
                },
            },
        }
    }

    #[test]
    fn only_a_stated_release_that_differs_from_the_declared_one_is_noted() {
        assert_eq!(
            ReviewNote::review(Some("1.1.0"), &stating(Some("1.0.3"))),
            [ReviewNote {
                kind: ReviewKind::RestapiVersionDiffers,
                declared: "1.1.0".to_owned(),
                stated: "1.0.3".to_owned(),
            }]
        );
        assert_eq!(
            ReviewNote::review(Some("1.1.0"), &stating(Some("1.1"))).len(),
            1,
            "the texts are compared as written"
        );
        assert_eq!(
            ReviewNote::review(Some("1.1.0"), &stating(Some("1.1.0"))),
            []
        );
        assert_eq!(ReviewNote::review(None, &stating(Some("1.0.3"))), []);
        assert_eq!(ReviewNote::review(Some("1.1.0"), &stating(None)), []);
        assert_eq!(ReviewNote::review(Some("1.1.0"), &stating(Some(" "))), []);
        assert_eq!(
            ReviewNote::review(Some("1.1.0"), &ServerIdentity::NotRecorded),
            []
        );
        let offered_nothing = ServerIdentity::Exchange {
            entry: 0,
            answer: IdentityRead::NotOffered { status: 405 },
        };
        assert_eq!(offered_nothing.restapi_specs_version(), None);
        assert_eq!(ReviewNote::review(Some("1.1.0"), &offered_nothing), []);
    }

    #[test]
    fn a_server_identity_round_trips_under_its_read_tag() {
        let identity = stating(Some("1.0.3"));
        let text = serde_json::to_value(&identity).unwrap();
        assert_eq!(
            text,
            serde_json::json!({
                "read": "read",
                "answer": { "outcome": "offered", "status": 200,
                            "options": { "restapi_specs_version": "1.0.3" } }
            })
        );
        assert_eq!(
            serde_json::from_value::<ServerIdentity>(text).unwrap(),
            identity
        );
        assert!(
            serde_json::from_value::<ServerIdentity>(serde_json::json!({ "read": "guessed" }))
                .is_err(),
            "an unknown read is refused"
        );
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

    #[test]
    fn a_development_row_shows_the_development_wording_of_its_citation() {
        let source = "specs/openehr/RM/docs/common/master06-change_control_package.adoc";
        let released_quote =
            "The possible values are `532|complete|`, `553|incomplete|` and `523|deleted|`.";
        let citation = Citation {
            source: source.to_owned(),
            section: "Version Lifecycle".to_owned(),
            quote: released_quote.to_owned(),
            report: None,
            level: None,
            development: Some(Box::new(Development::Reworded(DevelopmentText {
                source: None,
                section: "Version Lifecycle".to_owned(),
                quote: "the snapshot's wording".to_owned(),
            }))),
        };
        let row = || ResultRow {
            case: "c/w".to_owned(),
            row: 0,
            status: Status::Skipped,
            inputs: Row::new(),
            generated: Vec::new(),
            skip: Some(Skip {
                reason: SkipReason::NotClaimed,
                why: Why::of(&citation, &root()),
                detail: None,
            }),
            explanation: None,
        };
        let mut development = [row()];
        relink(
            &mut development,
            &root(),
            SpecProfile::Development,
            std::slice::from_ref(&citation),
        );
        let why = &development[0].skip.as_ref().unwrap().why;
        assert_eq!(why.quote, "the snapshot's wording");
        assert_eq!(why.source, source);
        assert!(
            why.link
                .as_deref()
                .is_some_and(|l| l.contains("specifications-RM/blob/654296bb027b")),
            "{why:?}"
        );
        let mut released = [row()];
        relink(
            &mut released,
            &root(),
            SpecProfile::Released,
            std::slice::from_ref(&citation),
        );
        assert_eq!(released[0].skip.as_ref().unwrap().why.quote, released_quote);
    }
}
