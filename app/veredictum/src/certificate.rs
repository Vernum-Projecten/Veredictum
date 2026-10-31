// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Conformance certificates: issued as a JWS, verified offline against a JWK
//! Set.
//!
//! A certificate is a JSON Web Signature in the compact serialization
//! (RFC 7515 §7.1, <https://www.rfc-editor.org/rfc/rfc7515#section-7.1>),
//! signed with Ed25519 under the `EdDSA` algorithm (RFC 8037 §3.1,
//! <https://www.rfc-editor.org/rfc/rfc8037#section-3.1>). Its protected header
//! carries `alg`, `typ` and the `kid` naming the signing key, and its payload
//! is a [`CertificatePayload`]: what was measured, by which instrument, and
//! the digests of the recorded exchanges. The verifying keys are a JWK Set
//! (RFC 7517 §5, <https://www.rfc-editor.org/rfc/rfc7517#section-5>) of
//! Ed25519 octet key pairs (RFC 8037 §2), so any JOSE library checks a
//! certificate with the published set alone.
//!
//! No openEHR spec governs this — our own design. Two members of a key in the
//! set are this instrument's own: `expires_at`, the last instant the key may
//! sign, and `revoked_at`, which withdraws every certificate the key signed.
//! A generic JOSE library ignores both (RFC 7517 §4), so [`verify`] is what
//! honours them, and the set's reader refuses any member it does not know, so
//! a misspelled `revoked_at` stops the read instead of leaving a revoked key
//! trusted.
//!
//! The signing and the signature check run through `jsonwebtoken`'s crypto
//! layer over `aws-lc-rs`
//! (<https://docs.rs/jsonwebtoken/11.1.0/jsonwebtoken/crypto/index.html>).
//! The header, the payload and the key set are parsed here, so every refusal
//! is its own [`VerifyError`] variant.

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Component, Path};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

use crate::arazzo::results::{Results, Status};
use crate::arazzo::spec_profile::NotReleased;
use crate::party::{IdentityField, InstrumentStatus, Sut};

/// The JWS `alg` every certificate carries: Ed25519 (RFC 8037 §3.1).
pub const ALGORITHM: &str = "EdDSA";

/// The JWS `typ` every certificate carries (RFC 7515 §4.1.9), which keeps a
/// certificate from being read as any other JOSE object the same key might
/// sign.
pub const TYPE: &str = "veredictum-certificate";

/// The file a certificate is written to beside the run it certifies.
pub const CERTIFICATE_FILE: &str = "certificate.jws";

/// What a verified certificate does and does not establish, printed with
/// every verification so the claim is never overread.
pub const HONESTY_LINE: &str = "A valid certificate proves that the holder of the key signed this payload and that it is unchanged; it claims nothing the payload does not name.";

/// The longest key id this instrument issues or accepts.
const KID_MAX: usize = 64;

/// The length of an Ed25519 public key (RFC 8032 §5.1.5).
const ED25519_PUBLIC_KEY_BYTES: usize = 32;

/// A JWK key id (RFC 7517 §4.5).
///
/// One to 64 characters from `A-Z a-z 0-9 . _ -`, so a key id is safe in a
/// file name, a log line and a URL without escaping. No openEHR spec governs
/// this — our own design.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Kid(String);

/// A key id outside the accepted grammar.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("key id {value:?} is not 1 to {KID_MAX} characters from A-Z, a-z, 0-9, `.`, `_` and `-`")]
pub struct KidError {
    /// The refused value, verbatim.
    pub value: String,
}

impl Kid {
    /// Parses a key id.
    ///
    /// # Errors
    /// [`KidError`] when `value` is empty, longer than 64 characters, or
    /// carries a character outside the grammar.
    pub fn parse(value: &str) -> Result<Self, KidError> {
        let well_formed = !value.is_empty()
            && value.len() <= KID_MAX
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        if well_formed {
            Ok(Self(value.to_owned()))
        } else {
            Err(KidError {
                value: value.to_owned(),
            })
        }
    }

    /// The key id as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Kid {
    type Error = KidError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<Kid> for String {
    fn from(kid: Kid) -> Self {
        kid.0
    }
}

impl fmt::Display for Kid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A SHA-256 digest, spelled `sha256:` followed by 64 lowercase hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Sha256Digest([u8; 32]);

/// Text that is not a `sha256:<64 lowercase hex>` digest.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{value:?} is not `sha256:` followed by 64 lowercase hex digits")]
pub struct DigestError {
    /// The refused text, verbatim.
    pub value: String,
}

impl Sha256Digest {
    /// The prefix the spelled digest carries.
    pub const PREFIX: &str = "sha256:";

    /// The digest of `bytes`.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    /// Parses `sha256:<64 lowercase hex>`.
    ///
    /// # Errors
    /// [`DigestError`] for any other text.
    pub fn parse(value: &str) -> Result<Self, DigestError> {
        let refused = || DigestError {
            value: value.to_owned(),
        };
        let hex = value.strip_prefix(Self::PREFIX).ok_or_else(refused)?;
        if hex.len() != 64 {
            return Err(refused());
        }
        let mut bytes = [0_u8; 32];
        let (pairs, _) = hex.as_bytes().as_chunks::<2>();
        for (byte, [high, low]) in bytes.iter_mut().zip(pairs) {
            let (Some(high), Some(low)) = (nibble(*high), nibble(*low)) else {
                return Err(refused());
            };
            *byte = (high << 4) | low;
        }
        Ok(Self(bytes))
    }
}

/// One lowercase hex digit's value.
fn nibble(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        _ => None,
    }
}

/// Lowercase hex of `bytes`.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(
        String::with_capacity(bytes.len().saturating_mul(2)),
        |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        },
    )
}

impl TryFrom<String> for Sha256Digest {
    type Error = DigestError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<Sha256Digest> for String {
    fn from(digest: Sha256Digest) -> Self {
        digest.to_string()
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", Self::PREFIX, hex(&self.0))
    }
}

/// The instrument that issued a certificate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instrument {
    /// The instrument's package name.
    pub name: String,
    /// The instrument's package version, which is also the engine version
    /// that drove and judged the run.
    pub version: String,
    /// The instrument's approval status, derived from its version.
    pub status: InstrumentStatus,
}

/// The catalogue revision a run drove.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogueRevision {
    /// The digest of the catalogue tree, as [`catalogue_digest`] computes it.
    pub digest: Sha256Digest,
}

/// Whether every row that bears on the verdict passed or was skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// No row failed outside the warning ones, and no row errored.
    Green,
    /// At least one row failed outside the warning ones, or errored.
    Red,
}

impl Outcome {
    /// Every outcome, in vocabulary order (schema emission derives from this).
    pub const ALL: &[Outcome] = &[Outcome::Green, Outcome::Red];
}

