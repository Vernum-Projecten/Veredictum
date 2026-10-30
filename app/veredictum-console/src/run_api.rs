// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The run wizard's server seam for S3 Connect and S4 Scope (#65).
//!
//! The wizard's memory is a server-side draft PER SUBMITTER (#389): the
//! connection facts, the credential VALUES (memory only — never persisted,
//! never rendered back, never logged), the statement pick and the filter. Two
//! visitors composing a connection at once each have their own, and what the
//! client can read back is [`DraftView`], which carries no secret by
//! construction.
//!
//! The reachability probe is the ONE console-originated request to a CDR: a
//! diagnostic rendered verbatim and never judged, so conformance traffic
//! stays the spawned instrument's alone (#54).

use serde::{Deserialize, Serialize};

/// The authentication choice, exactly the ixit's `AuthMode` vocabulary the
/// first cut supports (`bearer_mint` is deferred, #70).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthChoice {
    /// No Authorization header at all.
    None,
    /// HTTP Basic.
    Basic,
    /// A static OAuth2 bearer token.
    Bearer,
}

impl AuthChoice {
    /// The declaration token, matching the ixit's `mode`.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Basic => "basic",
            Self::Bearer => "bearer",
        }
    }
}

/// One principal's credential as the connect form sends it: the mode and
/// the secret values that mode reads.
///
/// The values travel client to server once, in the probe's request, and are
/// never sent back: `Debug` prints a redaction, and no response type carries
/// this struct.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialForm {
    /// The authentication mode.
    pub auth: AuthChoice,
    /// The Basic user, empty for the other modes.
    pub user: String,
    /// The Basic password, empty for the other modes.
    pub password: String,
    /// The bearer token, empty for the other modes.
    pub token: String,
}

impl CredentialForm {
    /// No credential at all.
    #[must_use]
    pub fn none() -> Self {
        Self {
            auth: AuthChoice::None,
            user: String::new(),
            password: String::new(),
            token: String::new(),
        }
    }
}

impl std::fmt::Debug for CredentialForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialForm")
            .field("auth", &self.auth)
            .finish_non_exhaustive()
    }
}

/// The version-signing posture the operator declares — the mode half.
///
/// The mode is a closed vocabulary (RM common
/// `master06-change_control_package.adoc` §Digital Signature: a deployment
/// signs by digest or by openPGP). `Undeclared` is a first-class answer: a
/// run whose ixit declares no posture records every `verifiable` case
/// not-applicable with the engine's own citation, which is the honest
/// outcome for a deployment fact nobody supplied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SigningChoice {
    /// The operator declares no signing posture.
    #[default]
    Undeclared,
    /// Plain digest, no public-key infrastructure.
    Digest,
    /// openPGP: a detached signature over the canonical form, verified
    /// against a declared public key.
    Pgp,
}

impl SigningChoice {
    /// Every choice, in the order the control renders.
    pub const ALL: [SigningChoice; 3] = [
        SigningChoice::Undeclared,
        SigningChoice::Digest,
        SigningChoice::Pgp,
    ];

    /// The label the control carries.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Undeclared => "not declared",
            Self::Digest => "digest",
            Self::Pgp => "openPGP",
        }
    }

    /// The id of this choice's button.
    #[must_use]
    pub fn control_id(self) -> &'static str {
        match self {
            Self::Undeclared => "signing-undeclared",
            Self::Digest => "signing-digest",
            Self::Pgp => "signing-pgp",
        }
    }
}

/// How a digest-mode signature is encoded on the wire.
///
/// Exactly the encodings the engine's own verifier implements, so a posture
/// the console composes can never name one the run cannot verify under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DigestEncoding {
    /// Standard base64 with padding.
    #[default]
    Base64,
    /// URL-safe base64 without padding.
    Base64Url,
}

impl DigestEncoding {
    /// Every encoding, in the order the control renders.
    pub const ALL: [DigestEncoding; 2] = [DigestEncoding::Base64, DigestEncoding::Base64Url];

    /// The ixit declaration token.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Base64 => "base64",
            Self::Base64Url => "base64url",
        }
    }

    /// The id of this encoding's button.
    #[must_use]
    pub fn control_id(self) -> &'static str {
        match self {
            Self::Base64 => "digest-base64",
            Self::Base64Url => "digest-base64url",
        }
    }
}

/// The openEHR generation set the deployment runs (`spec_profile`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SpecProfileChoice {
    /// The operator declares no generation set.
    #[default]
    Undeclared,
    /// The latest RELEASED openEHR generations.
    Stable,
    /// The development generations.
    Development,
}

impl SpecProfileChoice {
    /// Every choice, in the order the control renders.
    pub const ALL: [SpecProfileChoice; 3] = [
        SpecProfileChoice::Undeclared,
        SpecProfileChoice::Stable,
        SpecProfileChoice::Development,
    ];

    /// The label the control carries.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Undeclared => "not declared",
            Self::Stable => "stable",
            Self::Development => "development",
        }
    }

    /// The id of this choice's button.
    #[must_use]
    pub fn control_id(self) -> &'static str {
        match self {
            Self::Undeclared => "profile-undeclared",
            Self::Stable => "profile-stable",
            Self::Development => "profile-development",
        }
    }

    /// The declared generation set, `None` for an undeclared one.
    #[cfg(feature = "ssr")]
    #[must_use]
    pub fn declared(self) -> Option<veredictum::ixit::SpecProfile> {
        match self {
            Self::Undeclared => None,
            Self::Stable => Some(veredictum::ixit::SpecProfile::Stable),
            Self::Development => Some(veredictum::ixit::SpecProfile::Development),
        }
    }
}

/// What the browser sends for the deployment postures: one flat form, so the
/// public boundary carries no nested enum and every field is a scalar or a
/// closed vocabulary.
///
/// The form is UNTRUSTED input like every other server-fn argument. It is
/// narrowed to the typed `DeclaredPostures` at that boundary, where an
/// unusable declaration is refused by name rather than composed into an ixit
/// the run cannot use. Named in prose rather than linked: that type is
/// `ssr`-gated and this one is not, so a link breaks the featureless doc build.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PostureForm {
    /// The deployment's configured system identifier (`system_id`).
    pub system_id: String,
    /// A location on the SUT's OWN file system the admin dump/load operations
    /// may write to (`dump_location`). The console never opens it: it travels
    /// to the SUT in the request body those operations carry.
    pub dump_location: String,
    /// The signing mode.
    pub signing: SigningChoice,
    /// The digest encoding, read only for the digest mode.
    pub digest_encoding: DigestEncoding,
    /// The fixed prefix a digest-mode signature carries, read only for the
    /// digest mode. May be empty.
    pub digest_prefix: String,
    /// The armored openPGP public key, read only for the pgp mode.
    pub pgp_public_key: String,
    /// The openEHR generation set.
    pub spec_profile: SpecProfileChoice,
}

/// The signing posture a draft carries, narrowed from the flat form.
///
/// The parameters of a mode travel with that mode, so a digest posture can
/// never hold a key and a pgp posture can never hold an encoding.
#[cfg(feature = "ssr")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SigningPosture {
    /// Plain digest, no public-key infrastructure.
    Digest {
        /// How the digest is encoded on the wire.
        encoding: DigestEncoding,
        /// The fixed prefix the wire form carries before the encoded digest.
        prefix: String,
    },
    /// openPGP, verified against this armored public key.
    Pgp {
        /// The armored public key.
        public_key: String,
    },
}

