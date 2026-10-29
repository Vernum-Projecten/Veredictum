// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The closed vocabularies the instrument's documents use.
//!
//! A value outside these enums is unrepresentable in the typed model, so a
//! typo in a document is a loud error rather than a silent default.

use serde::{Deserialize, Serialize};

/// The schedule component a case belongs to (the chapter taxonomy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Component {
    /// EHR lifecycle and `EHR_STATUS` (ITS-REST EHR API).
    Ehr,
    /// COMPOSITION commit/read/versioning (ITS-REST EHR API).
    EhrComposition,
    /// CONTRIBUTION commit and retrieval (ITS-REST EHR API).
    EhrContribution,
    /// The EHR's FOLDER directory (ITS-REST EHR API).
    EhrDirectory,
    /// ADL 1.4 template definitions — OPT upload/list/get (ITS-REST Definition API).
    #[serde(rename = "DEFINITION_ADL14")]
    DefinitionAdl14,
    /// ADL 2 archetype/template definitions (ITS-REST Definition API).
    #[serde(rename = "DEFINITION_ADL2")]
    DefinitionAdl2,
    /// Stored-query definitions (ITS-REST Definition API).
    DefinitionQuery,
    /// AQL execution — ad-hoc and stored (ITS-REST Query API).
    Query,
    /// Demographic parties and relationships (ITS-REST Demographic API).
    Demographic,
    /// The administrative surface (ITS-REST Admin API).
    Admin,
    /// EHR-Extract / TDD messaging (no released ITS-REST API).
    Messaging,
    /// RM content semantics asserted per data type, independent of one
    /// endpoint — the template-parameterized decision-table chapter.
    Content,
    /// The Simplified Formats sub-specification: FLAT, STRUCTURED, Web Template.
    SimplifiedFormats,
    /// Authentication, authorization and audit behaviour of the served API.
    Security,
    /// Measured performance: the volumetric deployment classes.
    Performance,
}

/// A capability family (the certificate's rating dimensions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Family {
    /// The platform (CDR) profile family: CORE/STANDARD/OPTIONS.
    Platform,
    /// The proposed Enterprise extension: D/M/X.
    Enterprise,
    /// The Security & Privacy family: SEC-BASIC (higher rungs are future SEC work).
    Security,
}

/// A family-scoped profile tier. Each tier knows its family so a
/// capability-matrix row declaring a foreign tier is a typed error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Tier {
    /// Platform CORE: the capabilities every conformant CDR must serve.
    #[serde(rename = "CORE")]
    Core,
    /// Platform STANDARD: capabilities expected of a full platform above CORE.
    #[serde(rename = "STANDARD")]
    Standard,
    /// Platform OPTIONS: capabilities a CDR may serve, rated when present.
    #[serde(rename = "OPTIONS")]
    Options,
    /// Security & Privacy basic rung: authentication, authorization, audit.
    #[serde(rename = "SEC-BASIC")]
    SecBasic,
    /// Enterprise D (the proposed Enterprise family's data-management rung).
    #[serde(rename = "D")]
    EnterpriseD,
    /// Enterprise M (the proposed Enterprise family's management rung).
    #[serde(rename = "M")]
    EnterpriseM,
    /// Enterprise X (the proposed Enterprise family's extension rung).
    #[serde(rename = "X")]
    EnterpriseX,
}

impl Tier {
    /// The family this tier is scoped to.
    #[must_use]
    pub fn family(self) -> Family {
        match self {
            Tier::Core | Tier::Standard | Tier::Options => Family::Platform,
            Tier::SecBasic => Family::Security,
            Tier::EnterpriseD | Tier::EnterpriseM | Tier::EnterpriseX => Family::Enterprise,
        }
    }
}