/// The verdict summary of the run a certificate covers.
///
/// The five counts partition the rows: `failed` counts the failed rows that
/// bear on the verdict, and `warning` the failed rows whose criterion is a
/// SHOULD, which never count against a capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerdictSummary {
    /// Green or red.
    pub outcome: Outcome,
    /// Passed rows.
    pub passed: u64,
    /// Failed rows that bear on the verdict.
    pub failed: u64,
    /// Failed rows whose criterion yields a warning.
    pub warning: u64,
    /// Errored rows: the instrument could not judge them.
    pub errored: u64,
    /// Skipped rows, each with its reason in the results document.
    pub skipped: u64,
}

impl VerdictSummary {
    /// Counts the rows of `results`.
    #[must_use]
    pub fn of(results: &Results) -> Self {
        let mut summary = Self {
            outcome: Outcome::Green,
            passed: 0,
            failed: 0,
            warning: 0,
            errored: 0,
            skipped: 0,
        };
        for row in &results.rows {
            let count = match row.status {
                Status::Passed => &mut summary.passed,
                Status::Skipped => &mut summary.skipped,
                Status::Errored => &mut summary.errored,
                Status::Failed if crate::pipeline::arazzo_run::is_red(row) => &mut summary.failed,
                Status::Failed => &mut summary.warning,
            };
            *count = count.saturating_add(1);
        }
        if summary.failed > 0 || summary.errored > 0 {
            summary.outcome = Outcome::Red;
        }
        summary
    }
}

/// The digests of the recorded exchanges a certificate covers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exchanges {
    /// The digest of the run's `results.json`, byte for byte.
    pub results: Sha256Digest,
    /// The digest of the run's HAR recording, byte for byte.
    pub recording: Sha256Digest,
}

/// What a certificate states: the payload of the JWS.
///
/// Published as `schemas/certificate.schema.json`. Field order is the JSON
/// key order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CertificatePayload {
    /// When the certificate was issued.
    pub issued_at: jiff::Timestamp,
    /// The instrument that issued it.
    pub instrument: Instrument,
    /// The server under test, each field with who supplied it.
    pub sut: Sut,
    /// The base URL the run drove.
    pub base_url: String,
    /// The results document's own name: `catalogue` for a run over every
    /// document, or the one document a narrower run drove.
    pub document: String,
    /// The catalogue revision the run drove.
    pub catalogue: CatalogueRevision,
    /// The verdict summary.
    pub verdict: VerdictSummary,
    /// The digests of the recorded exchanges.
    pub exchanges: Exchanges,
}

/// The recorded run a certificate is issued over.
#[derive(Debug, Clone)]
pub struct RecordedRun<'a> {
    /// The run's `results.json`, byte for byte.
    pub results: &'a [u8],
    /// The run's HAR recording, byte for byte.
    pub recording: &'a [u8],
    /// The server's identity as the run recorded it. Its
    /// `restapi_specs_version` is not read: the certificate states the
    /// release the run's results record under `server`.
    pub sut: Sut,
    /// The catalogue root the run drove.
    pub catalogue: &'a Path,
    /// When the certificate is issued.
    pub issued_at: jiff::Timestamp,
}

impl CertificatePayload {
    /// Builds the payload of a certificate over `run`.
    ///
    /// The results document must be the one this engine wrote: a run judged
    /// by another engine version is refused, so the payload's instrument is
    /// both the engine that judged and the one that issued. It must also be
    /// judged against the released text: a development result is never
    /// certified. The SUT's
    /// `restapi_specs_version` is the one the server stated in that results
    /// document's `server` member, and absent when it stated none.
    ///
    /// # Errors
    /// [`IssueError::Results`] when the results document does not parse,
    /// [`IssueError::EngineMismatch`] when another engine wrote it,
    /// [`IssueError::NotReleased`] when it was judged under the development
    /// profile,
    /// [`IssueError::MissingIdentity`] when the SUT's name or version is
    /// blank, and the [`catalogue_digest`] failures.
    pub fn of_run(run: &RecordedRun<'_>) -> Result<Self, IssueError> {
        let results: Results =
            serde_json::from_slice(run.results).map_err(|source| IssueError::Results { source })?;
        let version = env!("CARGO_PKG_VERSION");
        if results.engine.name != env!("CARGO_PKG_NAME") || results.engine.version != version {
            return Err(IssueError::EngineMismatch {
                recorded: format!("{} {}", results.engine.name, results.engine.version),
                issuing: format!("{} {version}", env!("CARGO_PKG_NAME")),
            });
        }
        results.specification.ensure_released()?;
        if run.sut.name.value.trim().is_empty() || run.sut.version.value.trim().is_empty() {
            return Err(IssueError::MissingIdentity);
        }
        Ok(Self {
            issued_at: run.issued_at,
            instrument: Instrument {
                name: env!("CARGO_PKG_NAME").to_owned(),
                version: version.to_owned(),
                status: InstrumentStatus::CURRENT,
            },
            sut: Sut {
                restapi_specs_version: results
                    .server
                    .as_ref()
                    .and_then(crate::arazzo::results::ServerIdentity::restapi_specs_version)
                    .map(IdentityField::server),
                ..run.sut.clone()
            },
            base_url: results.base_url.clone(),
            document: results.document.clone(),
            catalogue: CatalogueRevision {
                digest: catalogue_digest(run.catalogue)?,
            },
            verdict: VerdictSummary::of(&results),
            exchanges: Exchanges {
                results: Sha256Digest::of(run.results),
                recording: Sha256Digest::of(run.recording),
            },
        })
    }
}

/// The digest of a catalogue tree.
///
/// Every regular file under `root`, named by its path relative to `root` with
/// `/` separators, sorted by the bytes of that path; one line per file,
/// `<64 lowercase hex>  <path>` and a newline, which is the line `sha256sum`
/// prints; and the SHA-256 of those lines. So
/// `find . -type f | sed 's|^\./||' | LC_ALL=C sort | xargs -d '\n' sha256sum | sha256sum`
/// run inside the catalogue root prints the same hex. No openEHR spec governs
/// this — our own design.
///
/// # Errors
/// [`IssueError::Catalogue`] when the tree cannot be read, and
/// [`IssueError::CatalogueEntry`] for an entry the digest cannot name: a
/// symbolic link, or a path that is not UTF-8.
pub fn catalogue_digest(root: &Path) -> Result<Sha256Digest, IssueError> {
    let mut files = BTreeSet::new();
    collect_files(root, root, &mut files)?;
    let mut manifest = String::new();
    for path in &files {
        let body = std::fs::read(root.join(path)).map_err(|source| IssueError::Catalogue {
            path: root.join(path).display().to_string(),
            source,
        })?;
        manifest.push_str(&hex(&Sha256::digest(&body)));
        manifest.push_str("  ");
        manifest.push_str(path);
        manifest.push('\n');
    }
    Ok(Sha256Digest::of(manifest.as_bytes()))
}

/// Collects every regular file under `dir`, relative to `root`.
fn collect_files(root: &Path, dir: &Path, files: &mut BTreeSet<String>) -> Result<(), IssueError> {
    let read_error = |source| IssueError::Catalogue {
        path: dir.display().to_string(),
        source,
    };
    for entry in std::fs::read_dir(dir).map_err(read_error)? {
        let entry = entry.map_err(read_error)?;
        let path = entry.path();
        let kind = entry.file_type().map_err(read_error)?;
        if kind.is_symlink() {
            return Err(IssueError::CatalogueEntry {
                path: path.display().to_string(),
                reason: "a symbolic link",
            });
        }
        if kind.is_dir() {
            collect_files(root, &path, files)?;
        } else {
            files.insert(relative_name(root, &path)?);
        }
    }
    Ok(())
}