/// The deployment postures a draft declares, each absent by default.
///
/// An absent fact is a FIRST-CLASS state: it composes no key in the ixit, and
/// the engine's own selection law then records the cases that need it
/// not-applicable with a citation. Nothing here ever stands in for a
/// declaration the operator did not make.
#[cfg(feature = "ssr")]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeclaredPostures {
    /// The deployment's configured system identifier.
    pub system_id: Option<String>,
    /// A location on the SUT's own file system for the admin dump/load
    /// operations. The console never opens it.
    pub dump_location: Option<String>,
    /// The version-signing posture.
    pub signing: Option<SigningPosture>,
    /// The openEHR generation set.
    pub spec_profile: Option<veredictum::ixit::SpecProfile>,
}

#[cfg(feature = "ssr")]
impl DeclaredPostures {
    /// The declared facts, one line each, for the interface to state back —
    /// empty when the operator declared none.
    ///
    /// A pgp key is summarized rather than echoed: it is public material, and
    /// a screen restating kilobytes of armor tells the reader nothing.
    #[must_use]
    pub fn summary(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(system_id) = &self.system_id {
            lines.push(format!("system_id {system_id}"));
        }
        if let Some(dump_location) = &self.dump_location {
            lines.push(format!("dump_location {dump_location}"));
        }
        match &self.signing {
            None => {}
            Some(SigningPosture::Digest { encoding, prefix }) => lines.push(format!(
                "signing digest (sha256, {}, prefix {:?})",
                encoding.token(),
                prefix
            )),
            Some(SigningPosture::Pgp { .. }) => {
                lines.push(String::from("signing openPGP (public key declared)"));
            }
        }
        if let Some(profile) = self.spec_profile {
            lines.push(format!("spec_profile {}", profile.token()));
        }
        lines
    }
}

/// A system identifier is a configured name, not a document.
#[cfg(feature = "ssr")]
const SYSTEM_ID_CAP_BYTES: u64 = 256;

/// A path on the SUT's own file system, capped well past any real one.
#[cfg(feature = "ssr")]
const DUMP_LOCATION_CAP_BYTES: u64 = 4_096;

/// A self-describing signature prefix (`sha256:`), never a payload.
#[cfg(feature = "ssr")]
const DIGEST_PREFIX_CAP_BYTES: u64 = 64;

/// An armored public key, capped far above any real one.
#[cfg(feature = "ssr")]
const PGP_KEY_CAP_BYTES: u64 = 65_536;

/// An identity label (display name or version), capped far above any real
/// product name.
#[cfg(feature = "ssr")]
pub const IDENTITY_LABEL_CAP_BYTES: u64 = 1_024;

/// Every input cap a `#[server]` function in this crate enforces.
///
/// The router sizes ONE transport cap for all of them, from the largest, so
/// an endpoint that starts judging a larger input belongs in this list and
/// wants that cap re-derived with it.
#[cfg(feature = "ssr")]
pub const SERVER_FN_INPUT_CAPS: &[u64] = &[
    read::STATEMENT_CAP_BYTES,
    SYSTEM_ID_CAP_BYTES,
    DUMP_LOCATION_CAP_BYTES,
    DIGEST_PREFIX_CAP_BYTES,
    PGP_KEY_CAP_BYTES,
    IDENTITY_LABEL_CAP_BYTES,
];

/// The postures the flat form declares, or the refusal naming the one field
/// that cannot be used.
///
/// Every value is UNTRUSTED input on a public endpoint, so each is capped and
/// each unusable declaration is an error rather than a silently dropped
/// field: a posture the operator believes they declared, and the run then
/// judged without, is the defect this whole seam exists to close.
///
/// # Errors
/// A value past its cap, a pgp mode with no armored key, and a digest prefix
/// past its cap — each naming the field.
#[cfg(feature = "ssr")]
pub fn declared_postures(form: &PostureForm) -> Result<DeclaredPostures, String> {
    /// What the engine's own openPGP reader needs to see first.
    const PGP_ARMOR_HEADER: &str = "-----BEGIN PGP PUBLIC KEY BLOCK-----";

    fn capped(field: &str, value: &str, cap: u64) -> Result<Option<String>, String> {
        let value = value.trim();
        if u64::try_from(value.len()).unwrap_or(u64::MAX) > cap {
            return Err(format!(
                "{field} is {} bytes; the cap is {cap}",
                value.len()
            ));
        }
        Ok((!value.is_empty()).then(|| value.to_owned()))
    }

    let signing = match form.signing {
        SigningChoice::Undeclared => None,
        SigningChoice::Digest => Some(SigningPosture::Digest {
            encoding: form.digest_encoding,
            prefix: capped(
                "the digest prefix",
                &form.digest_prefix,
                DIGEST_PREFIX_CAP_BYTES,
            )?
            .unwrap_or_default(),
        }),
        SigningChoice::Pgp => {
            let key = capped(
                "the openPGP public key",
                &form.pgp_public_key,
                PGP_KEY_CAP_BYTES,
            )?
            .ok_or_else(|| {
                String::from(
                    "the openPGP signing posture declares no public key: paste the \
                     armored key the deployment signs with, or leave the posture undeclared",
                )
            })?;
            if !key.contains(PGP_ARMOR_HEADER) {
                return Err(format!(
                    "the openPGP public key carries no {PGP_ARMOR_HEADER} line, so the run's \
                     verifier cannot read it"
                ));
            }
            Some(SigningPosture::Pgp { public_key: key })
        }
    };
    Ok(DeclaredPostures {
        system_id: capped("the system id", &form.system_id, SYSTEM_ID_CAP_BYTES)?,
        dump_location: capped(
            "the dump location",
            &form.dump_location,
            DUMP_LOCATION_CAP_BYTES,
        )?,
        signing,
        spec_profile: form.spec_profile.declared(),
    })
}

/// The server-side draft (ssr only; never serialized to the client).
#[cfg(feature = "ssr")]
#[derive(Debug)]
pub struct RunDraft {
    /// The CDR base URL.
    pub base_url: String,
    /// The display name the operator entered, which the record carries only
    /// when the server does not state its own.
    pub sut_name: String,
    /// The version label the operator entered, which the record carries only
    /// when the server does not state its own.
    pub sut_version: String,
    /// What the server answered to the identity read, `None` when it was
    /// never read.
    pub identity: Option<crate::identity::IdentityAnswer>,
    /// The authentication mode.
    pub auth: AuthChoice,
    /// How the administrative principal logs in, which the operator
    /// supplies separately; [`AuthChoice::None`] when they supplied none.
    pub admin_auth: AuthChoice,
    /// The credential values, redacted from every rendering: the server
    /// credential under the `--auth` variable names, the administrative one
    /// under the `--admin-auth` names.
    pub credentials: Vec<crate::engine::Credential>,
    /// Whether the probe answered 2xx for these facts.
    pub probed_ok: bool,
    /// The pasted statement DOCUMENT (the vendor's own claim), when any —
    /// validated content, written into the job's output directory at start.
    pub statement_json: Option<String>,
    /// The claim's product identity, parsed once at save time.
    pub statement_product: Option<String>,
    /// The document-id filter, when any.
    pub filter: Option<String>,
    /// The deployment postures the operator declared, each absent until they
    /// declare it (#456).
    pub postures: DeclaredPostures,
    /// The specification profile the run is judged under: the development
    /// snapshot until the operator switches to the frozen release.
    pub profile: veredictum::arazzo::spec_profile::SpecProfile,
}

#[cfg(feature = "ssr")]
impl RunDraft {
    /// The identity a run over this draft records, and the operator values
    /// it passes on.
    #[must_use]
    pub fn resolved(&self) -> crate::identity::Resolved {
        crate::identity::resolve(self.identity.as_ref(), &self.sut_name, &self.sut_version)
    }

