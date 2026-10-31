// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The console's server state: the mounted roots, and the catalogue loaded
//! through the published lib ONCE at startup.
//!
//! The console stores nothing of its own: this
//! state is a read of the mounts, and a restart re-reads them. A missing or
//! unreadable catalogue is a FIRST-CLASS state the screens render as the
//! named-mount explanation — the server still serves.

use std::path::PathBuf;
use std::sync::Arc;

/// The environment variable naming the catalogue root (default `catalogue`;
/// the image sets it to the documented `/work` mount).
pub const ROOT_ENV: &str = "VEREDICTUM_ROOT";

/// The environment variable naming the vendored spec tree (default
/// `specs/openehr`; the image sets it to the documented `/work` mount).
pub const SPECS_ENV: &str = "VEREDICTUM_SPECS";

/// The environment variable naming the run-output tree (default `out`; the
/// image sets it to the documented `/work` mount). Runs write here exactly
/// as a terminal run would.
pub const OUT_ENV: &str = "VEREDICTUM_OUT";

/// The environment variable stating how many runs this host can drive at once.
///
/// Unset is [`crate::run_job::MAX_CONCURRENT_RUNS`], which #388 reasoned about
/// and called a starting value to re-derive by measuring on the chosen host.
/// This is how a host states what it actually has: a 2 GB box drives one run,
/// not two, and the queue covers the difference with a position rather than a
/// refusal.
///
/// A value that is not a positive integer refuses to start. A cap is a safety
/// property, and falling back to a larger default on a typo would let a box
/// admit work it cannot hold — which the OOM killer then resolves, halfway
/// through somebody's conformance run.
pub const MAX_CONCURRENT_RUNS_ENV: &str = "VEREDICTUM_MAX_CONCURRENT_RUNS";

/// The environment variable naming the request header that carries the real
/// client address, for a deployment behind a proxy.
///
/// Unset is the default and the safe state: the socket peer is then the whole
/// answer, and no forwarded header is read at all. A header is trusted only
/// because the operator named it (the hosted deployment sets `Fly-Client-IP`),
/// since an unconditionally trusted `X-Forwarded-For` would let any visitor
/// claim any identity and defeat every per-submitter cap.
pub const CLIENT_IP_HEADER_ENV: &str = "VEREDICTUM_CLIENT_IP_HEADER";

