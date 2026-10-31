// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The principal a measured run presents: the IXIT's default instance, its
//! base URL, the headers it adds and its credential, resolved once.
//!
//! A credential is read from the environment variables the instance names,
//! or minted from the IXIT's SMART lane for a `bearer_mint` instance, and
//! never from an argument. The asynchronous client of the run reads the
//! `Authorization` value from here per request, so a minted grant is
//! re-minted only near its expiry.

use std::sync::{Arc, RwLock};

use base64::Engine;

use crate::ixit::{AuthMode, BearerMint, Environment, Instance, Ixit};

/// Re-mint a lane token this many milliseconds before it expires, so no
/// arrival can ever present one that lapses in flight.
const MINT_REFRESH_MARGIN_MS: i64 = 30_000;

/// One minted lane token and the instant it stops being presentable.
#[derive(Debug)]
struct MintedToken {
    header: String,
    expires_at_ms: i64,
}

/// The standing SMART grant a `bearer_mint` principal holds for the whole
/// measured window: the ixit lane's static test issuer plus THIS
/// principal's subject/roles/`default_scopes` (ITS-REST
/// `docs/smart_app_launch/master08-scopes.adoc` §Resource Scopes). A
/// measured run declares no per-step scopes — there are no steps — so the
/// standing grant IS the scope claim, minted once and re-minted only when
/// the declared `ttl_seconds` is about to lapse (never per arrival: the
/// instrument measures the CDR, not token signing).
#[derive(Debug)]
struct MintedGrant {
    mint: BearerMint,
    subject: Option<String>,
    roles: Option<Vec<String>>,
    scopes: Vec<String>,
    current: RwLock<Arc<MintedToken>>,
}

/// How a client presents itself: a fixed header (none / Basic / a Bearer
/// token from the environment) or a standing minted SMART grant.
#[derive(Debug)]
enum Credential {
    Fixed(Option<String>),
    // Boxed: the grant carries the whole mint declaration, and a fixed
    // header is the common case.
    Minted(Box<MintedGrant>),
}

/// The principal of a measured run (see the module doc).
#[derive(Clone)]
pub struct PerfClient {
    base_url: String,
    credential: Arc<Credential>,
    extra_headers: Vec<(String, String)>,
}