/// A wire format role (the §8.7 format axes; media types per the ITS-REST
/// `Accept_*`/`ContentType_*` parameter files).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum FormatName {
    /// Canonical openEHR JSON (`application/json`).
    #[serde(rename = "canonical-json")]
    CanonicalJson,
    /// Canonical openEHR XML (`application/xml`).
    #[serde(rename = "canonical-xml")]
    CanonicalXml,
    /// Simplified FLAT JSON (`application/openehr.wt.flat+json`).
    #[serde(rename = "wt-flat")]
    WtFlat,
    /// Simplified STRUCTURED JSON (`application/openehr.wt.structured+json`).
    #[serde(rename = "wt-structured")]
    WtStructured,
    /// The Web Template itself (`application/openehr.wt+json`, template GET only).
    #[serde(rename = "wt")]
    Wt,
}

/// A corpus payload format — the wire roles plus the template-source form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CorpusFormat {
    /// Canonical openEHR JSON payload.
    #[serde(rename = "canonical-json")]
    CanonicalJson,
    /// Canonical openEHR XML payload.
    #[serde(rename = "canonical-xml")]
    CanonicalXml,
    /// Simplified FLAT JSON payload.
    #[serde(rename = "wt-flat")]
    WtFlat,
    /// Simplified STRUCTURED JSON payload.
    #[serde(rename = "wt-structured")]
    WtStructured,
    /// An ADL 1.4 operational template (OPT XML).
    #[serde(rename = "opt-xml")]
    OptXml,
    /// AQL query text (stored-query definitions).
    #[serde(rename = "aql-text")]
    AqlText,
    /// ADL2 artefact source text (archetypes/templates/OPTs in ADL syntax).
    #[serde(rename = "adl2-text")]
    Adl2Text,
    /// ADL 1.4 archetype source text (the `.adl` form the ADL 1.4
    /// archetype-provisioning extension routes exchange as `text/plain`).
    #[serde(rename = "adl14-text")]
    Adl14Text,
    /// A JSON payload delivered BYTE-FOR-BYTE, unparsed.
    ///
    /// `canonical-json` round-trips its source through `serde_json::Value`
    /// before the send, silently repairing every byte-level defect a JSON
    /// document can carry: a repeated member, member ordering, an exotic number
    /// lexeme. Those are wire behaviours the reader must refuse, so this format
    /// loads the file verbatim and the driver writes those bytes as the request
    /// body. The media type still comes from the case/step `format` axis.
    #[serde(rename = "raw-json")]
    RawJson,
}

/// HTTP method of a binding request (the ITS-REST realization layer).
///
/// The vocabulary is the method subset the released overview tabulates
/// (`ITS-REST docs/overview/Requests_and_responses.md` §HTTP Methods: GET,
/// HEAD, POST, PUT, DELETE, OPTIONS) — never a wider HTTP method set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    /// Retrieve a representation of the target resource.
    Get,
    /// Submit a representation to the target resource.
    Post,
    /// Replace the target resource with the enclosed representation.
    Put,
    /// Remove the target resource.
    Delete,
    /// Like `GET`, but the response carries headers only.
    Head,
    /// "Describe the communication options for the target resource"
    /// (`Requests_and_responses.md` §HTTP Methods) — the method the STABLE
    /// System API's one operation is served with.
    Options,
}

/// Parameter-iteration law (`CNF platform_test_schedule master04` iteration
/// semantics: "the pre-conditions and post-conditions apply to the run for X").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Iteration {
    /// The master04 law: the whole `requires` block re-established per row.
    ResetPerRow,
    /// Rows execute against one shared server state — required when an
    /// aggregate postcondition spans rows.
    SinglePass,
}

/// Spec-component keys of the `applies` version-applicability map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecComponent {
    /// The Reference Model (`RM`).
    Rm,
    /// The BASE component (foundation + base types).
    Base,
    /// The Archetype Model (`AM`).
    Am,
    /// The Archetype Query Language (`QUERY`).
    Aql,
    /// The REST implementation technology specification (`ITS-REST`).
    ItsRest,
    /// The Terminology component (`TERM`).
    Term,
}

