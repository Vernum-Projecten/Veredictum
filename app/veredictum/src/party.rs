// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The party artifacts — the two canonical JSON interchange documents a
//! conformance submission carries, plus their supporting value types.
//!
//! ISO/IEC 9646 splits a conformance submission into the supplier's
//! *statement* (the ICS — Implementation Conformance Statement — declaring
//! what is claimed, plus the `SDoC` self-declaration) and the *results* the
//! test campaign produced. This module models the statement as typed,
//! round-trippable JSON ([`Statement`]) and the value types the results of a
//! catalogue run and a certificate record share (the SUT identity, the
//! instrument status). Verdicts are never asserted here; the evaluator
//! computes them from the recorded exchanges.
//!
//! Unlike the schedule artifacts (which are the published norm and carry
//! bespoke closed grammars), the party artifacts are our own interchange
//! contract, so they use plain `serde` derives with authored-order
//! preservation (`serde_json`'s `preserve_order`).

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT"
)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ids::{CapabilityName, CaseId, OptionTag};
use crate::vocab::{FormatName, ItsName, Tier};

/// A party-artifact invariant violation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PartyError {
    /// A `skipped`/`not_applicable` outcome carries no citation (the
    /// schedule's honesty rule: every non-executed verdict names the spec
    /// text or register entry that excuses it).
    #[error("outcome for case {case} (status {status}) is missing its mandatory citation")]
    MissingCitation {
        /// The offending case id.
        case: String,
        /// The status token that requires a citation.
        status: &'static str,
    },
}

// ── the statement (ICS + SDoC) ──────────────────────────────────────────────

/// The system under test as the supplier identifies it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    /// Product/solution name.
    pub name: String,
    /// Product version/release.
    pub version: String,
    /// Vendor/organization.
    pub vendor: String,
    /// A stable product identifier (URI, SKU, …).
    pub identifier: String,
}

/// Declared spec-component versions the SUT implements, keyed by
/// specification component.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpecVersions {
    /// Reference Model version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rm: Option<String>,
    /// BASE version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// Archetype Model version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub am: Option<String>,
    /// AQL/QUERY version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aql: Option<String>,
    /// ITS-REST version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub its_rest: Option<String>,
    /// TERM version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub term: Option<String>,
}

impl SpecVersions {
    /// The declared version string for a spec component, if any.
    #[must_use]
    pub fn get(&self, component: crate::vocab::SpecComponent) -> Option<&str> {
        use crate::vocab::SpecComponent;
        match component {
            SpecComponent::Rm => self.rm.as_deref(),
            SpecComponent::Base => self.base.as_deref(),
            SpecComponent::Am => self.am.as_deref(),
            SpecComponent::Aql => self.aql.as_deref(),
            SpecComponent::ItsRest => self.its_rest.as_deref(),
            SpecComponent::Term => self.term.as_deref(),
        }
    }
}

/// The verdict-bearing claims (the ICS core): the capabilities the supplier
/// asserts, and any profile tier tokens it lists.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Claims {
    /// Verdict-bearing capability names, each the name of a capability
    /// folder under the catalogue's `cases/` (for example `ehr`).
    #[serde(default)]
    pub capabilities: Vec<CapabilityName>,
    /// The profile tier tokens listed (e.g. `CORE`, `SEC-BASIC`), from the
    /// [`Tier`] vocabulary. The catalogue assigns no capability to a tier, so
    /// no selection, gate or verdict reads them.
    #[serde(default)]
    pub profiles: Vec<Tier>,
}

/// A declared technology profile: one ITS and the wire formats exercised on
/// it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechProfile {
    /// The ITS the profile realizes.
    pub its: ItsName,
    /// The wire formats claimed for this ITS.
    #[serde(default)]
    pub formats: Vec<FormatName>,
}

/// Where the technology profile a results document records came from.
///
/// The recorded profile is what the verdict pipeline selects gating records
/// with, so a reader who cannot tell a declaration from a fallback cannot tell
/// how wide the record's claim is. A declared profile is the party's own
/// its-rest format claim; a defaulted one is every format the instrument
/// speaks, which is what a campaign nothing selected has to record so no red
/// row vanishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TechProfileSource {
    /// Read from the party statement's `tech_profiles` entry for this ITS.
    Declared,
    /// No declaration named this ITS, so every format the instrument speaks
    /// was recorded.
    Defaulted,
}

impl TechProfileSource {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[TechProfileSource] =
        &[TechProfileSource::Declared, TechProfileSource::Defaulted];

    /// The source token.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            TechProfileSource::Declared => "declared",
            TechProfileSource::Defaulted => "defaulted",
        }
    }
}

