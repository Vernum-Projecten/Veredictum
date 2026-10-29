// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Certificate issuance on this console, and the trial mode that keeps it
//! off.
//!
//! One environment setting decides it, [`ISSUANCE_ENV`]. Only the exact value
//! `issue` turns issuance on; unset, `trial`, or anything else is trial mode,
//! so a typo never starts signing. In trial mode runs, results and verdicts
//! work as ever, no certificate is created, every page says so, and no signing
//! key is read: [`ssr::Issuance::load`] does not open the key file at all.
//!
//! With issuance on, the console reads the Ed25519 key [`KEY_FILE_ENV`] names,
//! checks it against the JWK Set this console publishes at [`JWKS_ROUTE`], and
//! refuses to start on a key the set does not publish, revokes or has let
//! expire. A finished run then gets [`veredictum::certificate::CERTIFICATE_FILE`]
//! beside its results, issued by the engine's own `certificate` module. No
//! openEHR spec governs this — our own design.

use serde::{Deserialize, Serialize};

/// The environment variable that turns certificate issuance on.
pub const ISSUANCE_ENV: &str = "VEREDICTUM_CERTIFICATES";

/// The one value of [`ISSUANCE_ENV`] that turns issuance on.
pub const ISSUE: &str = "issue";

/// The environment variable naming the Ed25519 private key file (PKCS#8 PEM),
/// read only when issuance is on.
pub const KEY_FILE_ENV: &str = "VEREDICTUM_SIGNING_KEY_FILE";

/// The environment variable naming the key id the key is published under,
/// read only when issuance is on.
pub const KID_ENV: &str = "VEREDICTUM_SIGNING_KID";

/// Where the console serves the JWK Set certificates verify against: the
/// well-known location RFC 8615 reserves, under the name OpenID Connect
/// Discovery and the SMART App Launch both use for a key set.
pub const JWKS_ROUTE: &str = "/.well-known/jwks.json";

/// The route a run's certificate downloads from; the run id follows.
pub const CERTIFICATE_ROUTE: &str = "/run/certificate";

/// Whether this console issues certificates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssuanceMode {
    /// Trial mode: no certificate is created or signed.
    Trial,
    /// Every finished run gets a certificate.
    Issuing,
}

impl IssuanceMode {
    /// The sentence every page carries in this mode, when it carries one.
    #[must_use]
    pub const fn notice(self) -> Option<&'static str> {
        match self {
            Self::Trial => Some(
                "Trial mode: runs, results and verdicts are complete, and no certificate is issued.",
            ),
            Self::Issuing => None,
        }
    }
}

/// What happened about a finished run's certificate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CertificateStatus {
    /// Trial mode: none was created.
    Trial,
    /// It was issued and written beside the results.
    Issued,
    /// Issuance is on and this run got none; the reason, verbatim.
    NotIssued(String),
}

/// The download path of one run's certificate.
#[must_use]
pub fn download_href(run: &str) -> String {
    format!("{CERTIFICATE_ROUTE}/{run}")
}

#[cfg(feature = "ssr")]
pub mod ssr {
    //! The server half: reading the setting, loading the key, issuing, and the
    //! two routes.

    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use veredictum::certificate::{
        CERTIFICATE_FILE, CertificatePayload, Jwks, JwksError, KeyError, Kid, KidError,
        RecordedRun, SigningKey, issue,
    };
    use veredictum::party::Sut;
    use veredictum::pipeline::arazzo_run::{RECORDING_FILE, RESULTS_FILE};

    use super::{CertificateStatus, ISSUANCE_ENV, ISSUE, IssuanceMode, KEY_FILE_ENV, KID_ENV};

    /// The JWK Set this console publishes: the copy committed in this
    /// repository, compiled in, so the served bytes are the committed bytes.
    pub const PUBLISHED_JWKS: &str = include_str!("../../../deploy/hosted/jwks.json");

    /// The media type of a JWK Set (RFC 7517 §8.5.1).
    pub const JWKS_MEDIA_TYPE: &str = "application/jwk-set+json";

    /// The media type of a JWS in the compact serialization (RFC 7515 §9.2.1).
    pub const JWS_MEDIA_TYPE: &str = "application/jose";