/// AQL `RESULT_SET` comparison mode (`match:` of the `result_set` assertion).
///
/// `Set` is bag (multiset) equality — duplicate rows significant, AQL being
/// bag-semantics absent DISTINCT (QUERY `master03-syntax.adoc` §DISTINCT).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResultSetMatch {
    /// Legal only when the query totally orders the expected rows
    /// (QUERY master03 §ORDER BY: absent ORDER BY, ordering is undefined).
    Ordered,
    /// Bag equality.
    Set,
    /// Row count only.
    Count,
    /// Every expected row appears (bag-wise); extra rows permitted.
    Contains,
}

/// How a `result_set` assertion compares a scalar cell (`cells:` of the
/// `result_set` assertion).
///
/// The default is [`CellComparison::Lexeme`], the only comparison that can test
/// the write-path preservation sentence of ITS-REST
/// `specifications/docs/overview/Resources.md` §Datetime format: a body
/// date/time "will be preserved as it was sent by the client, and passed to the
/// underlying backend engine as is", stated with no RFC 2119 keyword.
/// [`CellComparison::Instant`] is declared by a case whose cells are read back
/// through a QUERY, where the same section drops to SHOULD-strength
/// ("Retrieval or querying those resources SHOULD return date, datetime, or
/// time values in the (original) format provided by underlying backend engine,
/// avoiding any format change"), so the instant is the fact the row may gate on
/// and the spelling is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CellComparison {
    /// Exact lexeme equality (the default when a case declares no `cells:`).
    #[default]
    Lexeme,
    /// ISO 8601 date/time cells carrying an explicit UTC designator or offset
    /// compare by the instant they denote; every other cell keeps exact
    /// lexeme equality.
    Instant,
}

/// The ITS a binding realizes. Closed to the REST ITS until another ITS
/// binding layer is registered by schedule release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItsName {
    /// The openEHR REST API implementation technology specification.
    #[serde(rename = "its-rest")]
    ItsRest,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_families() {
        assert_eq!(Tier::Core.family(), Family::Platform);
        assert_eq!(Tier::SecBasic.family(), Family::Security);
        assert_eq!(Tier::EnterpriseD.family(), Family::Enterprise);
    }

    /// The spec-component keys are the one spelling every `applies` map, every
    /// statement's `spec_versions` and every renderer uses.
    #[test]
    fn every_spec_component_carries_a_distinct_authored_key() {
        let tokens: Vec<&str> = SpecComponent::ALL.iter().map(|c| c.token()).collect();
        assert_eq!(tokens, ["rm", "base", "am", "aql", "its_rest", "term"]);
    }

    #[test]
    fn serde_tokens() {
        let tier: Tier = serde_json::from_value(serde_json::json!("SEC-BASIC")).unwrap();
        assert_eq!(tier, Tier::SecBasic);
        let comp: Component =
            serde_json::from_value(serde_json::json!("DEFINITION_ADL14")).unwrap();
        assert_eq!(comp, Component::DefinitionAdl14);
        let fmt: FormatName = serde_json::from_value(serde_json::json!("wt-flat")).unwrap();
        assert_eq!(fmt, FormatName::WtFlat);
    }
}

impl Component {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[Component] = &[
        Component::Ehr,
        Component::EhrComposition,
        Component::EhrContribution,
        Component::EhrDirectory,
        Component::DefinitionAdl14,
        Component::DefinitionAdl2,
        Component::DefinitionQuery,
        Component::Query,
        Component::Demographic,
        Component::Admin,
        Component::Messaging,
        Component::Content,
        Component::SimplifiedFormats,
        Component::Security,
        Component::Performance,
    ];
}

impl Family {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[Family] = &[Family::Platform, Family::Enterprise, Family::Security];
}

impl Tier {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[Tier] = &[
        Tier::Core,
        Tier::Standard,
        Tier::Options,
        Tier::SecBasic,
        Tier::EnterpriseD,
        Tier::EnterpriseM,
        Tier::EnterpriseX,
    ];
}

impl FormatName {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[FormatName] = &[
        FormatName::CanonicalJson,
        FormatName::CanonicalXml,
        FormatName::WtFlat,
        FormatName::WtStructured,
        FormatName::Wt,
    ];