/// The technology profile a results document covers, and where it came from.
///
/// The same two members a party declares ([`TechProfile`]), plus the
/// provenance a reader of the record alone cannot otherwise recover.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedTechProfile {
    /// The ITS the campaign realized.
    pub its: ItsName,
    /// The wire formats the campaign recorded outcomes for.
    #[serde(default)]
    pub formats: Vec<FormatName>,
    /// Whether the formats were declared or defaulted.
    ///
    /// Absent only in a document written before the member existed (v0.1.4 and
    /// earlier), where absence is UNKNOWN and never either source — a reader
    /// that defaults it to `declared` presents a fallback as a party's claim,
    /// which is the misreading the member exists to prevent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<TechProfileSource>,
}

/// One evidence pointer: a results document and its content digest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    /// The results artifact path (relative to the submission bundle).
    pub results_path: String,
    /// The SHA-256 digest of the referenced results artifact.
    pub sha256: String,
}

/// The `SDoC` self-declaration signature block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attestation {
    /// Who signs the declaration.
    pub signatory: String,
    /// Their role.
    pub role: String,
    /// The declaration date (ISO-8601 text; never re-stamped by the runner).
    pub date: String,
    /// The self-declaration statement text.
    pub statement: String,
}

/// The CNF schedule release this catalogue models, which every run stamps into
/// its results and every claim targets.
///
/// ISO/IEC 9646-7 assigns the cells of an ICS proforma other than the support
/// and supported-values columns to the proforma specifier, so the release the
/// form belongs to is the instrument's own fact rather than a supplier's.
pub const SCHEDULE_RELEASE: &str = "cnf-2.0-w2";

/// The party statement: the ICS (claims) + the `SDoC` (self-declaration).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statement {
    /// The system under test.
    pub product: Product,
    /// The schedule release the claim targets.
    pub schedule_release: String,
    /// Declared spec-component versions (the `applies` filter input).
    #[serde(default)]
    pub spec_versions: SpecVersions,
    /// The verdict-bearing claims.
    pub claims: Claims,
    /// Declared technology profiles (per ITS).
    #[serde(default)]
    pub tech_profiles: Vec<TechProfile>,
    /// The ICS option arms: one arm of each option of the catalogue's
    /// `capabilities.yaml`.
    #[serde(default)]
    pub options: Vec<OptionTag>,
    /// Free-form non-functional declarations — never a verdict input.
    #[serde(default)]
    pub non_functional: BTreeMap<String, serde_json::Value>,
    /// Evidence pointers (results artifacts + digests).
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    /// The `SDoC` attestation, if signed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attestation: Option<Attestation>,
}

// ── the results ─────────────────────────────────────────────────────────────

/// Who supplied one recorded identity field.
///
/// No openEHR spec governs this — our own design. The server's own System
/// OPTIONS document (ITS-REST `specifications/operations/options.yaml`,
/// "Options and Conformance") outranks the operator's flag, so a field the
/// server reported is always [`Self::Server`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentitySource {
    /// The server stated the value in its `Options` document.
    Server,
    /// The operator supplied the value, because the server did not state it.
    Operator,
}

impl IdentitySource {
    /// Every source, in vocabulary order (schema emission derives from this).
    pub const ALL: &[IdentitySource] = &[IdentitySource::Server, IdentitySource::Operator];

    /// The token the record spells.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            IdentitySource::Server => "server",
            IdentitySource::Operator => "operator",
        }
    }
}

/// One recorded identity value and who supplied it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityField {
    /// The value.
    pub value: String,
    /// Who supplied it.
    pub source: IdentitySource,
}

impl IdentityField {
    /// A value the server stated.
    #[must_use]
    pub fn server(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            source: IdentitySource::Server,
        }
    }

    /// A value the operator supplied.
    #[must_use]
    pub fn operator(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            source: IdentitySource::Operator,
        }
    }
}

/// The SUT identity as recorded by the run, each field with its source.
///
/// `name` and `version` are always present: the server's `solution` and
/// `solution_version` when it reported them, the operator's otherwise.
/// `vendor` and `restapi_specs_version` are present only when the server
/// reported them, so their source is always [`IdentitySource::Server`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sut {
    /// SUT name.
    pub name: IdentityField,
    /// SUT version.
    pub version: IdentityField,
    /// The vendor the server reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<IdentityField>,
    /// The ITS-REST release the server reported implementing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restapi_specs_version: Option<IdentityField>,
}