/// `path` relative to `root`, with `/` separators.
fn relative_name(root: &Path, path: &Path) -> Result<String, IssueError> {
    let unnamed = |reason| IssueError::CatalogueEntry {
        path: path.display().to_string(),
        reason,
    };
    let relative = path
        .strip_prefix(root)
        .map_err(|_not_under_root| unnamed("outside the catalogue root"))?;
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => {
                parts.push(part.to_str().ok_or_else(|| unnamed("not UTF-8"))?);
            }
            _ => return Err(unnamed("not a plain relative path")),
        }
    }
    Ok(parts.join("/"))
}

/// A certificate: the JWS in the compact serialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Certificate(String);

impl Certificate {
    /// The compact serialization, `header.payload.signature`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The text written to [`CERTIFICATE_FILE`]: the compact serialization
    /// and a trailing newline.
    #[must_use]
    pub fn to_text(&self) -> String {
        format!("{}\n", self.0)
    }
}

impl fmt::Display for Certificate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The protected header this instrument writes.
#[derive(Serialize)]
struct IssuedHeader<'a> {
    alg: &'a str,
    typ: &'a str,
    kid: &'a str,
}

/// The protected header a certificate is read with.
///
/// Members this reader does not know are ignored, as RFC 7515 §4 directs,
/// except under `crit`, which the reader refuses outright (§4.1.11).
#[derive(Deserialize)]
struct ReadHeader {
    alg: String,
    typ: Option<String>,
    kid: Option<String>,
    crit: Option<serde::de::IgnoredAny>,
}

/// An Ed25519 private key with the key id its public half is published under.
///
/// `Debug` never prints the key material.
pub struct SigningKey {
    kid: Kid,
    key: jsonwebtoken::EncodingKey,
}

impl fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SigningKey")
            .field("kid", &self.kid)
            .field("key", &"[redacted]")
            .finish()
    }
}

/// The bytes the key self-checks are signed over.
const PROBE: &[u8] = b"veredictum signing key probe";

impl SigningKey {
    /// Reads an Ed25519 private key in PKCS#8 PEM (RFC 8410 §7), the form
    /// `openssl genpkey -algorithm ed25519` writes.
    ///
    /// The key signs one probe before it is returned, so a key that parses
    /// and cannot sign is refused here rather than at the first issuance.
    ///
    /// # Errors
    /// [`KeyError::Unreadable`] when the text is not an Ed25519 PKCS#8 key,
    /// and [`KeyError::Unusable`] when it does not sign. Neither carries key
    /// material.
    pub fn from_pkcs8_pem(kid: Kid, pem: &[u8]) -> Result<Self, KeyError> {
        let key = jsonwebtoken::EncodingKey::from_ed_pem(pem)
            .map_err(|_key_text_never_carried| KeyError::Unreadable)?;
        jsonwebtoken::crypto::sign(PROBE, &key, jsonwebtoken::Algorithm::EdDSA)
            .map_err(|_key_text_never_carried| KeyError::Unusable)?;
        Ok(Self { kid, key })
    }

    /// The key id its public half is published under.
    #[must_use]
    pub fn kid(&self) -> &Kid {
        &self.kid
    }

    /// Checks that `jwks` publishes this key's public half under its key id,
    /// and that the published key may sign at `now`.
    ///
    /// # Errors
    /// [`KeyError::NotPublished`] when the set holds no key under this id,
    /// [`KeyError::Mismatch`] when the published key does not verify this
    /// key's signature, [`KeyError::Revoked`] when the published key is
    /// revoked, and [`KeyError::Expired`] when it expired before `now`.
    pub fn check_published(&self, jwks: &Jwks, now: jiff::Timestamp) -> Result<(), KeyError> {
        let published = jwks.key(&self.kid).ok_or_else(|| KeyError::NotPublished {
            kid: self.kid.clone(),
        })?;
        let signature =
            jsonwebtoken::crypto::sign(PROBE, &self.key, jsonwebtoken::Algorithm::EdDSA)
                .map_err(|_key_text_never_carried| KeyError::Unusable)?;
        if published.verifies(&signature, PROBE) != Some(true) {
            return Err(KeyError::Mismatch {
                kid: self.kid.clone(),
            });
        }
        if let Some(revoked_at) = published.revoked_at {
            return Err(KeyError::Revoked {
                kid: self.kid.clone(),
                revoked_at,
            });
        }
        match published.expires_at {
            Some(expires_at) if expires_at < now => Err(KeyError::Expired {
                kid: self.kid.clone(),
                expires_at,
            }),
            _ => Ok(()),
        }
    }
}

/// A signing key that cannot be used.
///
/// No variant carries key material, so any of them may be logged.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum KeyError {
    /// The text is not an Ed25519 private key in PKCS#8 PEM.
    #[error("the signing key is not an Ed25519 private key in PKCS#8 PEM")]
    Unreadable,
    /// The key parsed and could not sign.
    #[error("the signing key parsed and could not sign")]
    Unusable,
    /// The published set holds no key under this id.
    #[error("the published JWK Set holds no key `{kid}`")]
    NotPublished {
        /// The signing key's id.
        kid: Kid,
    },
    /// The published key under this id is not this key's public half.
    #[error("the published key `{kid}` is not the public half of the signing key")]
    Mismatch {
        /// The signing key's id.
        kid: Kid,
    },
    /// The published key is revoked.
    #[error("the published key `{kid}` was revoked at {revoked_at}")]
    Revoked {
        /// The signing key's id.
        kid: Kid,
        /// When it was revoked.
        revoked_at: jiff::Timestamp,
    },
    /// The published key expired.
    #[error("the published key `{kid}` expired at {expires_at}")]
    Expired {
        /// The signing key's id.
        kid: Kid,
        /// When it expired.
        expires_at: jiff::Timestamp,
    },
}

/// Why a certificate could not be issued.
#[derive(Debug, Error)]
pub enum IssueError {
    /// The results document did not parse.
    #[error("the results document does not parse: {source}")]
    Results {
        /// The parser's own diagnostic.
        #[source]
        source: serde_json::Error,
    },
    /// Another engine wrote the results document.
    #[error(
        "the results were written by {recorded}, and this certificate would be issued by {issuing}"
    )]
    EngineMismatch {
        /// The engine the results document names.
        recorded: String,
        /// This engine.
        issuing: String,
    },
    /// The SUT's name or version is blank.
    #[error("the server's name and version are both required, and one is blank")]
    MissingIdentity,
    /// The results were judged under the development profile, which never
    /// builds a certificate.
    #[error(transparent)]
    NotReleased(#[from] NotReleased),
    /// The catalogue tree could not be read.
    #[error("the catalogue cannot be read at {path}: {source}")]
    Catalogue {
        /// The path that could not be read.
        path: String,
        /// The filesystem's own error.
        #[source]
        source: std::io::Error,
    },
    /// A catalogue entry the digest cannot name.
    #[error("the catalogue entry {path} cannot be digested: it is {reason}")]
    CatalogueEntry {
        /// The entry.
        path: String,
        /// What it is.
        reason: &'static str,
    },
    /// The header or the payload did not serialize.
    #[error("the certificate does not serialize: {source}")]
    Serialize {
        /// The serializer's own diagnostic.
        #[source]
        source: serde_json::Error,
    },
    /// The signature could not be made.
    #[error("the certificate could not be signed: {source}")]
    Sign {
        /// The signer's own error.
        #[source]
        source: jsonwebtoken::errors::Error,
    },
}