    /// The token a record, a report and a rendered row all name this format
    /// by — one spelling, so two surfaces cannot drift apart.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            FormatName::CanonicalJson => "canonical-json",
            FormatName::CanonicalXml => "canonical-xml",
            FormatName::WtFlat => "wt-flat",
            FormatName::WtStructured => "wt-structured",
            FormatName::Wt => "wt",
        }
    }

    /// The media type a client names this format with in `Accept` or
    /// `Content-Type` (ITS-REST `docs/overview/Resources.md` §XML Format,
    /// §JSON Format and §Simplified Formats).
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            FormatName::CanonicalJson => "application/json",
            FormatName::CanonicalXml => "application/xml",
            FormatName::WtFlat => "application/openehr.wt.flat+json",
            FormatName::WtStructured => "application/openehr.wt.structured+json",
            FormatName::Wt => "application/openehr.wt+json",
        }
    }
}

impl CorpusFormat {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[CorpusFormat] = &[
        CorpusFormat::CanonicalJson,
        CorpusFormat::CanonicalXml,
        CorpusFormat::WtFlat,
        CorpusFormat::WtStructured,
        CorpusFormat::OptXml,
        CorpusFormat::AqlText,
        CorpusFormat::Adl2Text,
        CorpusFormat::Adl14Text,
        CorpusFormat::RawJson,
    ];

    /// The token the manifest declares this format by — one spelling, so a
    /// diagnostic names exactly what the catalogue author wrote.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            CorpusFormat::CanonicalJson => "canonical-json",
            CorpusFormat::CanonicalXml => "canonical-xml",
            CorpusFormat::WtFlat => "wt-flat",
            CorpusFormat::WtStructured => "wt-structured",
            CorpusFormat::OptXml => "opt-xml",
            CorpusFormat::AqlText => "aql-text",
            CorpusFormat::Adl2Text => "adl2-text",
            CorpusFormat::Adl14Text => "adl14-text",
            CorpusFormat::RawJson => "raw-json",
        }
    }
}

impl HttpMethod {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[HttpMethod] = &[
        HttpMethod::Get,
        HttpMethod::Post,
        HttpMethod::Put,
        HttpMethod::Delete,
        HttpMethod::Head,
        HttpMethod::Options,
    ];
}

impl Iteration {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[Iteration] = &[Iteration::ResetPerRow, Iteration::SinglePass];
}

impl SpecComponent {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[SpecComponent] = &[
        SpecComponent::Rm,
        SpecComponent::Base,
        SpecComponent::Am,
        SpecComponent::Aql,
        SpecComponent::ItsRest,
        SpecComponent::Term,
    ];

    /// The authored key of this component in an `applies` map / a statement's
    /// `spec_versions` — the one spelling every renderer and citation uses.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            SpecComponent::Rm => "rm",
            SpecComponent::Base => "base",
            SpecComponent::Am => "am",
            SpecComponent::Aql => "aql",
            SpecComponent::ItsRest => "its_rest",
            SpecComponent::Term => "term",
        }
    }
}

impl ResultSetMatch {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[ResultSetMatch] = &[
        ResultSetMatch::Ordered,
        ResultSetMatch::Set,
        ResultSetMatch::Count,
        ResultSetMatch::Contains,
    ];
}

impl CellComparison {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[CellComparison] = &[CellComparison::Lexeme, CellComparison::Instant];
}

impl ItsName {
    /// All variants, in vocabulary order (schema emission derives from this).
    pub const ALL: &[ItsName] = &[ItsName::ItsRest];
}

#[cfg(test)]
mod all_consts_tests {
    use super::*;

    /// Every `ALL` list is exhaustive: a match over the enum with no wildcard
    /// would fail to compile on a new variant, and this test pins the counts
    /// so schema emission can trust the lists.
    #[test]
    fn all_lists_are_exhaustive() {
        assert_eq!(Component::ALL.len(), 15);
        assert_eq!(Tier::ALL.len(), 7);
        assert_eq!(FormatName::ALL.len(), 5);
        assert_eq!(CorpusFormat::ALL.len(), 9);
    }
}