impl Sut {
    /// An identity both of whose fields the operator supplied.
    #[must_use]
    pub fn operator(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: IdentityField::operator(name),
            version: IdentityField::operator(version),
            vendor: None,
            restapi_specs_version: None,
        }
    }

    /// The name and version as one display label, `name version`.
    #[must_use]
    pub fn label(&self) -> String {
        format!("{} {}", self.name.value, self.version.value)
    }
}

/// Whether the runner's own verification pack (the self-check that the runner
/// itself is sound) passed for this campaign.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationPackStatus {
    /// The verification pack ran and passed.
    Passed,
    /// The verification pack was not run.
    NotRun,
    /// The verification pack ran and failed (results are untrustworthy).
    Failed,
}

impl VerificationPackStatus {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[VerificationPackStatus] = &[
        VerificationPackStatus::Passed,
        VerificationPackStatus::NotRun,
        VerificationPackStatus::Failed,
    ];
}

/// The approval status of the instrument that produced a document.
///
/// Every 0.x version is [`Self::PreApproval`], and a 1.0.0 or later version
/// is [`Self::SecApproved`]: 1.0.0 marks the openEHR SEC's approval of the
/// instrument. The status is a pure function of the crate's own semver major,
/// so [`Self::CURRENT`] is fixed at compile time and no run can claim another.
/// No openEHR spec governs this — our own design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstrumentStatus {
    /// A 0.x instrument, not yet approved by the openEHR SEC.
    PreApproval,
    /// A 1.0.0 or later instrument, approved by the openEHR SEC.
    SecApproved,
}

/// The crate's semver major, exactly as Cargo sets it.
const VERSION_MAJOR: &str = env!("CARGO_PKG_VERSION_MAJOR");

/// Whether `text` is a semver major: one or more ASCII digits, no leading zero.
const fn is_semver_major(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.is_empty() || (bytes.len() > 1 && matches!(bytes, [b'0', ..])) {
        return false;
    }
    let mut rest = bytes;
    while let [first, tail @ ..] = rest {
        if !first.is_ascii_digit() {
            return false;
        }
        rest = tail;
    }
    true
}

// A major Cargo could not have produced fails the build here, so the match in
// `InstrumentStatus::CURRENT` reads only a well-formed value.
const _: () = assert!(
    is_semver_major(VERSION_MAJOR),
    "CARGO_PKG_VERSION_MAJOR should be a semver major"
);

impl InstrumentStatus {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[InstrumentStatus] =
        &[InstrumentStatus::PreApproval, InstrumentStatus::SecApproved];

    /// The status of this build: [`Self::PreApproval`] for major `0`, and
    /// [`Self::SecApproved`] for every other (well-formed, hence nonzero)
    /// major.
    pub const CURRENT: InstrumentStatus = match VERSION_MAJOR.as_bytes() {
        b"0" => InstrumentStatus::PreApproval,
        _ => InstrumentStatus::SecApproved,
    };

    /// The serialized token, `pre-approval` or `sec-approved`.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            InstrumentStatus::PreApproval => "pre-approval",
            InstrumentStatus::SecApproved => "sec-approved",
        }
    }

    /// The status as one plain sentence fragment for a human reader.
    #[must_use]
    pub const fn summary(self) -> &'static str {
        match self {
            InstrumentStatus::PreApproval => "not yet approved by the openEHR SEC",
            InstrumentStatus::SecApproved => "approved by the openEHR SEC",
        }
    }
}

impl std::fmt::Display for InstrumentStatus {
    /// Writes `<token>: <summary>`, the line `veredictum --version` and the
    /// console banner carry.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.token(), self.summary())
    }
}

/// The version line `veredictum --version` prints after the program name:
/// `version` followed by [`InstrumentStatus::CURRENT`] in parentheses.
///
/// The binary and every consumer that verifies its output build the line
/// here, so the two never disagree on the format.
#[must_use]
pub fn version_line(version: &str) -> String {
    format!("{version} ({})", InstrumentStatus::CURRENT)
}

/// The runner identity + self-check status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Runner {
    /// Runner name.
    pub name: String,
    /// Runner version.
    pub version: String,
    /// The runner's verification-pack status for this campaign.
    pub verification_pack_status: VerificationPackStatus,
}

/// The outcome status of one case×format execution, as recorded in the results.
///
/// ISO/IEC 9646 verdicts: passed→pass, failed→fail, errored→
/// inconclusive; `skipped`/`not_applicable` are selection records each carrying a
/// mandatory citation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    /// Every driven row passed.
    Passed,
    /// At least one row failed.
    Failed,
    /// Inconclusive (transport fault / unmapped response).
    Errored,
    /// Not executed by choice (with citation).
    Skipped,
    /// Not applicable to this SUT/profile (with citation).
    NotApplicable,
}