/// Signs `payload` with `key`, returning the certificate.
///
/// # Errors
/// [`IssueError::Serialize`] when the header or the payload does not
/// serialize, which a well-formed value never yields, and [`IssueError::Sign`]
/// when the signature cannot be made.
pub fn issue(payload: &CertificatePayload, key: &SigningKey) -> Result<Certificate, IssueError> {
    let header = serde_json::to_vec(&IssuedHeader {
        alg: ALGORITHM,
        typ: TYPE,
        kid: key.kid.as_str(),
    })
    .map_err(|source| IssueError::Serialize { source })?;
    let body = serde_json::to_vec(payload).map_err(|source| IssueError::Serialize { source })?;
    let message = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(header),
        URL_SAFE_NO_PAD.encode(body)
    );
    let signature =
        jsonwebtoken::crypto::sign(message.as_bytes(), &key.key, jsonwebtoken::Algorithm::EdDSA)
            .map_err(|source| IssueError::Sign { source })?;
    Ok(Certificate(format!("{message}.{signature}")))
}

/// The JWK key type this instrument publishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum KeyType {
    /// An octet key pair (RFC 8037 §2).
    #[serde(rename = "OKP")]
    Okp,
}

/// The JWK curve this instrument publishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Curve {
    /// Ed25519 (RFC 8037 §2).
    Ed25519,
}

/// The JWK `alg` this instrument publishes (RFC 7517 §4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum KeyAlgorithm {
    /// Ed25519 (RFC 8037 §3.1).
    #[serde(rename = "EdDSA")]
    EdDsa,
}

/// The JWK `use` this instrument publishes (RFC 7517 §4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum KeyUse {
    /// A signature key.
    #[serde(rename = "sig")]
    Signature,
}

/// One key of the set, exactly as read.
///
/// `kty`, `crv`, `alg` and `use` are read only to be refused when they name
/// anything but an Ed25519 signature key: each is a one-value vocabulary.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawJwk {
    #[serde(rename = "kty")]
    _kty: KeyType,
    #[serde(rename = "crv")]
    _crv: Curve,
    x: String,
    kid: Kid,
    #[serde(rename = "alg")]
    _alg: Option<KeyAlgorithm>,
    #[serde(rename = "use")]
    _use: Option<KeyUse>,
    d: Option<serde::de::IgnoredAny>,
    expires_at: Option<jiff::Timestamp>,
    revoked_at: Option<jiff::Timestamp>,
}

/// The set, exactly as read.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawJwks {
    keys: Vec<RawJwk>,
}

/// One Ed25519 verifying key of a JWK Set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jwk {
    kid: Kid,
    x: String,
    expires_at: Option<jiff::Timestamp>,
    revoked_at: Option<jiff::Timestamp>,
}

impl Jwk {
    /// The key id.
    #[must_use]
    pub fn kid(&self) -> &Kid {
        &self.kid
    }

    /// The last instant the key may sign, when the set states one.
    #[must_use]
    pub fn expires_at(&self) -> Option<jiff::Timestamp> {
        self.expires_at
    }

    /// When the key was revoked, when it was.
    #[must_use]
    pub fn revoked_at(&self) -> Option<jiff::Timestamp> {
        self.revoked_at
    }

    /// Whether `signature` (base64url) is this key's signature over
    /// `message`; `None` when the signature is not base64url.
    fn verifies(&self, signature: &str, message: &[u8]) -> Option<bool> {
        // NOTE: no openEHR spec governs this — our own design; `x` was decoded
        // at parse, so a refusal here is the signature's encoding, which the
        // caller reports as a malformed signature.
        let key = jsonwebtoken::DecodingKey::from_ed_components(&self.x).ok()?;
        jsonwebtoken::crypto::verify(signature, message, &key, jsonwebtoken::Algorithm::EdDSA).ok()
    }
}

/// A JWK Set of Ed25519 verifying keys (RFC 7517 §5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jwks {
    keys: Vec<Jwk>,
}

/// A JWK Set this instrument refuses to read.
#[derive(Debug, Error)]
pub enum JwksError {
    /// The document is not a set of Ed25519 keys this reader knows, or a key
    /// carries a member it does not know.
    #[error("the JWK Set does not parse: {source}")]
    Parse {
        /// The parser's own diagnostic.
        #[source]
        source: serde_json::Error,
    },
    /// A key carries private key material (`d`, RFC 8037 §2).
    #[error(
        "the key `{kid}` carries its private half (`d`), which a published set must never hold"
    )]
    PrivateKeyMaterial {
        /// The key's id.
        kid: Kid,
    },
    /// Two keys share an id, so a certificate's `kid` would not name one.
    #[error("the key id `{kid}` appears twice")]
    DuplicateKid {
        /// The repeated id.
        kid: Kid,
    },
    /// A key's `x` is not 32 bytes of base64url.
    #[error("the key `{kid}` has an `x` that is not 32 bytes of base64url")]
    KeyMaterial {
        /// The key's id.
        kid: Kid,
    },
}

impl Jwks {
    /// Reads a JWK Set.
    ///
    /// # Errors
    /// The [`JwksError`] naming the first refusal.
    pub fn parse(text: &str) -> Result<Self, JwksError> {
        let raw: RawJwks =
            serde_json::from_str(text).map_err(|source| JwksError::Parse { source })?;
        let mut seen = BTreeSet::new();
        let mut keys = Vec::with_capacity(raw.keys.len());
        for key in raw.keys {
            if key.d.is_some() {
                return Err(JwksError::PrivateKeyMaterial { kid: key.kid });
            }
            if !seen.insert(key.kid.clone()) {
                return Err(JwksError::DuplicateKid { kid: key.kid });
            }
            let decoded = URL_SAFE_NO_PAD.decode(key.x.as_bytes());
            if decoded.map(|x| x.len()) != Ok(ED25519_PUBLIC_KEY_BYTES) {
                return Err(JwksError::KeyMaterial { kid: key.kid });
            }
            keys.push(Jwk {
                kid: key.kid,
                x: key.x,
                expires_at: key.expires_at,
                revoked_at: key.revoked_at,
            });
        }
        Ok(Self { keys })
    }

    /// The key under `kid`.
    #[must_use]
    pub fn key(&self, kid: &Kid) -> Option<&Jwk> {
        self.keys.iter().find(|key| &key.kid == kid)
    }