impl std::fmt::Debug for PerfClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PerfClient")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl PerfClient {
    /// Build the client from an ixit instance (credentials resolved from
    /// the referenced environment variables, exactly like the functional
    /// driver).
    ///
    /// # Errors
    /// A message when a credential env var is unset, a `bearer_mint`
    /// instance has no declared SMART lane to mint against, or the client
    /// cannot be built.
    pub fn from_instance(instance: &Instance, ixit: &Ixit) -> Result<Self, String> {
        let credential = match &instance.auth {
            AuthMode::None => Credential::Fixed(None),
            AuthMode::Basic {
                user_env,
                password_env,
            } => {
                let user = std::env::var(user_env)
                    .map_err(|error| format!("credential env {user_env}: {error}"))?;
                let pass = std::env::var(password_env)
                    .map_err(|error| format!("credential env {password_env}: {error}"))?;
                let token = base64::engine::general_purpose::STANDARD
                    .encode(format!("{user}:{pass}").as_bytes());
                Credential::Fixed(Some(format!("Basic {token}")))
            }
            AuthMode::Bearer { token_env } => {
                let token = std::env::var(token_env)
                    .map_err(|error| format!("credential env {token_env}: {error}"))?;
                Credential::Fixed(Some(format!("Bearer {token}")))
            }
            // The SMART resource-server posture IS this product's standard
            // conformance posture, so the measured lane drives it too: the
            // principal's STANDING grant (`default_scopes`) is minted once
            // against the ixit's declared static test issuer and re-minted
            // only near expiry. Every measured arrival therefore rides a
            // scope-limited Bearer token, which is what the SMART scope
            // grammar is enforced against under load.
            AuthMode::BearerMint {
                subject,
                roles,
                default_scopes,
            } => {
                let lane = ixit.smart.as_ref().ok_or_else(|| {
                    "instance declares auth mode `bearer_mint` but the ixit declares no `smart` \
                     lane to mint against"
                        .to_owned()
                })?;
                let grant = MintedGrant {
                    mint: lane.mint.clone(),
                    subject: subject.clone(),
                    roles: roles.clone(),
                    scopes: default_scopes.clone(),
                    current: RwLock::new(Arc::new(MintedToken {
                        header: String::new(),
                        expires_at_ms: i64::MIN,
                    })),
                };
                // Mint eagerly so a credential defect fails the run before
                // any window opens, never as error arrivals inside one.
                let token = grant.freshly_minted()?;
                grant
                    .current
                    .write()
                    .map_err(|error| format!("minted-token lock poisoned: {error}"))?
                    .clone_from(&token);
                Credential::Minted(Box::new(grant))
            }
        };
        Ok(Self {
            base_url: instance.base_url.trim_end_matches('/').to_owned(),
            credential: Arc::new(credential),
            extra_headers: instance.headers.clone().unwrap_or_default(),
        })
    }

    /// Builds the client of a target whose credential is held in process:
    /// a stack this instrument composed itself, never the operator's.
    ///
    #[must_use]
    pub fn fixed(base_url: &str, authorization: Option<String>) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            credential: Arc::new(Credential::Fixed(authorization)),
            extra_headers: Vec::new(),
        }
    }

    /// The base URL every path is joined to.
    pub(crate) fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The headers the IXIT instance adds to every request.
    pub(crate) fn extra_headers(&self) -> &[(String, String)] {
        &self.extra_headers
    }

    /// The `Authorization` header this arrival presents.
    pub(crate) fn authorization(&self) -> Result<Option<String>, String> {
        match self.credential.as_ref() {
            Credential::Fixed(header) => Ok(header.clone()),
            Credential::Minted(grant) => grant.presentable().map(Some),
        }
    }
}

impl MintedGrant {
    /// Sign one token for this principal's standing grant.
    fn freshly_minted(&self) -> Result<Arc<MintedToken>, String> {
        let token = crate::exec::auth::mint_access_token(
            &self.mint,
            self.subject.as_deref(),
            self.roles.as_deref(),
            &self.scopes,
        )?;
        let ttl_ms = i64::try_from(self.mint.ttl_seconds.saturating_mul(1_000)).unwrap_or(i64::MAX);
        Ok(Arc::new(MintedToken {
            header: format!("Bearer {token}"),
            expires_at_ms: crate::exec::auth::now_ms().saturating_add(ttl_ms),
        }))
    }

    /// The token to present now, re-minting once when the standing one is
    /// within the refresh margin of its declared expiry.
    fn presentable(&self) -> Result<String, String> {
        let now = crate::exec::auth::now_ms();
        {
            let current = self
                .current
                .read()
                .map_err(|error| format!("minted-token lock poisoned: {error}"))?;
            if now.saturating_add(MINT_REFRESH_MARGIN_MS) < current.expires_at_ms {
                return Ok(current.header.clone());
            }
        }
        let mut current = self
            .current
            .write()
            .map_err(|error| format!("minted-token lock poisoned: {error}"))?;
        // Re-check: another arrival may have re-minted while this one
        // waited for the write lock.
        if now.saturating_add(MINT_REFRESH_MARGIN_MS) < current.expires_at_ms {
            return Ok(current.header.clone());
        }
        let fresh = self.freshly_minted()?;
        let header = fresh.header.clone();
        *current = fresh;
        Ok(header)
    }
}

/// The principal and the deployment a measured run needs from the IXIT: the
/// default instance's client and the environment block.
///
/// # Errors
/// A message naming the missing piece: no default instance, an unusable
/// credential, the absent environment block, or the member it lacks.
pub fn measured_run_context(ixit: &Ixit) -> Result<(PerfClient, Environment), String> {
    let primary = PerfClient::from_instance(ixit.default_instance()?, ixit)?;
    let environment = ixit
        .measured_environment()
        .map_err(|error| error.to_string())?;
    Ok((primary, environment))
}