    /// The identity a run over this draft records, each field with who
    /// supplied it; a field neither side supplied is the operator's blank,
    /// which a certificate refuses. The REST release is left absent: the run's
    /// own results record it under `server`, and a certificate reads it there.
    #[must_use]
    pub fn recorded_sut(&self) -> veredictum::party::Sut {
        use veredictum::party::IdentityField;
        let resolved = self.resolved();
        let field = |value: Option<String>, operator: Option<String>| match (value, operator) {
            (Some(value), None) => IdentityField::server(value),
            (_, Some(operator)) => IdentityField::operator(operator),
            (None, None) => IdentityField::operator(String::new()),
        };
        let stated = self.identity.as_ref();
        veredictum::party::Sut {
            name: field(resolved.name, resolved.operator_name),
            version: field(resolved.version, resolved.operator_version),
            vendor: stated
                .and_then(crate::identity::IdentityAnswer::vendor)
                .map(IdentityField::server),
            restapi_specs_version: None,
        }
    }
}

/// Every visitor's connection draft, one per submitter.
///
/// Bounded and evicted oldest-first exactly as the job map is
/// (`run_job::DRAFTS_KEPT`): these entries hold credential VALUES, so an
/// unbounded map of them is not an option. A draft lives in memory and in the
/// spawned run's environment, and reaches no file, no log line and no
/// client-visible state.
#[cfg(feature = "ssr")]
#[derive(Debug, Default)]
pub struct Drafts {
    entries: std::collections::BTreeMap<crate::submitter::Submitter, (u64, RunDraft)>,
    next: u64,
}

#[cfg(feature = "ssr")]
impl Drafts {
    /// An empty set of drafts.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// This submitter's draft, when they have one.
    #[must_use]
    pub fn get(&self, submitter: crate::submitter::Submitter) -> Option<&RunDraft> {
        self.entries.get(&submitter).map(|(_, draft)| draft)
    }

    /// This submitter's draft, to be edited in place.
    pub fn get_mut(&mut self, submitter: crate::submitter::Submitter) -> Option<&mut RunDraft> {
        self.entries.get_mut(&submitter).map(|(_, draft)| draft)
    }

    /// Stores this submitter's draft, replacing any prior one, and evicts the
    /// oldest drafts past the cap.
    pub fn insert(&mut self, submitter: crate::submitter::Submitter, draft: RunDraft) {
        let seq = self.next;
        self.next = self.next.saturating_add(1);
        self.entries.insert(submitter, (seq, draft));
        let excess = self
            .entries
            .len()
            .saturating_sub(crate::run_job::DRAFTS_KEPT);
        if excess == 0 {
            return;
        }
        let mut ages: Vec<(u64, crate::submitter::Submitter)> = self
            .entries
            .iter()
            .map(|(who, (seq, _))| (*seq, *who))
            .collect();
        ages.sort_unstable();
        for (_, who) in ages.into_iter().take(excess) {
            self.entries.remove(&who);
        }
    }
}

/// What the client may read back of the draft: no secret, by construction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftView {
    /// The CDR base URL.
    pub base_url: String,
    /// The SUT display name the record will carry, the server's when it
    /// stated one; empty when neither side supplied it.
    pub sut_name: String,
    /// The SUT version label the record will carry, the server's when it
    /// stated one; empty when neither side supplied it.
    pub sut_version: String,
    /// The authentication mode token.
    pub auth: String,
    /// The administrative principal's authentication mode token, `none`
    /// when the operator supplied no administrative credential.
    pub admin_auth: String,
    /// Whether the probe answered 2xx.
    pub probed_ok: bool,
    /// The saved claim's product identity, when a statement was pasted.
    pub statement: Option<String>,
    /// The document-id filter, when any.
    pub filter: Option<String>,
    /// The deployment postures the draft declares, one line each — empty when
    /// the operator declared none.
    pub postures: Vec<String>,
    /// The specification profile token the run is judged under.
    pub profile: String,
}

/// The accepted claim, summarized for the overview the operator reads
/// before starting: who claims, and how many capability folders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimSummary {
    /// The product identity the statement declares.
    pub product: String,
    /// The number of claimed capability folders.
    pub capabilities: u64,
}

/// One capability folder the Scope screen can build a claim from, and how
/// much of the catalogue claiming it reaches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRow {
    /// The folder name, the token `claims.capabilities` names it by.
    pub name: String,
    /// The Arazzo documents in the folder.
    pub documents: u64,
    /// The workflows those documents carry.
    pub cases: u64,
}

/// The probe's verbatim answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProbeAnswer {
    /// The server answered; the fields are its own words.
    Answered {
        /// The status line, verbatim.
        status: String,
        /// Round-trip time in milliseconds.
        elapsed_ms: u64,
        /// Whether the status was 2xx (what seed-gates Continue).
        ok: bool,
    },
    /// The connection itself failed; the field is the transport's own words.
    Unreachable {
        /// The error, verbatim.
        error: String,
    },
}

/// What the connect step's probe answered: the reachability probe and the
/// engine's identity read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectAnswer {
    /// The reachability probe's verbatim answer.
    pub probe: ProbeAnswer,
    /// What the server stated about itself through System OPTIONS.
    pub identity: crate::identity::IdentityAnswer,
}

/// The honest scope preview: what a run over this scope will PROCESS.
///
/// Every workflow of a document in filter scope lands as a row, and the
/// statement decides how many are skipped at drive time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopePreview {
    /// Workflows in scope (the count a run processes).
    pub total: u64,
    /// The per-capability breakdown, folder-sorted.
    pub chapters: Vec<(String, u64)>,
}

/// What the live screen is looking at (#386).
///
/// Four honest answers, so a reader can tell "this run is executing here"
/// from "this instance never heard of that run". A console instance answers
/// only about itself: it drives a run in its own memory, or it reads the
/// run's own directory under the mounted output tree, or it knows nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunScreen {
    /// This process is driving the run: stream it.
    Live(Box<crate::run_job::JobView>),
    /// The run is not in this process's memory, and its artifacts answer
    /// instead.
    Recorded(Box<RecordedRun>),
    /// A run was named and this instance knows nothing of it.
    Unknown(crate::run_job::RunId),
    /// No run was named, and nothing is in flight here.
    NoRunNamed,
}

/// What a start request answered (#389).
///
/// The per-submitter refusal travels as an ANSWER rather than as a transport
/// error, because it carries the run the visitor already has and the screen's
/// whole job is to link them to it. The typed refusal at the boundary that
/// branches is `run_job::JobError::Busy`; this is how it reaches a browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartOutcome {
    /// The run was accepted under this id, driving or queued. Its own live
    /// screen states which.
    Accepted(crate::run_job::RunId),
    /// This submitter already has a run in flight, so nothing was started;
    /// the field is the run they already have.
    AlreadyInFlight(crate::run_job::RunId),
}

/// A run read back from its own directory, never from this process's memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedRun {
    /// The run this directory belongs to.
    pub id: crate::run_job::RunId,
    /// The artifacts directory, as the screen shows it.
    pub dir: String,
    /// What the results document says; `None` when the directory holds none.
    pub results: Option<RecordedResults>,
}

/// The tally a recorded run's results document carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedResults {
    /// The base URL the results document says the run drove.
    pub base_url: String,
    /// Passed rows.
    pub passed: u64,
    /// Failed rows.
    pub failed: u64,
    /// Errored rows.
    pub errored: u64,
    /// Skipped rows.
    pub skipped: u64,
    /// Where the results document sits.
    pub results_path: String,
}

#[cfg(feature = "ssr")]
pub mod read {
    //! The component-free ssr readers and writers behind the endpoints.

    use super::{
        CapabilityRow, ClaimSummary, DraftView, RecordedResults, RecordedRun, RunDraft, RunScreen,
        ScopePreview, StartOutcome,
    };
    use crate::run_job::{Latest, RunId};
    use crate::state::ConsoleState;
    use crate::submitter::Submitter;