impl OutcomeStatus {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[OutcomeStatus] = &[
        OutcomeStatus::Passed,
        OutcomeStatus::Failed,
        OutcomeStatus::Errored,
        OutcomeStatus::Skipped,
        OutcomeStatus::NotApplicable,
    ];

    /// The status token.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            OutcomeStatus::Passed => "passed",
            OutcomeStatus::Failed => "failed",
            OutcomeStatus::Errored => "errored",
            OutcomeStatus::Skipped => "skipped",
            OutcomeStatus::NotApplicable => "not_applicable",
        }
    }

    /// Whether this status mandates a citation.
    #[must_use]
    pub fn needs_citation(self) -> bool {
        matches!(self, OutcomeStatus::Skipped | OutcomeStatus::NotApplicable)
    }
}

/// One failed or errored row of a table-driven case.
///
/// It carries the row index (0-based, the content-table order), the step it
/// stopped at (0 = a precondition, postcondition or aggregate), and the
/// reason. The list carrying it says which of the two it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailedRow {
    /// The 0-based row index in the case's parameter table.
    pub row: usize,
    /// The step the row stopped at (0 for a precondition, postcondition or
    /// aggregate).
    pub step: u32,
    /// The failure reason.
    pub reason: String,
}

/// One case×format outcome record: a single rolled-up verdict in the results
/// document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomeRecord {
    /// The case id.
    pub case: CaseId,
    /// The wire format, when the case is format-parameterized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<FormatName>,
    /// The rolled-up status.
    pub status: OutcomeStatus,
    /// Rows driven / rows selected (the printed coverage bound).
    pub rows_driven: usize,
    /// Rows selected.
    pub rows_total: usize,
    /// The first failing/erroring step, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failing_step: Option<u32>,
    /// The failure/error reason, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The excusing citation — mandatory for `skipped`/`not_applicable`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citation: Option<String>,
    /// Every failing row of a table-driven case (empty unless `failed`) —
    /// the per-row evidence a triage needs, not just the first reason.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed_rows: Vec<FailedRow>,
    /// Every errored (inconclusive) row, whatever the rolled-up status: a
    /// failed row outranks an errored one in `status`, and this list keeps the
    /// errored row on the record all the same.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errored_rows: Vec<FailedRow>,
}

impl OutcomeRecord {
    /// The invariant: `skipped`/`not_applicable` records carry a citation.
    ///
    /// # Errors
    /// [`PartyError::MissingCitation`] when the status needs a citation and
    /// none is present.
    pub fn check_invariants(&self) -> Result<(), PartyError> {
        if self.status.needs_citation() && self.citation.as_deref().unwrap_or_default().is_empty() {
            return Err(PartyError::MissingCitation {
                case: self.case.to_string(),
                status: self.status.token(),
            });
        }
        Ok(())
    }
}

/// What ISO/IEC 9646 test selection had to select the campaign with.
///
/// The ICS is the list of components and capabilities a party is answerable
/// for (CNF profiles `master02-overview.adoc` §Overview), so a campaign
/// driven without one is a sweep of the whole catalogue rather than a
/// party-scoped record. A reader of the results document alone must be able
/// to tell the two apart, because the option arms, the extension routes, the
/// claimed capabilities and the release floors are all selected from the ICS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionBasis {
    /// A party statement was supplied, so selection applied the ICS.
    Statement,
    /// No party statement was supplied: nothing selected the party's option
    /// arms, extension routes, claimed capabilities or release floors.
    StatementBlind,
}

impl SelectionBasis {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[SelectionBasis] = &[SelectionBasis::Statement, SelectionBasis::StatementBlind];

    /// The basis token.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            SelectionBasis::Statement => "statement",
            SelectionBasis::StatementBlind => "statement_blind",
        }
    }
}

/// Two provenance members of one results document that cannot both be true, so
/// the conditions of the run cannot be reconstructed from the document at all.
///
/// A record with no provenance members is not this: absence is unknown, and a
/// document written before a member existed carries nothing to contradict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenanceContradiction {
    /// `tech_profile.source` reads `declared` while `selection_basis` reads
    /// `statement_blind`: a campaign nothing selected had no declaration to
    /// read a technology profile from.
    DeclaredProfileWithoutSelection,
}

impl std::fmt::Display for ProvenanceContradiction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            ProvenanceContradiction::DeclaredProfileWithoutSelection => f.write_str(
                "tech_profile.source reads declared and selection_basis reads statement_blind, \
                 and a campaign no statement selected has no declaration to read a technology \
                 profile from",
            ),
        }
    }
}