#[cfg(test)]
#[expect(
    clippy::disallowed_types,
    reason = "the ixit fixtures are authored as wire JSON, the shape the loader reads"
)]
mod tests {
    use super::*;

    /// The SMART posture's standing grant is minted ONCE and cached: a
    /// measured run must never pay token-signing cost per arrival, and every
    /// arrival must present a scope-limited Bearer token.
    #[test]
    fn a_bearer_mint_principal_presents_one_cached_standing_grant() {
        let key = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
            .join("catalogue/keys/smart-test-issuer/cnf-smart-test.key.pem");
        assert!(key.is_file(), "committed test issuer key is missing");
        let ixit: Ixit = serde_json::from_value(serde_json::json!({
            "instances": { "sut": { "base_url": "http://stub", "auth": {
                "mode": "bearer_mint", "subject": "cnf-user", "roles": ["USER"],
                "default_scopes": ["user/aql-*.r"] } } },
            "smart": { "platform_instance": "sut", "mint": {
                "issuer": "https://as.cnf.test", "subject": "cnf-smart-app",
                "roles": ["USER"], "key_file": key, "kid": "cnf-smart-test",
                "ttl_seconds": 3600 } }
        }))
        .unwrap();
        let client = PerfClient::from_instance(ixit.default_instance().unwrap(), &ixit).unwrap();
        let first = client.authorization().unwrap().unwrap();
        let second = client.authorization().unwrap().unwrap();
        assert!(first.starts_with("Bearer "));
        assert_eq!(first.split('.').count(), 3, "not a JWS compact token");
        assert_eq!(first, second, "the standing grant was re-minted per call");
    }

    /// A `bearer_mint` principal with no declared SMART lane is an authoring
    /// defect the run refuses before any window opens.
    #[test]
    fn a_bearer_mint_principal_without_a_lane_is_refused() {
        let ixit: Ixit = serde_json::from_value(serde_json::json!({
            "instances": { "sut": { "base_url": "http://stub",
                                     "auth": { "mode": "bearer_mint" } } }
        }))
        .unwrap();
        let error = PerfClient::from_instance(ixit.default_instance().unwrap(), &ixit).unwrap_err();
        assert!(error.contains("no `smart` lane"), "{error}");
    }

    /// The credential env var every supported platform sets, so the two
    /// environment-resolved auth modes have a value to read without the
    /// test process mutating its own environment.
    const PRESENT_ENV: &str = "PATH";

    fn instance(auth: &serde_json::Value) -> Ixit {
        serde_json::from_value(serde_json::json!({
            "instances": { "sut": { "base_url": "http://stub/", "auth": auth } }
        }))
        .unwrap()
    }

    /// Basic resolves to the base64 of `user:password` read from the two
    /// named environment variables, and Bearer to the raw token from one.
    /// A run whose credential variable is unset fails at construction, not
    /// as error arrivals inside a window.
    #[test]
    fn the_environment_resolved_auth_modes_build_their_header_or_refuse() {
        let basic = instance(&serde_json::json!({
            "mode": "basic", "user_env": PRESENT_ENV, "password_env": PRESENT_ENV
        }));
        let client = PerfClient::from_instance(basic.default_instance().unwrap(), &basic).unwrap();
        let header = client.authorization().unwrap().unwrap();
        assert!(header.starts_with("Basic "), "{header}");
        let value = std::env::var(PRESENT_ENV).unwrap();
        assert_eq!(
            header,
            format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD
                    .encode(format!("{value}:{value}").as_bytes())
            )
        );

        let bearer = instance(&serde_json::json!({
            "mode": "bearer", "token_env": PRESENT_ENV
        }));
        let client =
            PerfClient::from_instance(bearer.default_instance().unwrap(), &bearer).unwrap();
        assert_eq!(
            client.authorization().unwrap().unwrap(),
            format!("Bearer {value}")
        );