    /// The pasted-claim size cap: far above any real ICS, far below abuse.
    ///
    /// The router sizes the server functions' transport cap from this number,
    /// so a paste past it reads this module's own sentence.
    pub const STATEMENT_CAP_BYTES: u64 = 1_048_576;

    /// The refusal every step past Connect gives when no draft exists.
    const NO_DRAFT: &str = "no connection draft: complete the Connect step first";

    /// NOTE: no openEHR spec governs this — our own design; usize → u64 is
    /// lossless on every supported target (see `catalogue_api::read::count`).
    fn count(n: usize) -> u64 {
        u64::try_from(n).unwrap_or(u64::MAX)
    }

    /// The client-safe view of this submitter's draft.
    #[must_use]
    pub fn draft_view(state: &ConsoleState, submitter: Submitter) -> Option<DraftView> {
        let guard = state.draft.lock().ok()?;
        guard.get(submitter).map(|draft| {
            let resolved = draft.resolved();
            DraftView {
                base_url: draft.base_url.clone(),
                sut_name: resolved.name.unwrap_or_default(),
                sut_version: resolved.version.unwrap_or_default(),
                auth: draft.auth.token().to_owned(),
                admin_auth: draft.admin_auth.token().to_owned(),
                probed_ok: draft.probed_ok,
                statement: draft.statement_product.clone(),
                filter: draft.filter.clone(),
                postures: draft.postures.summary(),
                profile: draft.profile.token().to_owned(),
            }
        })
    }

    /// Stores the specification profile the run is judged under, from its
    /// token.
    ///
    /// # Errors
    /// The engine's refusal of a token that names no profile, "no connection
    /// draft" when S3 has not run, or the poisoned-lock diagnostic.
    pub fn save_profile(
        state: &ConsoleState,
        submitter: Submitter,
        token: &str,
    ) -> Result<(), String> {
        let profile: veredictum::arazzo::spec_profile::SpecProfile = token
            .parse()
            .map_err(|e: veredictum::arazzo::spec_profile::UnknownProfile| e.to_string())?;
        let mut guard = state.draft.lock().map_err(|e| e.to_string())?;
        let draft = guard
            .get_mut(submitter)
            .ok_or_else(|| String::from(NO_DRAFT))?;
        draft.profile = profile;
        Ok(())
    }

    /// Stores this submitter's connection draft, replacing any prior one.
    ///
    /// Another visitor's draft is untouched: the map is keyed by submitter,
    /// so two people composing a connection at once do not overwrite each
    /// other.
    ///
    /// # Errors
    /// The poisoned-lock diagnostic, verbatim.
    pub fn save_connection(
        state: &ConsoleState,
        submitter: Submitter,
        draft: RunDraft,
    ) -> Result<(), String> {
        let mut guard = state.draft.lock().map_err(|e| e.to_string())?;
        guard.insert(submitter, draft);
        Ok(())
    }