    /// Every key, in document order.
    #[must_use]
    pub fn keys(&self) -> &[Jwk] {
        &self.keys
    }
}

/// A certificate that verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    /// The key that signed it.
    pub kid: Kid,
    /// What it states.
    pub payload: CertificatePayload,
}

/// Which part of a certificate is not well formed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// The text is not three base64url segments joined by `.`.
    Serialization,
    /// The protected header.
    Header,
    /// The payload.
    Payload,
    /// The signature.
    Signature,
}

impl fmt::Display for Part {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Serialization => "the compact serialization",
            Self::Header => "the protected header",
            Self::Payload => "the payload",
            Self::Signature => "the signature",
        })
    }
}

/// Why a certificate is refused.
#[derive(Debug, Error)]
pub enum VerifyError {
    /// A part of the certificate is not well formed.
    #[error("{part} is not well formed")]
    Malformed {
        /// Which part.
        part: Part,
    },
    /// The header names extensions that must be understood (`crit`), and this
    /// reader understands none.
    #[error("the header carries `crit`, and this reader understands no critical extension")]
    CriticalHeader,
    /// The header's `alg` is not `EdDSA`.
    #[error("the algorithm is {alg:?}, and a certificate is signed with {ALGORITHM:?}")]
    WrongAlgorithm {
        /// The `alg` the header carries.
        alg: String,
    },
    /// The header's `typ` is not this instrument's certificate type.
    #[error("the type is {typ:?}, and a certificate carries {TYPE:?}")]
    WrongType {
        /// The `typ` the header carries, when it carries one.
        typ: Option<String>,
    },
    /// The header carries no `kid`.
    #[error("the header names no key (`kid`)")]
    MissingKid,
    /// The set holds no key under the header's `kid`.
    #[error("the JWK Set holds no key {kid:?}")]
    UnknownKid {
        /// The `kid` the header carries.
        kid: String,
    },
    /// The signature does not verify: the header or the payload changed
    /// after signing, or another key signed it.
    #[error(
        "the signature does not verify against the key `{kid}`: the header or the payload is not what the key signed"
    )]
    SignatureMismatch {
        /// The key the header names.
        kid: Kid,
    },
    /// The key is revoked, so nothing it signed is trusted.
    #[error("the key `{kid}` was revoked at {revoked_at}, so no certificate it signed is trusted")]
    KeyRevoked {
        /// The key.
        kid: Kid,
        /// When it was revoked.
        revoked_at: jiff::Timestamp,
    },
    /// The certificate was issued after its key expired.
    #[error(
        "the key `{kid}` expired at {expires_at}, before this certificate was issued at {issued_at}"
    )]
    KeyExpired {
        /// The key.
        kid: Kid,
        /// When it expired.
        expires_at: jiff::Timestamp,
        /// When the certificate says it was issued.
        issued_at: jiff::Timestamp,
    },
    /// The signed payload is not a certificate payload.
    #[error("the payload is not a certificate: {source}")]
    Payload {
        /// The parser's own diagnostic.
        #[source]
        source: serde_json::Error,
    },
}