        let unset = instance(&serde_json::json!({
            "mode": "bearer", "token_env": "VEREDICTUM_UNSET_CREDENTIAL_ENV"
        }));
        let error = PerfClient::from_instance(unset.default_instance().unwrap(), &unset)
            .expect_err("an unset credential variable is a run-stopping defect");
        assert!(error.contains("VEREDICTUM_UNSET_CREDENTIAL_ENV"), "{error}");

        // The base URL is normalized once, so no path is ever built with a
        // doubled separator.
        assert_eq!(client.base_url, "http://stub");
    }

    /// A standing grant inside its refresh margin is re-minted rather than
    /// presented: a token that lapses in flight would be an arrival the SUT
    /// refuses for the instrument's reason, never the server's.
    #[test]
    fn a_grant_inside_its_refresh_margin_is_re_minted_before_presentation() {
        let key = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
            .join("catalogue/keys/smart-test-issuer/cnf-smart-test.key.pem");
        let ixit: Ixit = serde_json::from_value(serde_json::json!({
            "instances": { "sut": { "base_url": "http://stub", "auth": {
                "mode": "bearer_mint", "subject": "cnf-user",
                "default_scopes": ["user/aql-*.r"] } } },
            "smart": { "platform_instance": "sut", "mint": {
                "issuer": "https://as.cnf.test", "subject": "cnf-smart-app",
                "key_file": key, "kid": "cnf-smart-test", "ttl_seconds": 1 } }
        }))
        .unwrap();
        let client = PerfClient::from_instance(ixit.default_instance().unwrap(), &ixit).unwrap();
        let header = client.authorization().unwrap().unwrap();
        assert!(header.starts_with("Bearer "), "{header}");
        assert_eq!(header.split('.').count(), 3, "not a JWS compact token");
    }

    /// The environment block is mandatory for a measured run, because a
    /// throughput number without the deployment it was measured in describes
    /// nothing.
    #[test]
    fn the_measured_run_context_requires_a_declared_environment() {
        let ixit: Ixit = serde_json::from_value(serde_json::json!({
            "instances": { "sut": { "base_url": "http://stub/", "auth": { "mode": "none" } } },
            "environment": { "hardware_class": "test-stub", "cores": 1, "memory_gb": 1,
                              "storage_class": "ram", "topology": "in-process stub" }
        }))
        .unwrap();
        let (principal, environment) = measured_run_context(&ixit).unwrap();
        assert_eq!(principal.base_url(), "http://stub");
        assert_eq!(environment.hardware_class, "test-stub");

        let bare: Ixit = serde_json::from_value(serde_json::json!({
            "instances": { "sut": { "base_url": "http://stub", "auth": { "mode": "none" } } }
        }))
        .unwrap();
        let error = measured_run_context(&bare).expect_err("a measured run needs its environment");
        assert!(error.contains("no environment block"), "{error}");
    }

    /// A conformance-only environment block, `exclusive_server` alone, is
    /// not enough for a measured run: the context names the first member it
    /// lacks.
    #[test]
    fn the_measured_run_context_names_the_environment_member_it_lacks() {
        let ixit: Ixit = serde_json::from_value(serde_json::json!({
            "instances": { "sut": { "base_url": "http://stub", "auth": { "mode": "none" } } },
            "environment": { "exclusive_server": true }
        }))
        .unwrap();
        let error = measured_run_context(&ixit).expect_err("a measured run needs hardware_class");
        assert!(error.contains("declares no `hardware_class`"), "{error}");

        let ixit: Ixit = serde_json::from_value(serde_json::json!({
            "instances": { "sut": { "base_url": "http://stub", "auth": { "mode": "none" } } },
            "environment": { "hardware_class": "test-stub", "cores": 1, "memory_gb": 1,
                              "storage_class": "ram" }
        }))
        .unwrap();
        let error = measured_run_context(&ixit).expect_err("a measured run needs topology");
        assert!(error.contains("declares no `topology`"), "{error}");
    }
}