    /// What [`ISSUANCE_ENV`] says.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Setting {
        /// Trial mode, as set or as unset.
        Trial,
        /// Trial mode, because the value names nothing; the value, verbatim,
        /// so startup can say it was not understood.
        Unrecognised(String),
        /// Issue certificates.
        Issue,
    }

    impl Setting {
        /// Reads the setting's value; `None` is unset.
        #[must_use]
        pub fn parse(value: Option<&str>) -> Self {
            match value.map(str::trim) {
                None | Some("" | "trial") => Self::Trial,
                Some(ISSUE) => Self::Issue,
                Some(other) => Self::Unrecognised(other.to_owned()),
            }
        }

        /// The mode the setting puts the console in.
        #[must_use]
        pub fn mode(&self) -> IssuanceMode {
            match self {
                Self::Trial | Self::Unrecognised(_) => IssuanceMode::Trial,
                Self::Issue => IssuanceMode::Issuing,
            }
        }
    }

    /// Why the console refuses to start with issuance on.
    ///
    /// No variant carries key material.
    #[derive(Debug, thiserror::Error)]
    pub enum IssuanceError {
        /// Issuance is on and a required variable is unset.
        #[error("{ISSUANCE_ENV}={ISSUE} needs {missing} as well")]
        Missing {
            /// The unset variable.
            missing: &'static str,
        },
        /// The key id is not a key id.
        #[error(transparent)]
        Kid(#[from] KidError),
        /// The key file could not be read.
        #[error("the signing key file {path} cannot be read: {source}")]
        KeyFile {
            /// The file.
            path: PathBuf,
            /// The filesystem's own error.
            #[source]
            source: std::io::Error,
        },
        /// The key is unusable, or the published set does not publish it.
        #[error(transparent)]
        Key(#[from] KeyError),
        /// The compiled-in published set does not read.
        #[error(transparent)]
        Jwks(#[from] JwksError),
    }

    /// The issuer a console with issuance on holds.
    #[derive(Debug)]
    pub struct Issuer {
        key: SigningKey,
        catalogue: PathBuf,
    }

    /// Whether this console issues, and with what.
    #[derive(Debug, Clone, Default)]
    pub enum Issuance {
        /// Trial mode: nothing is issued and no key is held.
        #[default]
        Trial,
        /// Issuing with this key, over this catalogue.
        Issuing(Arc<Issuer>),
    }

    impl Issuance {
        /// Builds the issuance a setting asks for.
        ///
        /// In trial mode nothing else is read: `key_file` is never opened,
        /// whatever it names. With issuance on, the key is read, its id
        /// parsed, and the key checked against `published` at `now`.
        ///
        /// # Errors
        /// [`IssuanceError`] with issuance on: an unset key file or key id, an
        /// unreadable or unusable key, or a key the published set does not
        /// publish, revokes or has let expire.
        pub fn load(
            setting: &Setting,
            key_file: Option<&Path>,
            kid: Option<&str>,
            published: &Jwks,
            catalogue: &Path,
            now: jiff::Timestamp,
        ) -> Result<Self, IssuanceError> {
            if setting.mode() == IssuanceMode::Trial {
                return Ok(Self::Trial);
            }
            let key_file = key_file.ok_or(IssuanceError::Missing {
                missing: KEY_FILE_ENV,
            })?;
            let kid = Kid::parse(kid.ok_or(IssuanceError::Missing { missing: KID_ENV })?)?;
            let pem = std::fs::read(key_file).map_err(|source| IssuanceError::KeyFile {
                path: key_file.to_path_buf(),
                source,
            })?;
            let key = SigningKey::from_pkcs8_pem(kid, &pem)?;
            key.check_published(published, now)?;
            Ok(Self::Issuing(Arc::new(Issuer {
                key,
                catalogue: catalogue.to_path_buf(),
            })))
        }

        /// Reads the setting and, with issuance on, the key from the
        /// environment, checked against [`PUBLISHED_JWKS`].
        ///
        /// # Errors
        /// [`Self::load`]'s, and [`IssuanceError::Jwks`] when the published
        /// set does not read.
        pub fn from_env(catalogue: &Path) -> Result<(Self, Setting), IssuanceError> {
            let setting = Setting::parse(std::env::var(ISSUANCE_ENV).ok().as_deref());
            let key_file = std::env::var_os(KEY_FILE_ENV).map(PathBuf::from);
            let kid = std::env::var(KID_ENV).ok();
            let issuance = Self::load(
                &setting,
                key_file.as_deref(),
                kid.as_deref(),
                &Jwks::parse(PUBLISHED_JWKS)?,
                catalogue,
                jiff::Timestamp::now(),
            )?;
            Ok((issuance, setting))
        }

        /// The mode this issuance is in.
        #[must_use]
        pub fn mode(&self) -> IssuanceMode {
            match self {
                Self::Trial => IssuanceMode::Trial,
                Self::Issuing(_) => IssuanceMode::Issuing,
            }
        }

        /// Certifies the finished run in `run_dir`: in trial mode nothing is
        /// read or written; with issuance on, the certificate is issued over
        /// the run's results and recording and written beside them.
        #[must_use]
        pub fn certify(&self, run_dir: &Path, sut: &Sut) -> CertificateStatus {
            let Self::Issuing(issuer) = self else {
                return CertificateStatus::Trial;
            };
            match issuer.certify(run_dir, sut) {
                Ok(()) => CertificateStatus::Issued,
                Err(reason) => CertificateStatus::NotIssued(reason),
            }
        }
    }

    impl Issuer {
        /// Issues and writes the certificate, or says why not.
        fn certify(&self, run_dir: &Path, sut: &Sut) -> Result<(), String> {
            let read = |name: &str| {
                let path = run_dir.join(name);
                std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
            };
            let results = read(RESULTS_FILE)?;
            let recording = read(RECORDING_FILE)?;
            let payload = CertificatePayload::of_run(&RecordedRun {
                results: &results,
                recording: &recording,
                sut: sut.clone(),
                catalogue: &self.catalogue,
                issued_at: jiff::Timestamp::now(),
            })
            .map_err(|e| e.to_string())?;
            let certificate = issue(&payload, &self.key).map_err(|e| e.to_string())?;
            let path = run_dir.join(CERTIFICATE_FILE);
            std::fs::write(&path, certificate.to_text())
                .map_err(|e| format!("{}: {e}", path.display()))
        }
    }

    /// The published JWK Set, byte for byte as committed.
    pub async fn jwks() -> impl axum::response::IntoResponse {
        (
            [(axum::http::header::CONTENT_TYPE, JWKS_MEDIA_TYPE)],
            PUBLISHED_JWKS,
        )
    }

    /// One run's certificate as a download, or 404 when the run has none.
    ///
    /// The run id is parsed before any path is built, so the read stays under
    /// the mounted output tree.
    #[must_use]
    pub fn download(out: &Path, run: &str) -> axum::response::Response {
        use axum::http::{StatusCode, header};
        use axum::response::IntoResponse as _;

        let Ok(id) = run.parse::<crate::run_job::RunId>() else {
            return (StatusCode::NOT_FOUND, "not a run id").into_response();
        };
        let path = crate::run_job::job_dir(out, id).join(CERTIFICATE_FILE);
        match std::fs::read(&path) {
            Ok(body) => (
                [
                    (header::CONTENT_TYPE, JWS_MEDIA_TYPE),
                    (
                        header::CONTENT_DISPOSITION,
                        "attachment; filename=\"certificate.jws\"",
                    ),
                ],
                body,
            )
                .into_response(),
            Err(_absent) => (StatusCode::NOT_FOUND, "this run has no certificate").into_response(),
        }
    }
}

#[cfg(all(test, feature = "ssr"))]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::IssuanceMode;
    use super::ssr::{Issuance, IssuanceError, Setting};

    fn repo() -> PathBuf {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).to_path_buf()
    }

    fn now() -> Result<jiff::Timestamp, jiff::Error> {
        "2026-10-29T12:00:00Z".parse()
    }

    fn published() -> Result<veredictum::certificate::Jwks, veredictum::certificate::JwksError> {
        veredictum::certificate::Jwks::parse(super::ssr::PUBLISHED_JWKS)
    }

    fn test_jwks() -> Result<veredictum::certificate::Jwks, Box<dyn std::error::Error>> {
        Ok(veredictum::certificate::Jwks::parse(
            &std::fs::read_to_string(repo().join("fixtures/certificates/jwks.json"))?,
        )?)
    }

    /// With issuance on and the test key published, a finished run gets a
    /// certificate beside its results that verifies against the set.
    #[test]
    fn issuance_on_writes_a_certificate_that_verifies() -> Result<(), Box<dyn std::error::Error>> {
        let catalogue = assert_fs::TempDir::new()?;
        std::fs::write(catalogue.path().join("capabilities.yaml"), b"options: []\n")?;
        let run = assert_fs::TempDir::new()?;
        let results = format!(
            r#"{{ "document": "catalogue", "engine": {{ "name": "veredictum", "version": "{}" }}, "instrument_status": "{}", "base_url": "http://sut.example/openehr/v1", "rows": [] }}"#,
            crate::ENGINE_PIN,
            veredictum::party::InstrumentStatus::CURRENT.token()
        );
        std::fs::write(run.path().join("results.json"), &results)?;
        std::fs::write(run.path().join("recording.har"), b"{}")?;
        let issuance = Issuance::load(
            &Setting::Issue,
            Some(&repo().join("fixtures/certificates/test-signing.key.pem")),
            Some("veredictum-test"),
            &test_jwks()?,
            catalogue.path(),
            now()?,
        )?;
        assert_eq!(issuance.mode(), IssuanceMode::Issuing);
        let sut = veredictum::party::Sut::operator("ExampleCDR", "1");
        assert_eq!(
            issuance.certify(run.path(), &sut),
            super::CertificateStatus::Issued
        );
        let written =
            std::fs::read_to_string(run.path().join(veredictum::certificate::CERTIFICATE_FILE))?;
        let verified = veredictum::certificate::verify(&written, &test_jwks()?)?;
        assert_eq!(verified.payload.sut, sut);
        assert_eq!(
            verified.payload.exchanges.results,
            veredictum::certificate::Sha256Digest::of(results.as_bytes())
        );
        Ok(())
    }

    /// A run issuance cannot certify says why, and writes nothing.
    #[test]
    fn a_run_without_its_recording_is_not_certified() -> Result<(), Box<dyn std::error::Error>> {
        let catalogue = assert_fs::TempDir::new()?;
        let run = assert_fs::TempDir::new()?;
        let issuance = Issuance::load(
            &Setting::Issue,
            Some(&repo().join("fixtures/certificates/test-signing.key.pem")),
            Some("veredictum-test"),
            &test_jwks()?,
            catalogue.path(),
            now()?,
        )?;
        let status = issuance.certify(run.path(), &veredictum::party::Sut::operator("a", "1"));
        assert!(
            matches!(status, super::CertificateStatus::NotIssued(_)),
            "{status:?}"
        );
        assert_eq!(std::fs::read_dir(run.path())?.count(), 0);
        Ok(())
    }

    #[test]
    fn only_the_exact_value_issue_turns_issuance_on() {
        assert_eq!(Setting::parse(None).mode(), IssuanceMode::Trial);
        assert_eq!(Setting::parse(Some("trial")).mode(), IssuanceMode::Trial);
        assert_eq!(Setting::parse(Some("")).mode(), IssuanceMode::Trial);
        assert_eq!(
            Setting::parse(Some(" issue ")).mode(),
            IssuanceMode::Issuing
        );
        for typo in ["Issue", "issued", "on", "true", "1", "yes"] {
            assert_eq!(
                Setting::parse(Some(typo)),
                Setting::Unrecognised(typo.to_owned()),
                "{typo}"
            );
            assert_eq!(Setting::parse(Some(typo)).mode(), IssuanceMode::Trial);
        }
    }

    /// The key file named here does not exist, so a trial load that opened it
    /// would fail: an `Ok` is the proof that no key is read.
    #[test]
    fn trial_mode_reads_no_key() -> Result<(), Box<dyn std::error::Error>> {
        let absent = repo().join("fixtures/certificates/no-such-key.pem");
        for setting in [Setting::Trial, Setting::Unrecognised("on".to_owned())] {
            let issuance = Issuance::load(
                &setting,
                Some(&absent),
                Some("veredictum-test"),
                &published()?,
                &repo().join("catalogue"),
                now()?,
            )?;
            assert_eq!(issuance.mode(), IssuanceMode::Trial);
        }
        Ok(())
    }

    /// A finished run in trial mode gets nothing written and nothing read.
    #[test]
    fn trial_mode_creates_no_certificate() -> Result<(), Box<dyn std::error::Error>> {
        let dir = assert_fs::TempDir::new()?;
        let status = Issuance::Trial.certify(
            dir.path(),
            &veredictum::party::Sut::operator("ExampleCDR", "1"),
        );
        assert_eq!(status, super::CertificateStatus::Trial);
        assert_eq!(std::fs::read_dir(dir.path())?.count(), 0);
        Ok(())
    }

    #[test]
    fn issuance_on_refuses_to_start_without_a_published_key()
    -> Result<(), Box<dyn std::error::Error>> {
        let catalogue = repo().join("catalogue");
        let missing = Issuance::load(
            &Setting::Issue,
            None,
            Some("k"),
            &published()?,
            &catalogue,
            now()?,
        );
        assert!(matches!(missing, Err(IssuanceError::Missing { .. })));
        let unreadable = Issuance::load(
            &Setting::Issue,
            Some(&repo().join("fixtures/certificates/no-such-key.pem")),
            Some("veredictum-test"),
            &published()?,
            &catalogue,
            now()?,
        );
        assert!(matches!(unreadable, Err(IssuanceError::KeyFile { .. })));
        // The test key is a fixture: the published set never carries it.
        let unpublished = Issuance::load(
            &Setting::Issue,
            Some(&repo().join("fixtures/certificates/test-signing.key.pem")),
            Some("veredictum-test"),
            &published()?,
            &catalogue,
            now()?,
        );
        assert!(
            matches!(
                unpublished,
                Err(IssuanceError::Key(
                    veredictum::certificate::KeyError::NotPublished { .. }
                ))
            ),
            "{unpublished:?}"
        );
        Ok(())
    }

    #[test]
    fn the_served_set_is_the_committed_set_byte_for_byte() -> Result<(), std::io::Error> {
        let committed = std::fs::read_to_string(repo().join("deploy/hosted/jwks.json"))?;
        assert_eq!(super::ssr::PUBLISHED_JWKS, committed);
        Ok(())
    }
}