/// Verifies `certificate` against `jwks`, offline.
///
/// Surrounding whitespace is ignored, so a certificate file with a trailing
/// newline reads as the certificate. The checks run in this order: the
/// serialization, the header (`crit`, `alg`, `typ`, `kid`), the signature,
/// the key's revocation, the payload, and the key's expiry against the
/// payload's `issued_at`, which is trusted only once the signature holds.
///
/// # Errors
/// The [`VerifyError`] naming the first refusal.
pub fn verify(certificate: &str, jwks: &Jwks) -> Result<Verified, VerifyError> {
    let malformed = |part| VerifyError::Malformed { part };
    let mut segments = certificate.trim().split('.');
    let (Some(header_b64), Some(payload_b64), Some(signature_b64), None) = (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) else {
        return Err(malformed(Part::Serialization));
    };
    let header_bytes = URL_SAFE_NO_PAD
        .decode(header_b64)
        .map_err(|_not_a_header| malformed(Part::Header))?;
    let header: ReadHeader =
        serde_json::from_slice(&header_bytes).map_err(|_not_a_header| malformed(Part::Header))?;
    if header.crit.is_some() {
        return Err(VerifyError::CriticalHeader);
    }
    if header.alg != ALGORITHM {
        return Err(VerifyError::WrongAlgorithm { alg: header.alg });
    }
    if header.typ.as_deref() != Some(TYPE) {
        return Err(VerifyError::WrongType { typ: header.typ });
    }
    let kid_text = header.kid.ok_or(VerifyError::MissingKid)?;
    let key = Kid::parse(&kid_text)
        .ok()
        .and_then(|kid| jwks.key(&kid))
        .ok_or(VerifyError::UnknownKid { kid: kid_text })?;
    let message = format!("{header_b64}.{payload_b64}");
    match key.verifies(signature_b64, message.as_bytes()) {
        Some(true) => {}
        Some(false) => {
            return Err(VerifyError::SignatureMismatch {
                kid: key.kid.clone(),
            });
        }
        None => return Err(malformed(Part::Signature)),
    }
    if let Some(revoked_at) = key.revoked_at {
        return Err(VerifyError::KeyRevoked {
            kid: key.kid.clone(),
            revoked_at,
        });
    }
    let payload_bytes = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|_not_base64url| malformed(Part::Payload))?;
    let payload: CertificatePayload =
        serde_json::from_slice(&payload_bytes).map_err(|source| VerifyError::Payload { source })?;
    if let Some(expires_at) = key.expires_at
        && payload.issued_at > expires_at
    {
        return Err(VerifyError::KeyExpired {
            kid: key.kid.clone(),
            expires_at,
            issued_at: payload.issued_at,
        });
    }
    Ok(Verified {
        kid: key.kid.clone(),
        payload,
    })
}
#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::party::IdentityField;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// The public key of RFC 8037 Appendix A.1, which is not the test key.
    const OTHER_X: &str = "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo";

    /// The test key's own public half, as the committed test set spells it.
    const TEST_X: &str = "QtjMNDbWhebpipQfQRa_jxiUDLPK4krAsqyY_qQgECs";

    fn repo() -> PathBuf {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).to_path_buf()
    }

    /// The committed test key, published in the committed test set.
    fn test_key() -> Result<SigningKey, Box<dyn std::error::Error>> {
        let pem = std::fs::read(repo().join("fixtures/certificates/test-signing.key.pem"))?;
        Ok(SigningKey::from_pkcs8_pem(
            Kid::parse("veredictum-test")?,
            &pem,
        )?)
    }

    fn test_jwks_text() -> Result<String, std::io::Error> {
        std::fs::read_to_string(repo().join("fixtures/certificates/jwks.json"))
    }

    fn test_jwks() -> Result<Jwks, Box<dyn std::error::Error>> {
        Ok(Jwks::parse(&test_jwks_text()?)?)
    }

    /// The test set with one extra member on its one key.
    fn test_jwks_with(member: &str, value: &str) -> Result<Jwks, Box<dyn std::error::Error>> {
        let text = test_jwks_text()?.replace(
            r#""use": "sig""#,
            &format!(r#""use": "sig", "{member}": {value}"#),
        );
        Ok(Jwks::parse(&text)?)
    }

    /// A results document written by this engine, with one row in each
    /// status and one warning failure.
    fn results_text() -> String {
        let why = r#"{ "source": "specs/openehr/ITS-REST/x.adoc", "section": "s", "quote": "q" }"#;
        let explanation = |whose: &str| {
            format!(
                r#"{{ "step": "s", "what": "w", "why": {why}, "expected": "e", "received": "r", "whose": "{whose}" }}"#
            )
        };
        format!(
            r#"{{
  "document": "catalogue",
  "engine": {{ "name": "veredictum", "version": "{version}" }},
  "instrument_status": "{status}",
  "base_url": "http://sut.example/openehr/v1",
  "rows": [
    {{ "case": "ehr/a", "row": 0, "status": "passed", "inputs": {{}}, "generated": [] }},
    {{ "case": "ehr/b", "row": 0, "status": "failed", "inputs": {{}}, "generated": [], "explanation": {server} }},
    {{ "case": "ehr/c", "row": 0, "status": "failed", "inputs": {{}}, "generated": [], "explanation": {warning} }},
    {{ "case": "ehr/d", "row": 0, "status": "errored", "inputs": {{}}, "generated": [], "explanation": {instrument} }},
    {{ "case": "ehr/e", "row": 0, "status": "skipped", "inputs": {{}}, "generated": [], "skip": {{ "reason": "not-claimed", "why": {why} }} }}
  ]
}}
"#,
            version = env!("CARGO_PKG_VERSION"),
            status = InstrumentStatus::CURRENT.token(),
            server = explanation("server"),
            warning = explanation("warning"),
            instrument = explanation("instrument"),
        )
    }

    fn sut() -> Sut {
        Sut {
            name: IdentityField::server("ExampleCDR"),
            version: IdentityField::operator("2.1.0"),
            vendor: Some(IdentityField::server("Example")),
            restapi_specs_version: None,
        }
    }

    fn issued_at() -> Result<jiff::Timestamp, jiff::Error> {
        "2026-10-29T12:00:00Z".parse()
    }

    const RECORDING: &[u8] = b"{\"log\":{}}\n";

    fn payload() -> Result<CertificatePayload, Box<dyn std::error::Error>> {
        let results = results_text();
        Ok(CertificatePayload::of_run(&RecordedRun {
            results: results.as_bytes(),
            recording: RECORDING,
            sut: sut(),
            catalogue: &repo().join("fixtures/declaration"),
            issued_at: issued_at()?,
        })?)
    }

    fn certificate() -> Result<Certificate, Box<dyn std::error::Error>> {
        Ok(issue(&payload()?, &test_key()?)?)
    }

    /// The certificate with one segment of its compact serialization replaced.
    fn with_segment(certificate: &Certificate, index: usize, segment: &str) -> String {
        let mut parts: Vec<&str> = certificate.as_str().split('.').collect();
        parts[index] = segment;
        parts.join(".")
    }

    /// One segment of the certificate, decoded to text.
    fn decoded(
        certificate: &Certificate,
        index: usize,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let part = certificate
            .as_str()
            .split('.')
            .nth(index)
            .ok_or("no such segment")?;
        Ok(String::from_utf8(URL_SAFE_NO_PAD.decode(part)?)?)
    }

    #[test]
    fn an_issued_certificate_verifies_offline_and_returns_its_payload() -> TestResult {
        let certificate = certificate()?;
        let verified = verify(&certificate.to_text(), &test_jwks()?)?;
        assert_eq!(verified.kid.as_str(), "veredictum-test");
        assert_eq!(verified.payload, payload()?);
        Ok(())
    }

    #[test]
    fn the_header_names_the_algorithm_the_type_and_the_key() -> TestResult {
        assert_eq!(
            decoded(&certificate()?, 0)?,
            r#"{"alg":"EdDSA","typ":"veredictum-certificate","kid":"veredictum-test"}"#
        );
        Ok(())
    }

    #[test]
    fn the_payload_names_what_was_measured() -> TestResult {
        let results = results_text();
        let payload = payload()?;
        assert_eq!(payload.instrument.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(payload.instrument.status, InstrumentStatus::CURRENT);
        assert_eq!(payload.sut, sut());
        assert_eq!(payload.base_url, "http://sut.example/openehr/v1");
        assert_eq!(payload.document, "catalogue");
        assert_eq!(
            payload.catalogue.digest,
            catalogue_digest(&repo().join("fixtures/declaration"))?
        );
        assert_eq!(
            payload.verdict,
            VerdictSummary {
                outcome: Outcome::Red,
                passed: 1,
                failed: 1,
                warning: 1,
                errored: 1,
                skipped: 1,
            }
        );
        assert_eq!(
            payload.exchanges.results,
            Sha256Digest::of(results.as_bytes())
        );
        assert_eq!(payload.exchanges.recording, Sha256Digest::of(RECORDING));
        Ok(())
    }

    #[test]
    #[expect(
        clippy::disallowed_types,
        reason = "the schema validator reads a JSON value"
    )]
    fn the_payload_validates_against_its_published_schema() -> TestResult {
        let schema = crate::schema::certificate_schema();
        let validator = jsonschema::validator_for(&schema)?;
        let instance: serde_json::Value = serde_json::from_str(&decoded(&certificate()?, 1)?)?;
        let errors: Vec<String> = validator
            .iter_errors(&instance)
            .map(|e| e.to_string())
            .collect();
        assert!(errors.is_empty(), "{errors:?}");
        Ok(())
    }

    #[test]
    #[expect(
        clippy::disallowed_types,
        reason = "the schema validator reads a JSON value"
    )]
    fn every_committed_key_set_reads_and_validates_against_its_published_schema() -> TestResult {
        let schema = crate::schema::jwks_schema();
        let validator = jsonschema::validator_for(&schema)?;
        for path in ["fixtures/certificates/jwks.json", "deploy/hosted/jwks.json"] {
            let text = std::fs::read_to_string(repo().join(path))?;
            Jwks::parse(&text)?;
            let instance: serde_json::Value = serde_json::from_str(&text)?;
            let errors: Vec<String> = validator
                .iter_errors(&instance)
                .map(|e| e.to_string())
                .collect();
            assert!(errors.is_empty(), "{path}: {errors:?}");
        }
        Ok(())
    }

    #[test]
    fn a_modified_payload_is_refused_as_a_signature_mismatch() -> TestResult {
        let certificate = certificate()?;
        let forged =
            decoded(&certificate, 1)?.replace(r#""outcome":"red""#, r#""outcome":"green""#);
        assert_ne!(
            forged,
            decoded(&certificate, 1)?,
            "the forgery changed something"
        );
        let tampered = with_segment(&certificate, 1, &URL_SAFE_NO_PAD.encode(forged));
        let refused = verify(&tampered, &test_jwks()?);
        assert!(
            matches!(refused, Err(VerifyError::SignatureMismatch { .. })),
            "{refused:?}"
        );
        Ok(())
    }

    #[test]
    fn a_certificate_checked_against_another_key_under_its_kid_is_refused() -> TestResult {
        let other = Jwks::parse(&test_jwks_text()?.replace(TEST_X, OTHER_X))?;
        let refused = verify(certificate()?.as_str(), &other);
        assert!(
            matches!(refused, Err(VerifyError::SignatureMismatch { .. })),
            "{refused:?}"
        );
        Ok(())
    }

    #[test]
    fn an_unknown_kid_is_refused() -> TestResult {
        let renamed =
            Jwks::parse(&test_jwks_text()?.replace("veredictum-test", "veredictum-other"))?;
        let refused = verify(certificate()?.as_str(), &renamed);
        assert!(
            matches!(&refused, Err(VerifyError::UnknownKid { kid }) if kid == "veredictum-test"),
            "{refused:?}"
        );
        Ok(())
    }

    #[test]
    fn a_wrong_algorithm_is_refused() -> TestResult {
        let certificate = certificate()?;
        for alg in ["HS256", "none", "ES256", "RS256"] {
            let header = format!(
                r#"{{"alg":"{alg}","typ":"veredictum-certificate","kid":"veredictum-test"}}"#
            );
            let forged = with_segment(&certificate, 0, &URL_SAFE_NO_PAD.encode(header));
            let refused = verify(&forged, &test_jwks()?);
            assert!(
                matches!(&refused, Err(VerifyError::WrongAlgorithm { alg: found }) if found == alg),
                "{alg}: {refused:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn an_expired_key_refuses_a_certificate_issued_after_it_expired() -> TestResult {
        let expired = test_jwks_with("expires_at", r#""2026-10-01T00:00:00Z""#)?;
        let refused = verify(certificate()?.as_str(), &expired);
        assert!(
            matches!(refused, Err(VerifyError::KeyExpired { .. })),
            "{refused:?}"
        );
        // A key that expires later still verifies what it signed before then.
        let later = test_jwks_with("expires_at", r#""2027-10-01T00:00:00Z""#)?;
        assert!(verify(certificate()?.as_str(), &later).is_ok());
        Ok(())
    }

    #[test]
    fn a_revoked_key_refuses_everything_it_signed() -> TestResult {
        let revoked = test_jwks_with("revoked_at", r#""2027-01-01T00:00:00Z""#)?;
        let refused = verify(certificate()?.as_str(), &revoked);
        assert!(
            matches!(refused, Err(VerifyError::KeyRevoked { .. })),
            "{refused:?}"
        );
        Ok(())
    }

    #[test]
    fn a_header_without_kid_of_another_type_or_with_crit_is_refused() -> TestResult {
        let certificate = certificate()?;
        let jwks = test_jwks()?;
        let refusal = |header: &str| {
            verify(
                &with_segment(&certificate, 0, &URL_SAFE_NO_PAD.encode(header)),
                &jwks,
            )
        };
        let missing = refusal(r#"{"alg":"EdDSA","typ":"veredictum-certificate"}"#);
        assert!(
            matches!(missing, Err(VerifyError::MissingKid)),
            "{missing:?}"
        );
        for header in [
            r#"{"alg":"EdDSA","typ":"JWT","kid":"veredictum-test"}"#,
            r#"{"alg":"EdDSA","kid":"veredictum-test"}"#,
        ] {
            let typed = refusal(header);
            assert!(
                matches!(typed, Err(VerifyError::WrongType { .. })),
                "{typed:?}"
            );
        }
        let crit = refusal(
            r#"{"alg":"EdDSA","typ":"veredictum-certificate","kid":"veredictum-test","crit":["b64"],"b64":false}"#,
        );
        assert!(matches!(crit, Err(VerifyError::CriticalHeader)), "{crit:?}");
        Ok(())
    }

    #[test]
    fn a_malformed_certificate_names_the_part() -> TestResult {
        let certificate = certificate()?;
        let jwks = test_jwks()?;
        let part_of = |text: &str| match verify(text, &jwks) {
            Err(VerifyError::Malformed { part }) => Some(part),
            _ => None,
        };
        assert_eq!(part_of("only.two"), Some(Part::Serialization));
        assert_eq!(part_of("a.b.c.d"), Some(Part::Serialization));
        assert_eq!(
            part_of(&with_segment(&certificate, 0, "!!")),
            Some(Part::Header)
        );
        assert_eq!(
            part_of(&with_segment(&certificate, 2, "!!")),
            Some(Part::Signature)
        );
        Ok(())
    }

    #[test]
    fn a_signed_payload_that_is_not_a_certificate_is_refused() -> TestResult {
        let key = test_key()?;
        let header = URL_SAFE_NO_PAD
            .encode(r#"{"alg":"EdDSA","typ":"veredictum-certificate","kid":"veredictum-test"}"#);
        let body = URL_SAFE_NO_PAD.encode(r#"{"issued_at":"2026-10-29T12:00:00Z"}"#);
        let message = format!("{header}.{body}");
        let signature = jsonwebtoken::crypto::sign(
            message.as_bytes(),
            &key.key,
            jsonwebtoken::Algorithm::EdDSA,
        )?;
        let refused = verify(&format!("{message}.{signature}"), &test_jwks()?);
        assert!(
            matches!(refused, Err(VerifyError::Payload { .. })),
            "{refused:?}"
        );
        Ok(())
    }

    #[test]
    fn a_key_set_refuses_private_material_unknown_members_duplicates_and_short_keys() -> TestResult
    {
        let text = test_jwks_text()?;
        let with =
            |extra: &str| text.replace(r#""use": "sig""#, &format!(r#""use": "sig", {extra}"#));
        assert!(matches!(
            Jwks::parse(&with(r#""d": "AAAA""#)),
            Err(JwksError::PrivateKeyMaterial { .. })
        ));
        assert!(matches!(
            Jwks::parse(&with(r#""revokd_at": "2027-01-01T00:00:00Z""#)),
            Err(JwksError::Parse { .. })
        ));
        assert!(matches!(
            Jwks::parse(&text.replace(r#""kty": "OKP""#, r#""kty": "RSA""#)),
            Err(JwksError::Parse { .. })
        ));
        assert!(matches!(
            Jwks::parse(&text.replace(r#""alg": "EdDSA""#, r#""alg": "RS256""#)),
            Err(JwksError::Parse { .. })
        ));
        assert!(matches!(
            Jwks::parse(&text.replace(TEST_X, "QtjM")),
            Err(JwksError::KeyMaterial { .. })
        ));
        let key = text
            .split_once("\"keys\": [")
            .and_then(|(_, rest)| rest.rsplit_once(']'))
            .map(|(key, _)| key.trim().to_owned())
            .ok_or("the set holds one key")?;
        assert!(matches!(
            Jwks::parse(&format!(r#"{{ "keys": [{key}, {key}] }}"#)),
            Err(JwksError::DuplicateKid { .. })
        ));
        Ok(())
    }

    #[test]
    fn a_signing_key_is_checked_against_the_set_that_publishes_it() -> TestResult {
        let key = test_key()?;
        let now = issued_at()?;
        assert_eq!(key.check_published(&test_jwks()?, now), Ok(()));
        let empty = Jwks::parse(r#"{ "keys": [] }"#)?;
        assert!(matches!(
            key.check_published(&empty, now),
            Err(KeyError::NotPublished { .. })
        ));
        let other = Jwks::parse(&test_jwks_text()?.replace(TEST_X, OTHER_X))?;
        assert!(matches!(
            key.check_published(&other, now),
            Err(KeyError::Mismatch { .. })
        ));
        let revoked = test_jwks_with("revoked_at", r#""2026-01-01T00:00:00Z""#)?;
        assert!(matches!(
            key.check_published(&revoked, now),
            Err(KeyError::Revoked { .. })
        ));
        let expired = test_jwks_with("expires_at", r#""2026-01-01T00:00:00Z""#)?;
        assert!(matches!(
            key.check_published(&expired, now),
            Err(KeyError::Expired { .. })
        ));
        Ok(())
    }

    #[test]
    fn a_signing_key_never_prints_its_material() -> TestResult {
        let pem =
            std::fs::read_to_string(repo().join("fixtures/certificates/test-signing.key.pem"))?;
        let printed = format!("{:?}", test_key()?);
        let body = pem.lines().nth(1).ok_or("a PEM body line")?;
        assert!(!printed.contains(body), "{printed}");
        assert!(printed.contains("[redacted]"));
        assert_eq!(
            SigningKey::from_pkcs8_pem(Kid::parse("k")?, b"not a key").err(),
            Some(KeyError::Unreadable)
        );
        Ok(())
    }

    /// The release a certificate states is the one the run's results record
    /// under `server`, whatever the caller's identity carries.
    #[test]
    fn the_certificate_states_the_release_the_run_recorded() -> TestResult {
        let catalogue = repo().join("fixtures/declaration");
        let caller = Sut {
            restapi_specs_version: Some(IdentityField::server("9.9.9")),
            ..sut()
        };
        let stated = results_text().replacen(
            "\"rows\":",
            r#""server": { "read": "read", "answer": { "outcome": "offered", "status": 200,
                "options": { "restapi_specs_version": "1.0.3" } } },
  "rows":"#,
            1,
        );
        let payload = CertificatePayload::of_run(&RecordedRun {
            results: stated.as_bytes(),
            recording: RECORDING,
            sut: caller.clone(),
            catalogue: &catalogue,
            issued_at: issued_at()?,
        })?;
        assert_eq!(
            payload.sut.restapi_specs_version,
            Some(IdentityField::server("1.0.3"))
        );
        assert_eq!(payload.sut.name, caller.name);
        let silent = results_text();
        let payload = CertificatePayload::of_run(&RecordedRun {
            results: silent.as_bytes(),
            recording: RECORDING,
            sut: caller,
            catalogue: &catalogue,
            issued_at: issued_at()?,
        })?;
        assert_eq!(payload.sut.restapi_specs_version, None);
        Ok(())
    }

    #[test]
    fn a_run_another_engine_judged_or_without_an_identity_is_not_certified() -> TestResult {
        let other = results_text().replace(
            &format!(r#""version": "{}""#, env!("CARGO_PKG_VERSION")),
            r#""version": "0.0.0-other""#,
        );
        let catalogue = repo().join("fixtures/declaration");
        let run = RecordedRun {
            results: other.as_bytes(),
            recording: RECORDING,
            sut: sut(),
            catalogue: &catalogue,
            issued_at: issued_at()?,
        };
        assert!(matches!(
            CertificatePayload::of_run(&run),
            Err(IssueError::EngineMismatch { .. })
        ));
        let results = results_text();
        let blank = RecordedRun {
            results: results.as_bytes(),
            sut: Sut::operator("ExampleCDR", " "),
            ..run
        };
        assert!(matches!(
            CertificatePayload::of_run(&blank),
            Err(IssueError::MissingIdentity)
        ));
        Ok(())
    }

    #[test]
    fn a_development_result_is_never_certified() -> TestResult {
        let development = results_text().replacen(
            "\"base_url\":",
            r#""specification": { "profile": "development", "snapshot": [
      { "component": "RM", "repository": "https://github.com/openEHR/specifications-RM",
        "branch": "master", "commit": "654296bb027b03b4b076101e7171f3359f44cebc" } ] },
  "base_url":"#,
            1,
        );
        let catalogue = repo().join("fixtures/declaration");
        let refused = CertificatePayload::of_run(&RecordedRun {
            results: development.as_bytes(),
            recording: RECORDING,
            sut: sut(),
            catalogue: &catalogue,
            issued_at: issued_at()?,
        });
        assert!(
            matches!(&refused, Err(IssueError::NotReleased(NotReleased { label })) if label.contains("RM master@654296bb027b")),
            "{refused:?}"
        );
        let released = results_text().replacen(
            "\"base_url\":",
            "\"specification\": { \"profile\": \"released\" },\n  \"base_url\":",
            1,
        );
        assert!(
            CertificatePayload::of_run(&RecordedRun {
                results: released.as_bytes(),
                recording: RECORDING,
                sut: sut(),
                catalogue: &catalogue,
                issued_at: issued_at()?,
            })
            .is_ok()
        );
        Ok(())
    }

    /// The digest recomputed from its definition, over a tree whose byte
    /// order (`cases/Z.yaml` before `cases/ehr/…`) differs from a
    /// case-folded order.
    #[test]
    fn the_catalogue_digest_is_the_sha256sum_manifest_of_the_tree() -> TestResult {
        let dir = assert_fs::TempDir::new()?;
        std::fs::create_dir_all(dir.path().join("cases/ehr"))?;
        std::fs::write(dir.path().join("capabilities.yaml"), b"a\n")?;
        std::fs::write(dir.path().join("cases/ehr/ehr.arazzo.yaml"), b"b\n")?;
        std::fs::write(dir.path().join("cases/Z.yaml"), b"c\n")?;
        let line = |body: &[u8], path: &str| format!("{}  {path}\n", hex(&Sha256::digest(body)));
        let manifest = [
            line(b"a\n", "capabilities.yaml"),
            line(b"c\n", "cases/Z.yaml"),
            line(b"b\n", "cases/ehr/ehr.arazzo.yaml"),
        ]
        .concat();
        assert_eq!(
            catalogue_digest(dir.path())?,
            Sha256Digest::of(manifest.as_bytes())
        );
        Ok(())
    }

    #[test]
    fn a_digest_and_a_kid_round_trip_and_refuse_other_text() -> TestResult {
        let digest = Sha256Digest::of(b"x");
        assert_eq!(Sha256Digest::parse(&digest.to_string())?, digest);
        let upper = digest.to_string().to_uppercase();
        for bad in ["sha256:", "sha256:XY", "md5:00", upper.as_str()] {
            assert!(Sha256Digest::parse(bad).is_err(), "{bad}");
        }
        let long = "k".repeat(65);
        for bad in ["", "has space", "slash/no", long.as_str()] {
            assert!(Kid::parse(bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            Kid::parse("veredictum-2026.1_a")?.as_str(),
            "veredictum-2026.1_a"
        );
        Ok(())
    }
}
