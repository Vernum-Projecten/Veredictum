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

use crate::arazzo::extension::ExtensionKey;
use crate::ids::{CapabilityName, OptionTag};
use crate::vocab::{FormatName, ItsName};

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
/// asserts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Claims {
    /// Verdict-bearing capability names, each the name of a capability
    /// folder under the catalogue's `cases/` (for example `ehr`).
    #[serde(default)]
    pub capabilities: Vec<CapabilityName>,
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

/// One extension route the party offers: its own OpenAPI document and the
/// operation in it that realises the extension.
///
/// No openEHR spec governs this — our own design: the route only prepares
/// ground for a case in a shared setup, and no verdict judges its response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclaredExtension {
    /// The party's OpenAPI document, a path relative to the statement.
    pub document: String,
    /// The `operationId` in that document that realises the extension.
    pub operation_id: String,
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
    /// The extension routes the party offers, by the extension each
    /// realises; an extension the statement does not declare is not offered.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<ExtensionKey, DeclaredExtension>,
    /// Evidence pointers (results artifacts + digests).
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    /// The `SDoC` attestation, if signed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attestation: Option<Attestation>,
}

// ── the recorded identity and the instrument status ───────────────────────

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
            "claims": { "capabilities": ["EhrOperations"] },
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

    #[test]
    fn a_statement_declares_extension_routes_by_a_closed_key() {
        let mut json = serde_json::json!({
            "product": { "name": "p", "version": "1", "vendor": "v", "identifier": "i" },
            "schedule_release": "cnf-2.0-w2",
            "claims": { "capabilities": ["ehr"] },
            "extensions": {
                "ehr-import": { "document": "party/import.yaml", "operation_id": "import_ehr" }
            }
        });
        let s: Statement = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(
            s.extensions.get(&ExtensionKey::EhrImport),
            Some(&DeclaredExtension {
                document: "party/import.yaml".to_owned(),
                operation_id: "import_ehr".to_owned(),
            })
        );
        assert_eq!(
            serde_json::to_value(&s).unwrap()["extensions"],
            json["extensions"]
        );
        json["extensions"] = serde_json::json!({
            "ehr-export": { "document": "party/import.yaml", "operation_id": "import_ehr" }
        });
        let unknown = serde_json::from_value::<Statement>(json.clone()).unwrap_err();
        assert!(unknown.to_string().contains("ehr-export"), "{unknown}");
        json["extensions"] = serde_json::json!({
            "ehr-import": { "document": "party/import.yaml", "operation": "import_ehr" }
        });
        assert!(serde_json::from_value::<Statement>(json).is_err());
        let without: Statement = serde_json::from_value(serde_json::json!({
            "product": { "name": "p", "version": "1", "vendor": "v", "identifier": "i" },
            "schedule_release": "cnf-2.0-w2",
            "claims": { "capabilities": ["ehr"] }
        }))
        .unwrap();
        assert!(without.extensions.is_empty());
        assert!(
            serde_json::to_value(&without)
                .unwrap()
                .get("extensions")
                .is_none()
        );
    }
}