/// A provenance digest's width in bytes, which renders as twice that many hex
/// characters.
const DIGEST_BYTES: usize = 8;

/// The leading 8 bytes of the SHA-256 over a document's bytes, lowercase hex.
///
/// Nothing is canonicalized, reordered or reformatted first, so every recorded
/// digest is the one `sha256sum <file> | cut -c1-16` prints over the file the
/// campaign was handed.
fn leading_digest(text: &str) -> String {
    use std::fmt::Write as _;

    use sha2::{Digest as _, Sha256};

    Sha256::digest(text.as_bytes())
        .iter()
        .take(DIGEST_BYTES)
        .fold(
            String::with_capacity(DIGEST_BYTES.saturating_mul(2)),
            |mut out, byte| {
                let _ = write!(out, "{byte:02x}");
                out
            },
        )
}

/// Returns the ixit digest a results record carries, which binds the record
/// to the exact declaration it was taken under.
///
/// The digest is the leading 8 bytes of the SHA-256 over the ixit document's
/// bytes exactly as they sit on disk, lowercase hex, so anyone holding the
/// declaration re-derives the recorded value with
/// `sha256sum ixit.json | cut -c1-16`.
///
/// ```
/// use veredictum::party::ixit_digest;
///
/// // `printf '{}' | sha256sum` prints 44136fa355b3678a…
/// assert_eq!(ixit_digest("{}"), "44136fa355b3678a");
/// ```
#[must_use]
pub fn ixit_digest(ixit_text: &str) -> String {
    leading_digest(ixit_text)
}

/// Returns the statement digest a results record carries, which names the
/// exact claim the record was taken under.
///
/// Same shape as [`ixit_digest`], over the statement document's own bytes.
///
/// ```
/// use veredictum::party::statement_digest;
///
/// // `printf '{}' | sha256sum` prints 44136fa355b3678a…
/// assert_eq!(statement_digest("{}"), "44136fa355b3678a");
/// ```
#[must_use]
pub fn statement_digest(statement_text: &str) -> String {
    leading_digest(statement_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_semver_major_is_digits_without_a_leading_zero() {
        for good in ["0", "1", "10", "123"] {
            assert!(is_semver_major(good), "{good} is a semver major");
        }
        for bad in ["", "01", "1a", "-1", "1.0", " 1"] {
            assert!(!is_semver_major(bad), "{bad:?} is not a semver major");
        }
    }

    #[test]
    fn the_current_status_follows_the_crate_major() {
        let expected = if env!("CARGO_PKG_VERSION_MAJOR") == "0" {
            InstrumentStatus::PreApproval
        } else {
            InstrumentStatus::SecApproved
        };
        assert_eq!(InstrumentStatus::CURRENT, expected);
    }

    #[test]
    fn the_status_token_is_its_serde_token() {
        for status in InstrumentStatus::ALL {
            let value = serde_json::to_value(status).unwrap();
            assert_eq!(value, serde_json::Value::String(status.token().to_owned()));
            let back: InstrumentStatus = serde_json::from_value(value).unwrap();
            assert_eq!(back, *status);
        }
        assert_eq!(
            InstrumentStatus::PreApproval.to_string(),
            "pre-approval: not yet approved by the openEHR SEC"
        );
    }

    #[test]
    fn an_unknown_status_token_is_refused() {
        assert!(serde_json::from_str::<InstrumentStatus>("\"approved\"").is_err());
    }

    #[test]
    fn statement_round_trips() {
        let json = serde_json::json!({
            "product": { "name": "FerroEHR", "version": "3.5.0",
                          "vendor": "Vernum Projecten B.V.", "identifier": "urn:rubentalstra:ferroehr" },
            "schedule_release": "CNF-2.0",
            "spec_versions": { "rm": "1.2.0", "its_rest": "1.1.0" },
            "claims": { "capabilities": ["EhrOperations"], "profiles": ["CORE"] },
            "tech_profiles": [ { "its": "its-rest", "formats": ["canonical-json"] } ],
            "options": ["adl14-duplicate-conflict"],
            "evidence": [ { "results_path": "results.json", "sha256": "abc" } ]
        });
        let s: Statement = serde_json::from_value(json).unwrap();
        assert_eq!(s.spec_versions.rm.as_deref(), Some("1.2.0"));
        assert_eq!(s.claims.capabilities.len(), 1);
        let back = serde_json::to_value(&s).unwrap();
        let s2: Statement = serde_json::from_value(back).unwrap();
        assert_eq!(s2.product.name, "FerroEHR");
    }
}