/// Why the console refused to start.
///
/// Three values that this console will not guess at. Everything else missing
/// is a first-class state a screen explains; these are safety properties, and
/// a wrong guess about any of them lets the instrument do something it should
/// not.
#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    /// The posture is a value that names no posture.
    #[error(transparent)]
    Posture(#[from] crate::posture::UnknownPosture),
    /// The concurrency cap is not a positive integer.
    #[error(
        "{env}={value:?} is not a positive integer, and a concurrency cap is not a value to guess at: a box that admits more runs than it can hold has the OOM killer resolve the difference, halfway through somebody's conformance run"
    )]
    Concurrency {
        /// The variable that carried it.
        env: &'static str,
        /// The value, verbatim.
        value: String,
    },
    /// Issuance is on and the signing key cannot issue: unset, unreadable,
    /// or not published, revoked or expired in the published set.
    #[error(transparent)]
    Certificates(#[from] crate::certificates::ssr::IssuanceError),
    /// The metrics token is set and is not a usable token.
    #[error(transparent)]
    MetricsToken(#[from] crate::metrics::TokenRefusal),
}

/// The loaded catalogue, or the explanation of why there is none.
#[derive(Debug, Clone)]
pub struct ConsoleState {
    /// The catalogue root the console reads and runs.
    pub root: PathBuf,
    /// The vendored spec tree citations resolve against.
    pub specs: PathBuf,
    /// The run-output tree job artifacts land under.
    pub out: PathBuf,
    /// The one startup read of the catalogue, its gate findings beside its
    /// listing, shared by every request; `Err` is the verbatim reason the
    /// catalogue could not be opened.
    pub catalogue: Arc<Result<veredictum::pipeline::listing::Listing, String>>,
    /// The in-flight run drafts (the wizard's server-side memory), one per
    /// submitter (#389): two visitors composing a connection at once do not
    /// overwrite each other. A restart legitimately forgets every draft — no
    /// console-local store exists — and the map is capped and evicted
    /// oldest-first like the job map beside it.
    pub draft: Arc<std::sync::Mutex<crate::run_api::Drafts>>,
    /// The run-job map (#66, #389): every run this process is driving or has
    /// recently driven, addressed by its own id.
    pub jobs: crate::run_job::JobSlot,
    /// The request header the operator asked to be trusted for the client
    /// address ([`CLIENT_IP_HEADER_ENV`]), when they named one.
    pub client_ip_header: Option<String>,
    /// Whose network this instance sits in (#390), read once from
    /// [`crate::posture::POSTURE_ENV`]. The hosted posture refuses a target
    /// only this instance can reach; the local one refuses nothing.
    pub posture: crate::posture::Posture,
    /// The per-submitter rate ledger (#390) the probe and start seams ask
    /// before spending anything, over the same submitter identity the
    /// concurrency caps read.
    pub rates: crate::rate_limit::RateLimiter,
    /// Whether the documentation capture mode is on
    /// ([`crate::capture::CAPTURE_ENV`]): the facts a run stamps then render
    /// as fixed stand-ins. It changes what the surfaces DISPLAY and nothing
    /// that is written, sealed or signed.
    pub capture: bool,
    /// The operator's signal (#489): the counters the job map and the request
    /// seams report into, and the token that opens them
    /// ([`crate::metrics::TOKEN_ENV`]).
    pub metrics: crate::metrics::Metrics,
}

impl ConsoleState {
    /// The startup listing of the catalogue.
    ///
    /// # Errors
    /// [`ConsoleError::CatalogueUnavailable`](crate::error::ConsoleError::CatalogueUnavailable)
    /// carrying the verbatim reason the catalogue could not be opened.
    pub fn listing(
        &self,
    ) -> Result<&veredictum::pipeline::listing::Listing, crate::error::ConsoleError> {
        self.catalogue.as_ref().as_ref().map_err(|reason| {
            crate::error::ConsoleError::CatalogueUnavailable {
                detail: reason.clone(),
            }
        })
    }

    /// Reads the mounts from the environment and reads the catalogue once,
    /// through the published lib — the same gates `validate` runs.
    ///
    /// # Errors
    /// [`StartupError`]: a posture that names no posture, a concurrency cap
    /// that is not a positive integer, certificate issuance switched on
    /// with a key that cannot issue, or a metrics token that is set and
    /// unusable. Those are the startup values this console
    /// refuses to guess at — a missing mount is a first-class state the
    /// screens explain, while a wrong guess about one of these lets the
    /// instrument drive an address it should not, admit work it cannot hold,
    /// or sign with a key nobody can verify.
    ///
    /// In trial mode, the default, no signing key is read.
    pub fn load() -> Result<Self, StartupError> {
        Ok(Self::load_with_setting()?.0)
    }

    /// [`Self::load`], with the issuance setting as read, so startup can say
    /// when a value was not understood and trial mode was kept.
    ///
    /// # Errors
    /// [`Self::load`]'s.
    pub fn load_with_setting() -> Result<(Self, crate::certificates::ssr::Setting), StartupError> {
        let posture = crate::posture::from_env()?;
        // What this host can actually drive at once. #388 reasoned two, and
        // called it a starting value to re-derive by measuring on the chosen
        // host; this is where the host says what it measured.
        let max_concurrent = match std::env::var(MAX_CONCURRENT_RUNS_ENV) {
            Err(_) => crate::run_job::MAX_CONCURRENT_RUNS,
            Ok(value) => match value.trim().parse::<usize>() {
                Ok(n) if n > 0 => n,
                _ => {
                    return Err(StartupError::Concurrency {
                        env: MAX_CONCURRENT_RUNS_ENV,
                        value,
                    });
                }
            },
        };
        let root =
            PathBuf::from(std::env::var(ROOT_ENV).unwrap_or_else(|_| String::from("catalogue")));
        let specs = PathBuf::from(
            std::env::var(SPECS_ENV).unwrap_or_else(|_| String::from("specs/openehr")),
        );
        let out = PathBuf::from(std::env::var(OUT_ENV).unwrap_or_else(|_| String::from("out")));
        let (issuance, setting) = crate::certificates::ssr::Issuance::from_env(&root)?;
        let metrics = crate::metrics::Metrics::default()
            .with_access(crate::metrics::AccessToken::from_env()?);
        let catalogue =
            veredictum::pipeline::listing::read(&root, Some(&specs)).map_err(|e| e.to_string());
        let state = Self {
            root,
            specs,
            out,
            catalogue: Arc::new(catalogue),
            draft: Arc::new(std::sync::Mutex::new(crate::run_api::Drafts::new())),
            jobs: crate::run_job::JobSlot::with_limits(crate::run_job::Limits {
                max_concurrent,
                ..crate::run_job::Limits::default()
            })
            .with_issuance(issuance)
            .with_metrics(metrics.clone()),
            client_ip_header: std::env::var(CLIENT_IP_HEADER_ENV)
                .ok()
                .filter(|name| !name.trim().is_empty()),
            posture,
            rates: crate::rate_limit::RateLimiter::default(),
            capture: crate::capture::enabled(),
            metrics,
        };
        Ok((state, setting))
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_CONCURRENT_RUNS_ENV, StartupError};

    /// The cap a host may state, and the values it may not.
    ///
    /// Driven over the parse itself rather than through `load()`: that reads
    /// process-wide environment, and a test that sets a variable races every
    /// other test in the binary.
    fn cap_of(value: Option<&str>) -> Result<usize, StartupError> {
        match value {
            None => Ok(crate::run_job::MAX_CONCURRENT_RUNS),
            Some(value) => match value.trim().parse::<usize>() {
                Ok(n) if n > 0 => Ok(n),
                _ => Err(StartupError::Concurrency {
                    env: MAX_CONCURRENT_RUNS_ENV,
                    value: value.to_owned(),
                }),
            },
        }
    }

    #[test]
    fn an_unset_cap_is_the_reasoned_default() {
        assert_eq!(cap_of(None).ok(), Some(crate::run_job::MAX_CONCURRENT_RUNS));
    }

    #[test]
    fn a_host_states_what_it_can_drive() {
        assert_eq!(cap_of(Some("1")).ok(), Some(1));
        assert_eq!(cap_of(Some(" 4 ")).ok(), Some(4));
    }

    /// Every one of these would otherwise fall back to a LARGER cap than the
    /// host stated, which is the direction that ends a run halfway through.
    #[test]
    fn a_cap_that_is_not_a_positive_integer_refuses_to_start() {
        for value in ["", "0", "two", "-1", "1.5", "1 2"] {
            let refused = cap_of(Some(value));
            assert!(
                matches!(refused, Err(StartupError::Concurrency { .. })),
                "{value:?} must refuse rather than fall back"
            );
        }
    }
}