    /// Validates and stores the scope half onto the existing draft.
    ///
    /// The pasted claim is UNTRUSTED input on a public endpoint: size-capped,
    /// parsed through the published lib's own statement type, and refused
    /// with the reader's finding verbatim — never stored unvalidated.
    ///
    /// # Errors
    /// "no connection draft" when S3 has not run, the size cap, the
    /// statement reader's finding verbatim, or the poisoned-lock diagnostic.
    #[expect(
        clippy::disallowed_types,
        reason = "the artifact-loader family: schema validation runs over the raw JSON value before the typed parse, exactly as the engine's own loaders do"
    )]
    pub fn save_scope(
        state: &ConsoleState,
        submitter: Submitter,
        statement_json: Option<String>,
        filter: Option<String>,
        postures: &super::PostureForm,
    ) -> Result<Option<ClaimSummary>, String> {
        let summary = statement_json
            .as_deref()
            .map(|body| {
                if count(body.len()) > STATEMENT_CAP_BYTES {
                    return Err(format!(
                        "the statement is {} bytes; the cap is {STATEMENT_CAP_BYTES}",
                        body.len()
                    ));
                }
                let value: serde_json::Value = serde_json::from_str(body)
                    .map_err(|e| format!("the statement is not JSON: {e}"))?;
                // Held to the PUBLISHED statement schema — the same document
                // the engine emits and loads by — before the typed parse.
                let validator = jsonschema::validator_for(&veredictum::schema::statement_schema())
                    .map_err(|e| format!("the statement schema itself failed to compile: {e}"))?;
                if let Some(finding) = validator.iter_errors(&value).next() {
                    return Err(format!(
                        "the statement fails its published schema at {}: {finding}",
                        finding.instance_path()
                    ));
                }
                let statement: veredictum::party::Statement = serde_json::from_value(value)
                    .map_err(|e| format!("the statement does not parse: {e}"))?;
                Ok(ClaimSummary {
                    product: format!("{} {}", statement.product.name, statement.product.version),
                    capabilities: count(statement.claims.capabilities.len()),
                })
            })
            .transpose()?;
        // Narrowed BEFORE the draft is touched, so a refused declaration
        // leaves the stored scope exactly as it was.
        let declared = super::declared_postures(postures)?;
        let mut guard = state.draft.lock().map_err(|e| e.to_string())?;
        let draft = guard
            .get_mut(submitter)
            .ok_or_else(|| String::from(NO_DRAFT))?;
        draft.statement_product = summary.as_ref().map(|claim| claim.product.clone());
        draft.statement_json = statement_json;
        draft.filter = filter;
        draft.postures = declared;
        Ok(summary)
    }

    /// The documents a document-id filter selects, in catalogue order; every
    /// document when the filter is empty.
    fn filtered<'l>(
        listing: &'l veredictum::pipeline::listing::Listing,
        filter: &str,
    ) -> impl Iterator<Item = &'l veredictum::pipeline::listing::ListedDocument> {
        listing
            .documents
            .iter()
            .filter(move |document| filter.is_empty() || document.id.contains(filter))
    }

    /// The document ids a run over this filter names with `--document`;
    /// empty, which runs every document, when the filter is empty.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent, and a filter
    /// no document id contains, which would otherwise run nothing.
    pub fn filtered_documents(state: &ConsoleState, filter: &str) -> Result<Vec<String>, String> {
        if filter.is_empty() {
            return Ok(Vec::new());
        }
        let listing = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
        let documents: Vec<String> = filtered(listing, filter)
            .map(|document| document.id.clone())
            .collect();
        if documents.is_empty() {
            return Err(format!(
                "no document id contains `{filter}`, so the run would drive nothing"
            ));
        }
        Ok(documents)
    }

    /// The scope preview over the loaded catalogue.
    ///
    /// The filter selects whole documents, exactly the set a run is handed
    /// with `--document`, so the count is honest without re-implementing the
    /// drive-time selection.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent.
    pub fn scope_preview(state: &ConsoleState, filter: &str) -> Result<ScopePreview, String> {
        let listing = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
        let mut chapters: std::collections::BTreeMap<String, u64> =
            std::collections::BTreeMap::new();
        let mut total = 0_usize;
        for document in filtered(listing, filter) {
            total += document.workflows.len();
            *chapters.entry(document.capability.clone()).or_insert(0) +=
                count(document.workflows.len());
        }
        Ok(ScopePreview {
            total: count(total),
            chapters: chapters.into_iter().collect(),
        })
    }

    /// The capability rows: each folder with its documents and workflows.
    ///
    /// # Errors
    /// The catalogue's verbatim load failure.
    pub fn capability_rows(state: &ConsoleState) -> Result<Vec<CapabilityRow>, String> {
        let listing = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
        Ok(listing
            .capabilities()
            .into_iter()
            .map(|(name, documents)| CapabilityRow {
                name: name.to_owned(),
                documents: count(documents.len()),
                cases: count(documents.iter().map(|d| d.workflows.len()).sum()),
            })
            .collect())
    }

    /// The composed claim's product identifier, derived from the identity the
    /// Connect step collected.
    fn identifier_of(name: &str, version: &str) -> String {
        let slug = |text: &str| {
            text.chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() {
                        c.to_ascii_lowercase()
                    } else {
                        '-'
                    }
                })
                .collect::<String>()
        };
        format!("urn:veredictum:console:{}:{}", slug(name), slug(version))
    }

    /// Composes the ad-hoc claim for a capability selection.
    ///
    /// The product identity is the connection draft's own SUT name and
    /// version; the claimed capabilities are the checked folders, in
    /// catalogue order. Nothing is stored: the answer is a document the
    /// operator saves like a pasted one.
    ///
    /// # Errors
    /// An empty selection, a name that is not a capability folder of the
    /// catalogue, a missing connection draft or product identity, and the
    /// catalogue's load failure, each verbatim.
    pub fn compose_claim(
        state: &ConsoleState,
        submitter: Submitter,
        selected: &[String],
    ) -> Result<String, String> {
        if selected.is_empty() {
            return Err(String::from(
                "check at least one capability: a claim with no capability claims nothing",
            ));
        }
        let listing = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
        let folders = listing.capabilities();
        if let Some(unknown) = selected
            .iter()
            .find(|name| !folders.contains_key(name.as_str()))
        {
            return Err(format!(
                "`{unknown}` is not a capability folder of the catalogue"
            ));
        }
        let (resolved, vendor) = {
            let guard = state.draft.lock().map_err(|e| e.to_string())?;
            let draft = guard.get(submitter).ok_or_else(|| String::from(NO_DRAFT))?;
            let vendor = draft
                .identity
                .as_ref()
                .and_then(crate::identity::IdentityAnswer::vendor)
                .map(str::to_owned);
            (draft.resolved(), vendor)
        };
        let (Some(name), Some(version)) = (resolved.name, resolved.version) else {
            return Err(String::from(
                "the composed claim's product identity is the server's stated name and version, or the Connect step's entries where it stated none: fill both in first",
            ));
        };
        // Normalized against the catalogue itself, so a repeated or reordered
        // list from a public endpoint composes the same claim.
        let capabilities: Vec<veredictum::ids::CapabilityName> = folders
            .keys()
            .filter(|name| selected.iter().any(|held| held == *name))
            .map(|name| veredictum::ids::CapabilityName::parse(name))
            .collect::<Result<_, _>>()
            .map_err(|e| format!("a capability folder is not a claimable name: {e}"))?;
        let statement = veredictum::party::Statement {
            product: veredictum::party::Product {
                identifier: identifier_of(&name, &version),
                name,
                version,
                vendor: vendor.unwrap_or_else(|| String::from("unknown")),
            },
            schedule_release: String::from(veredictum::party::SCHEDULE_RELEASE),
            spec_versions: veredictum::party::SpecVersions::default(),
            claims: veredictum::party::Claims {
                capabilities,
                profiles: Vec::new(),
            },
            tech_profiles: Vec::new(),
            options: Vec::new(),
            performance: None,
            non_functional: std::collections::BTreeMap::new(),
            evidence: Vec::new(),
            attestation: None,
        };
        let mut document = serde_json::to_string_pretty(&statement)
            .map_err(|e| format!("the composed claim did not serialize: {e}"))?;
        document.push('\n');
        Ok(document)
    }

    /// One instance of the ixit document: where it is, and how the run
    /// authenticates to it.
    #[derive(Debug, Clone, Copy, serde::Serialize)]
    struct IxitInstance<'d> {
        /// The CDR base URL.
        base_url: &'d str,
        /// The authentication mode, carrying env-var NAMES only.
        auth: IxitAuth,
    }

    /// The authentication modes the console composes — env-var NAMES only, so
    /// a credential VALUE cannot reach a file by construction.
    #[derive(Debug, Clone, Copy, serde::Serialize)]
    #[serde(tag = "mode", rename_all = "snake_case")]
    enum IxitAuth {
        /// No Authorization header at all.
        None,
        /// HTTP Basic, from the named variables.
        Basic {
            /// The variable carrying the user.
            user_env: &'static str,
            /// The variable carrying the password.
            password_env: &'static str,
        },
        /// A static bearer token, from the named variable.
        Bearer {
            /// The variable carrying the token.
            token_env: &'static str,
        },
    }

    /// The version-signing posture as the ixit spells it (RM common
    /// `master06-change_control_package.adoc` §Digital Signature).
    #[derive(Debug, Clone, Copy, serde::Serialize)]
    #[serde(tag = "mode", rename_all = "snake_case")]
    enum IxitSigning<'d> {
        /// Plain digest: the wire form is `<prefix><encoding(hash(bytes))>`.
        Digest {
            /// The hash algorithm applied to the canonical bytes.
            algorithm: &'static str,
            /// How the digest is encoded on the wire.
            encoding: &'static str,
            /// The fixed prefix the wire form carries.
            prefix: &'d str,
        },
        /// openPGP, verified against the declared armored public key.
        Pgp {
            /// The armored public key.
            public_key: &'d str,
        },
    }

    /// The ixit document the console composes for a run: the instances it can
    /// reach, plus exactly the deployment postures the operator declared.
    ///
    /// Every posture is skipped when absent, because an absent key is what
    /// makes the engine record a case not-applicable with its own citation. A
    /// present key with a stand-in value would instead claim something about
    /// somebody else's deployment.
    #[derive(Debug, Clone, serde::Serialize)]
    struct IxitDocument<'d> {
        /// The named instances the flow addresses.
        instances: std::collections::BTreeMap<&'static str, IxitInstance<'d>>,
        /// The deployment's configured system identifier.
        #[serde(skip_serializing_if = "Option::is_none")]
        system_id: Option<&'d str>,
        /// A location on the SUT's own file system for the dump/load
        /// operations.
        #[serde(skip_serializing_if = "Option::is_none")]
        dump_location: Option<&'d str>,
        /// The openEHR generation set.
        #[serde(skip_serializing_if = "Option::is_none")]
        spec_profile: Option<&'static str>,
        /// The version-signing posture.
        #[serde(skip_serializing_if = "Option::is_none")]
        signing: Option<IxitSigning<'d>>,
    }

    /// The ixit auth block for one principal: `choice` under the variable
    /// names that principal's role reads.
    fn ixit_auth(
        choice: super::AuthChoice,
        role: veredictum::arazzo::credential::Role,
    ) -> IxitAuth {
        let (user_env, password_env, token_env) = role.variables();
        match choice {
            super::AuthChoice::None => IxitAuth::None,
            super::AuthChoice::Basic => IxitAuth::Basic {
                user_env,
                password_env,
            },
            super::AuthChoice::Bearer => IxitAuth::Bearer { token_env },
        }
    }

    /// Renders the draft's ixit document.
    ///
    /// The instances point at the CDR, each carrying env-var NAMES only: the
    /// values live in the draft and reach the spawned run's environment
    /// alone. The `admin` instance is composed only when the operator
    /// supplied an administrative credential, and it carries that
    /// credential's own variable names, so one credential set is never
    /// presented as two principals. Beside them travel exactly the deployment postures
    /// the operator declared (#456) — a fact nobody declared composes no key,
    /// so the engine's selection law records the cases needing it
    /// not-applicable with its own citation instead of being driven under a
    /// guess.
    ///
    /// # Errors
    /// The serializer's verbatim failure, and the engine reader's refusal of a
    /// document the instrument cannot read.
    pub fn ixit_document(draft: &RunDraft) -> Result<String, String> {
        use veredictum::arazzo::credential::Role;

        let instance = |auth: IxitAuth| IxitInstance {
            base_url: &draft.base_url,
            auth,
        };
        let mut instances = std::collections::BTreeMap::new();
        instances.insert("sut", instance(ixit_auth(draft.auth, Role::Ordinary)));
        if draft.admin_auth != super::AuthChoice::None {
            instances.insert(
                "admin",
                instance(ixit_auth(draft.admin_auth, Role::Administrative)),
            );
        }
        instances.insert("unauthenticated", instance(IxitAuth::None));
        let signing = draft
            .postures
            .signing
            .as_ref()
            .map(|posture| match posture {
                // NOTE: RM common master06 §Digital Signature leaves the hash
                // to the deployment; sha256 is the one the engine's verifier
                // implements, so it is the one this console can declare.
                super::SigningPosture::Digest { encoding, prefix } => IxitSigning::Digest {
                    algorithm: "sha256",
                    encoding: encoding.token(),
                    prefix,
                },
                super::SigningPosture::Pgp { public_key } => IxitSigning::Pgp { public_key },
            });
        let document = IxitDocument {
            instances,
            system_id: draft.postures.system_id.as_deref(),
            dump_location: draft.postures.dump_location.as_deref(),
            spec_profile: draft
                .postures
                .spec_profile
                .map(veredictum::ixit::SpecProfile::token),
            signing,
        };
        let mut rendered = serde_json::to_string_pretty(&document)
            .map_err(|e| format!("the composed ixit did not serialize: {e}"))?;
        rendered.push('\n');
        // The composed bytes are held to the engine's OWN reader before a run
        // is built on them: a document the instrument cannot read is a console
        // defect, and it is refused here rather than at the spawned engine's
        // first line.
        serde_json::from_str::<veredictum::ixit::Ixit>(&rendered)
            .map_err(|e| format!("the composed ixit does not parse: {e}"))?;
        Ok(rendered)
    }

    /// Starts this submitter's drafted run.
    ///
    /// The public start seam: it spends this submitter's start budget and
    /// puts the drafted target through the posture's guard BEFORE the engine
    /// is located or spawned, then hands the drafted spec to
    /// [`start_run_with`].
    ///
    /// # Errors
    /// "no connection draft" before S3, the rate ledger's refusal with its
    /// retry, the target guard's refusal naming the address family, the
    /// engine's own locate/spawn refusals, and the filesystem's verbatim
    /// failures. The per-submitter concurrency refusal is an ANSWER
    /// ([`StartOutcome::AlreadyInFlight`]), not an error, because it names
    /// the run the screen must link to.
    pub async fn start_run(
        state: &ConsoleState,
        submitter: Submitter,
    ) -> Result<StartOutcome, String> {
        // The no-draft refusal precedes engine discovery, so an unconnected
        // wizard is refused for what it is on a host with no engine mounted.
        // The guard's lock is released before the awaits below: nothing here
        // holds it across one.
        let drafted = state
            .draft
            .lock()
            .map_err(|e| e.to_string())?
            .get(submitter)
            .map(|draft| draft.base_url.clone());
        let Some(base_url) = drafted else {
            return Err(String::from(NO_DRAFT));
        };
        state
            .rates
            .admit(submitter, crate::rate_limit::Metered::Start)
            .map_err(|e| e.to_string())?;
        // The target guard (#390), before the engine is spawned: a hosted
        // instance refuses an address only it can reach, resolving the name
        // first. The local posture refuses nothing.
        crate::target_safety::check(state.posture, &base_url)
            .await
            .map_err(|e| e.to_string())?;
        let engine = crate::engine::locate().map_err(|e| e.to_string())?;
        start_run_with(state, submitter, &engine)
    }

    /// Starts the drafted run through an already-located engine.
    ///
    /// Writes the ixit under the job's own output directory and hands the
    /// spec to the job slot.
    /// The credentials MOVE out of the draft into the spawned run's
    /// environment: they reach the child process and nothing else — no file,
    /// no log line, no client-visible state.
    ///
    /// The split exists for the gate: [`start_run`] finds the pinned binary the way the server does, and a
    /// test injects the one it verified itself rather than reaching for
    /// `PATH`.
    ///
    /// # Errors
    /// "no connection draft" before S3, a document filter that selects
    /// nothing, the engine's own spawn refusals, and the filesystem's verbatim
    /// failures. A submitter who already has a run
    /// in flight gets [`StartOutcome::AlreadyInFlight`], which is an answer
    /// naming that run rather than an error.
    pub fn start_run_with(
        state: &ConsoleState,
        submitter: Submitter,
        engine: &crate::engine::Engine,
    ) -> Result<StartOutcome, String> {
        // The pre-flight, so nothing is written for a run that will be
        // refused. The AUTHORITATIVE check is inside the job map's own lock;
        // if a second start from this submitter wins the race in between, its
        // answer still names their run and the cost is a re-probe.
        if let Some(existing) = state
            .jobs
            .in_flight_of(submitter)
            .map_err(|e| e.to_string())?
        {
            return Ok(StartOutcome::AlreadyInFlight(existing));
        }
        let mut guard = state.draft.lock().map_err(|e| e.to_string())?;
        let draft = guard
            .get_mut(submitter)
            .ok_or_else(|| String::from(NO_DRAFT))?;
        let documents = filtered_documents(state, draft.filter.as_deref().unwrap_or_default())?;
        let id = state.jobs.allocate_id();
        let out_dir = crate::run_job::job_dir(&state.out, id);
        std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
        let ixit_path = out_dir.join("ixit.json");
        std::fs::write(&ixit_path, ixit_document(draft)?)
            .map_err(|e| format!("{}: {e}", ixit_path.display()))?;
        // The claim travels WITH the run: the job directory carries the exact
        // bytes the engine selected by, never the mutable draft.
        let statement_path = draft
            .statement_json
            .as_deref()
            .map(|body| {
                let path = out_dir.join("statement.json");
                std::fs::write(&path, body).map_err(|e| format!("{}: {e}", path.display()))?;
                Ok::<_, String>(path)
            })
            .transpose()?;
        let spec = crate::engine::RunSpec {
            profile: draft.profile,
            root: state.root.clone(),
            base_url: draft.base_url.clone(),
            auth: draft.auth.token(),
            admin_auth: draft.admin_auth.token(),
            statement: statement_path,
            ixit: Some(ixit_path),
            documents,
            out_dir,
            credentials: std::mem::take(&mut draft.credentials),
        };
        let sut = draft.recorded_sut();
        drop(guard);
        match state.jobs.start(id, submitter, engine, spec, sut) {
            Ok(accepted) => Ok(StartOutcome::Accepted(accepted)),
            Err(crate::run_job::JobError::Busy(existing)) => {
                Ok(StartOutcome::AlreadyInFlight(existing))
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// What the live screen is looking at, for the run the URL names.
    ///
    /// A named run this process is driving streams; anything else falls back
    /// to the run's own directory under the mounted output tree, which is the
    /// durable half. `None` — the bare `/run/live` — asks about the run THIS
    /// SUBMITTER most recently started, never about somebody else's, and with
    /// none the answer is [`RunScreen::NoRunNamed`].
    ///
    /// The id is a parsed UUID, so the derived directory stays under the
    /// mounted output root by construction. A run this process never drove is
    /// still readable by id: the artifacts are the durable half, and nothing
    /// here is gated on who started it.
    ///
    /// # Errors
    /// The map's verbatim refusal, and the verbatim read or parse failure of
    /// a results document that exists but cannot be read.
    pub fn run_screen(
        state: &ConsoleState,
        submitter: Submitter,
        id: Option<RunId>,
    ) -> Result<RunScreen, String> {
        let named = match id {
            Some(named) => named,
            None => match state
                .jobs
                .latest_of(submitter, Latest::Any)
                .map_err(|e| e.to_string())?
            {
                Some(current) => current,
                None => return Ok(RunScreen::NoRunNamed),
            },
        };
        if let Some(view) = state.jobs.view_of(named).map_err(|e| e.to_string())? {
            return Ok(RunScreen::Live(Box::new(view)));
        }
        recorded_run(state, named)
    }

    /// The named run as its own directory describes it.
    ///
    /// The results document is parsed through the published lib, so the tally
    /// is the same one a live finished view carries.
    ///
    /// # Errors
    /// The verbatim read or parse failure of a results document that exists.
    fn recorded_run(state: &ConsoleState, id: RunId) -> Result<RunScreen, String> {
        let dir = crate::run_job::job_dir(&state.out, id);
        if !dir.is_dir() {
            return Ok(RunScreen::Unknown(id));
        }
        let results_path = dir.join(veredictum::pipeline::arazzo_run::RESULTS_FILE);
        if !results_path.is_file() {
            return Ok(RunScreen::Recorded(Box::new(RecordedRun {
                id,
                dir: dir.display().to_string(),
                results: None,
            })));
        }
        let body = std::fs::read_to_string(&results_path)
            .map_err(|e| format!("{}: {e}", results_path.display()))?;
        let results: veredictum::arazzo::results::Results =
            serde_json::from_str(&body).map_err(|e| format!("{}: {e}", results_path.display()))?;
        let (passed, failed, errored, skipped) = crate::run_job::tally(&results);
        Ok(RunScreen::Recorded(Box::new(RecordedRun {
            id,
            dir: dir.display().to_string(),
            results: Some(RecordedResults {
                base_url: results.base_url.clone(),
                passed,
                failed,
                errored,
                skipped,
                results_path: results_path.display().to_string(),
            }),
        })))
    }

    /// The credential values one principal's mode presents, under the
    /// variable names that principal's role reads; none for
    /// [`super::AuthChoice::None`].
    #[must_use]
    pub fn credentials_of(
        choice: super::AuthChoice,
        role: veredictum::arazzo::credential::Role,
        user: String,
        password: String,
        token: String,
    ) -> Vec<crate::engine::Credential> {
        let (user_env, password_env, token_env) = role.variables();
        let credential = |name: &str, value: String| crate::engine::Credential {
            name: name.to_owned(),
            value: crate::engine::Secret::new(value),
        };
        match choice {
            super::AuthChoice::None => Vec::new(),
            super::AuthChoice::Basic => vec![
                credential(user_env, user),
                credential(password_env, password),
            ],
            super::AuthChoice::Bearer => vec![credential(token_env, token)],
        }
    }

    /// Stores the identity labels on this submitter's draft: what the connect
    /// step shows when the operator continues, so an edit made after the
    /// probe reaches the run.
    ///
    /// # Errors
    /// "no connection draft" before a probe, a label past its cap, and the
    /// poisoned-lock diagnostic.
    pub fn save_identity(
        state: &ConsoleState,
        submitter: Submitter,
        sut_name: String,
        sut_version: String,
    ) -> Result<(), String> {
        for (field, value) in [("display name", &sut_name), ("version label", &sut_version)] {
            if count(value.len()) > super::IDENTITY_LABEL_CAP_BYTES {
                return Err(format!(
                    "the {field} is {} bytes; the cap is {}",
                    value.len(),
                    super::IDENTITY_LABEL_CAP_BYTES
                ));
            }
        }
        let mut guard = state.draft.lock().map_err(|e| e.to_string())?;
        let draft = guard
            .get_mut(submitter)
            .ok_or_else(|| String::from(NO_DRAFT))?;
        draft.sut_name = sut_name;
        draft.sut_version = sut_version;
        Ok(())
    }

    /// Reads the server's identity through the pinned engine's `identify`
    /// subcommand, on a blocking thread.
    ///
    /// The console never speaks System OPTIONS itself: the spawned engine
    /// does, with the same credential values the draft holds, under the
    /// variable names `identify` reads.
    ///
    /// # Errors
    /// Never: a failed read is the [`crate::identity::IdentityAnswer::Unread`]
    /// answer, which leaves the identity to the operator.
    pub async fn identify(
        base_url: &str,
        auth: super::AuthChoice,
        user: &str,
        password: &str,
        token: &str,
    ) -> crate::identity::IdentityAnswer {
        let credentials = credentials_of(
            auth,
            veredictum::arazzo::credential::Role::Ordinary,
            user.to_owned(),
            password.to_owned(),
            token.to_owned(),
        );
        let spec = crate::engine::IdentifySpec {
            base_url: base_url.to_owned(),
            auth: auth.token(),
            credentials,
        };
        let read = tokio::task::spawn_blocking(move || {
            crate::engine::locate()
                .and_then(|engine| engine.identify(&spec))
                .map(crate::engine::identity_answer)
                .map_err(|e| e.to_string())
        })
        .await;
        match read {
            Ok(Ok(answer)) => answer,
            Ok(Err(error)) => crate::identity::IdentityAnswer::Unread { error },
            Err(joined) => crate::identity::IdentityAnswer::Unread {
                error: joined.to_string(),
            },
        }
    }

    /// The reachability probe.
    ///
    /// ONE GET of the template list with the supplied credentials, the
    /// answer verbatim. A diagnostic, never a judgement — the one carved-out
    /// console-originated request to a CDR.
    ///
    /// # Errors
    /// Never: an unreachable server is an answer, not an error.
    pub async fn probe(
        base_url: &str,
        auth: super::AuthChoice,
        user: &str,
        password: &str,
        token: &str,
    ) -> super::ProbeAnswer {
        let url = format!(
            "{}/definition/template/adl1.4",
            base_url.trim_end_matches('/')
        );
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
        {
            Ok(client) => client,
            Err(e) => {
                return super::ProbeAnswer::Unreachable {
                    error: e.to_string(),
                };
            }
        };
        let mut request = client.get(&url);
        request = match auth {
            super::AuthChoice::None => request,
            super::AuthChoice::Basic => request.basic_auth(user, Some(password)),
            super::AuthChoice::Bearer => request.bearer_auth(token),
        };
        let started = std::time::Instant::now();
        match request.send().await {
            Ok(response) => {
                let elapsed = started.elapsed().as_millis();
                let status = response.status();
                super::ProbeAnswer::Answered {
                    status: format!(
                        "HTTP {} {}",
                        status.as_u16(),
                        status.canonical_reason().unwrap_or("")
                    ),
                    elapsed_ms: u64::try_from(elapsed).unwrap_or(u64::MAX),
                    ok: status.is_success(),
                }
            }
            Err(e) => super::ProbeAnswer::Unreachable {
                error: e.to_string(),
            },
        }
    }
}

pub mod fns {
    //! The `#[server]` endpoints, one module for one inner suppression.
    #![allow(
        clippy::unused_async,
        clippy::unused_async_trait_impl,
        missing_docs,
        reason = "fires only in some #[server] expansions; see catalogue_api::fns"
    )]

    use leptos::prelude::{ServerFnError, server};

    use super::{
        CapabilityRow, ClaimSummary, ConnectAnswer, CredentialForm, DraftView, PostureForm,
        RunScreen, ScopePreview, StartOutcome,
    };
    use crate::run_job::RunId;

    /// Probes the connection, reads the server's identity through the engine,
    /// and, on any answer, stores the draft with these facts (the probe
    /// outcome seed-gates Continue client-side). The secret values enter the
    /// server-side draft and the spawned `identify` environment, and nothing
    /// else. The administrative credential is optional and separate: the
    /// probe and the identity read never present it, and it reaches only the
    /// run's environment under the `--admin-auth` variable names.
    ///
    /// The probe is the one console-originated request to a CDR, so it is
    /// also one of the two seams a visitor-named target reaches (#390): the
    /// submitter's probe budget is spent and the posture's guard runs BEFORE
    /// the request is built, and a refused target stores no draft at all.
    ///
    /// # Errors
    /// The rate ledger's refusal with its retry, the target guard's refusal
    /// naming the address family, and the draft-store failure — each
    /// verbatim. An unreachable server is an ANSWER, not an error.
    #[server]
    pub async fn probe_and_save(
        base_url: String,
        sut_name: String,
        sut_version: String,
        server: CredentialForm,
        admin: CredentialForm,
    ) -> Result<ConnectAnswer, ServerFnError> {
        let CredentialForm {
            auth,
            user,
            password,
            token,
        } = server;
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        let who = crate::submitter::current(&state);
        state
            .rates
            .admit(who, crate::rate_limit::Metered::Probe)
            .map_err(ServerFnError::new)?;
        crate::target_safety::check(state.posture, &base_url)
            .await
            .map_err(ServerFnError::new)?;
        let answer = super::read::probe(&base_url, auth, &user, &password, &token).await;
        let probed_ok = matches!(answer, super::ProbeAnswer::Answered { ok: true, .. });
        let identity = match answer {
            super::ProbeAnswer::Answered { .. } => {
                super::read::identify(&base_url, auth, &user, &password, &token).await
            }
            super::ProbeAnswer::Unreachable { .. } => crate::identity::IdentityAnswer::Unread {
                error: String::from("nothing answered the reachability probe"),
            },
        };
        let mut credentials = super::read::credentials_of(
            auth,
            veredictum::arazzo::credential::Role::Ordinary,
            user,
            password,
            token,
        );
        let admin_auth = admin.auth;
        credentials.extend(super::read::credentials_of(
            admin.auth,
            veredictum::arazzo::credential::Role::Administrative,
            admin.user,
            admin.password,
            admin.token,
        ));
        super::read::save_connection(
            &state,
            who,
            super::RunDraft {
                profile: veredictum::arazzo::spec_profile::SpecProfile::Development,
                base_url,
                sut_name,
                sut_version,
                identity: Some(identity.clone()),
                auth,
                admin_auth,
                credentials,
                probed_ok,
                statement_json: None,
                statement_product: None,
                filter: None,
                postures: super::DeclaredPostures::default(),
            },
        )
        .map_err(ServerFnError::new)?;
        Ok(ConnectAnswer {
            probe: crate::capture::probe_answer(&state, answer),
            identity,
        })
    }

    /// Stores the identity labels the connect step shows when the operator
    /// continues, so an edit made after the probe reaches the run.
    ///
    /// # Errors
    /// "no connection draft" before a probe, a label past its cap, and the
    /// draft-store failure, each verbatim.
    #[server]
    pub async fn save_identity(sut_name: String, sut_version: String) -> Result<(), ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        let who = crate::submitter::current(&state);
        super::read::save_identity(&state, who, sut_name, sut_version).map_err(ServerFnError::new)
    }

    /// The client-safe draft this submitter has, when they have one.
    ///
    /// # Errors
    /// The server-fn transport only.
    #[server]
    pub async fn fetch_draft() -> Result<Option<DraftView>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        let who = crate::submitter::current(&state);
        let view = super::read::draft_view(&state, who);
        Ok(crate::capture::draft(&state, view))
    }

    /// The scope preview for a filter.
    ///
    /// # Errors
    /// The verbatim load failure when the catalogue is absent.
    #[server]
    pub async fn fetch_scope_preview(filter: String) -> Result<ScopePreview, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        super::read::scope_preview(&state, &filter).map_err(ServerFnError::new)
    }

    /// Starts this submitter's drafted run and answers with the run's id,
    /// which is the address `/run/live/{run_id}` carries.
    ///
    /// A submitter who already has a run in flight is answered with THAT
    /// run's id and nothing is started, so the screen can send them to it.
    ///
    /// # Errors
    /// "no connection draft" before S3, the start budget's refusal, the
    /// target guard's refusal, the engine's refusals, and filesystem
    /// failures — each verbatim.
    #[server]
    pub async fn start_run() -> Result<StartOutcome, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        let who = crate::submitter::current(&state);
        super::read::start_run(&state, who)
            .await
            .map_err(ServerFnError::new)
    }

    /// What the live screen is looking at, for the run the URL names.
    ///
    /// `None` asks about the run THIS SUBMITTER most recently started. The id
    /// is untrusted input, and a parsed UUID keeps the derived directory
    /// under the mounted output root.
    ///
    /// # Errors
    /// The map's poisoned-state diagnostic, or a results document that
    /// exists and cannot be read.
    #[server]
    pub async fn fetch_run(id: Option<RunId>) -> Result<RunScreen, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        let who = crate::submitter::current(&state);
        let screen = super::read::run_screen(&state, who, id).map_err(ServerFnError::new)?;
        Ok(crate::capture::run_screen(&state, screen))
    }

    /// Cancels the NAMED run.
    ///
    /// # Errors
    /// "no run is in flight" when this process is not driving that run, or
    /// the kill's own failure.
    #[server]
    pub async fn cancel_run(id: RunId) -> Result<(), ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        state.jobs.cancel(id).map_err(ServerFnError::new)
    }

    /// Validates and stores the scope half onto the draft, answering with
    /// the claim's summary (`None` for an honest no-claim run).
    ///
    /// # Errors
    /// "no connection draft" when S3 has not run, the statement reader's
    /// refusal verbatim, or the draft-store failure.
    #[server]
    pub async fn save_scope(
        statement_json: Option<String>,
        filter: Option<String>,
        postures: PostureForm,
    ) -> Result<Option<ClaimSummary>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        let who = crate::submitter::current(&state);
        super::read::save_scope(
            &state,
            who,
            statement_json.filter(|s| !s.trim().is_empty()),
            filter.filter(|f| !f.is_empty()),
            &postures,
        )
        .map_err(ServerFnError::new)
    }

    /// Stores the specification profile the run is judged under:
    /// `development` (the default) or `released`.
    ///
    /// # Errors
    /// The refusal of a token that names no profile, "no connection draft"
    /// when S3 has not run, or the draft-store failure.
    #[server]
    pub async fn save_profile(profile: String) -> Result<(), ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        let who = crate::submitter::current(&state);
        super::read::save_profile(&state, who, &profile).map_err(ServerFnError::new)
    }

    /// The capability folders with the documents and workflows each holds.
    ///
    /// # Errors
    /// The catalogue's verbatim load failure.
    #[server]
    pub async fn fetch_capability_rows() -> Result<Vec<CapabilityRow>, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        super::read::capability_rows(&state).map_err(ServerFnError::new)
    }

    /// Composes the ad-hoc claim for a capability selection, answering with
    /// the statement document itself.
    ///
    /// Nothing is stored: the operator saves the document through the same
    /// schema-validated path a pasted one takes, and a name that is not a
    /// capability folder is refused by the composer.
    ///
    /// The argument is optional because the default URL-encoded server-fn
    /// encoding carries an empty sequence as an ABSENT field
    /// (<https://book.leptos.dev/server/25_server_functions.html>), and an
    /// empty selection has to reach the composer's own refusal rather than a
    /// deserialization error.
    ///
    /// # Errors
    /// An empty selection, an unknown folder, a missing connection draft, and
    /// the catalogue's load failure, each verbatim.
    #[server]
    pub async fn compose_claim(capabilities: Option<Vec<String>>) -> Result<String, ServerFnError> {
        let state: crate::state::ConsoleState = leptos::prelude::expect_context();
        let who = crate::submitter::current(&state);
        super::read::compose_claim(&state, who, &capabilities.unwrap_or_default())
            .map_err(ServerFnError::new)
    }
}
