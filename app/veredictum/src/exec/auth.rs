// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `Authorization` header an IXIT instance presents, and the SMART
//! access token the instrument mints for a `bearer_mint` principal.
//!
//! The credentials are read from the environment variables the IXIT names,
//! never from the document itself.

#![expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT; a JWT claim set \
              is a JSON object"
)]

use base64::Engine as _;
use serde_json::Value;

use crate::ixit::{AuthMode, BearerMint, SmartLane};

/// Returns the `Authorization` header for an auth mode.
///
/// A `bearer_mint` mode mints its token against `smart`. `scopes` is the
/// SMART `scope` claim a request declares (`None` = it declared none), and is
/// consumed only by the `bearer_mint` principal.
///
/// # Errors
/// The unset credential variable by name, a `bearer_mint` mode with no lane,
/// or the mint's own failure.
pub fn lane_auth_header(
    smart: Option<&SmartLane>,
    auth: &AuthMode,
    scopes: Option<&[String]>,
) -> Result<Option<String>, String> {
    match auth {
        AuthMode::None => Ok(None),
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
            Ok(Some(format!("Basic {token}")))
        }
        AuthMode::Bearer { token_env } => {
            let token = std::env::var(token_env)
                .map_err(|error| format!("credential env {token_env}: {error}"))?;
            Ok(Some(format!("Bearer {token}")))
        }
        AuthMode::BearerMint {
            subject,
            roles,
            default_scopes,
        } => {
            let lane = smart.ok_or_else(|| {
                "instance declares auth mode `bearer_mint` but the ixit declares no `smart` \
                 lane to mint against"
                    .to_owned()
            })?;
            // A request-level `scopes` always wins; otherwise the instance's
            // standing grant applies.
            let token = mint_access_token(
                &lane.mint,
                subject.as_deref(),
                roles.as_deref(),
                scopes.unwrap_or(default_scopes),
            )?;
            Ok(Some(format!("Bearer {token}")))
        }
    }
}

/// Runner-clock milliseconds since the Unix epoch.
///
/// Wall-clock time comes from `jiff`, the pinned time library; elapsed-time
/// measurement uses [`std::time::Instant`] instead.
#[must_use]
pub fn now_ms() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

/// Mints one RS256 access token against the party's declared static test
/// issuer, carrying the requested SMART `scope` claim.
///
/// The CDR is a SMART resource server: it validates presented tokens and never
/// issues them (ITS-REST `docs/smart_app_launch/master06-authentication.adoc`
/// §Supported Authentication Flows), so the instrument takes the issuer's
/// role for the SMART lane. The token carries the registered
/// `iss`/`aud`/`sub`/`iat`/`exp` claims, the space-delimited `scope` claim
/// master08 §Resource Scopes defines, and the RBAC role claim.
///
/// # Errors
/// The key file's read or parse failure, or the signer's own failure.
pub fn mint_access_token(
    mint: &BearerMint,
    subject: Option<&str>,
    roles: Option<&[String]>,
    scopes: &[String],
) -> Result<String, String> {
    let pem = std::fs::read(&mint.key_file).map_err(|e| {
        format!(
            "smart mint: cannot read key file {}: {e}",
            mint.key_file.display()
        )
    })?;
    let key = jsonwebtoken::EncodingKey::from_rsa_pem(&pem)
        .map_err(|e| format!("smart mint: key file is not an RSA PEM: {e}"))?;
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some(mint.kid.clone());

    let issued_at = jiff::Timestamp::now().as_second();
    let expires_at = issued_at.saturating_add(i64::try_from(mint.ttl_seconds).unwrap_or(i64::MAX));
    let mut claims = serde_json::Map::new();
    claims.insert("iss".to_owned(), Value::String(mint.issuer.clone()));
    if let Some(audience) = &mint.audience {
        claims.insert("aud".to_owned(), Value::String(audience.clone()));
    }
    claims.insert(
        "sub".to_owned(),
        Value::String(subject.unwrap_or(&mint.subject).to_owned()),
    );
    claims.insert("iat".to_owned(), Value::from(issued_at));
    claims.insert("exp".to_owned(), Value::from(expires_at));
    // master08 §Resource Scopes: scopes ride the OAuth 2.0 `scope` claim,
    // space-delimited (RFC 6749 §3.3). An empty declaration mints the claim
    // as an empty string — the scope-less token the fail-closed deny branch
    // needs, distinct from a token with no `scope` claim at all.
    claims.insert("scope".to_owned(), Value::String(scopes.join(" ")));
    claims.insert(
        "realm_access".to_owned(),
        serde_json::json!({ "roles": roles.unwrap_or(&mint.roles) }),
    );

    jsonwebtoken::encode(&header, &claims, &key)
        .map_err(|e| format!("smart mint: cannot sign the access token: {e}"))
}

#[cfg(test)]
mod tests {
    use super::{AuthMode, lane_auth_header, mint_access_token};
    use crate::ixit::Ixit;
    use serde_json::Value;

    /// The committed CNF SMART test issuer (`fixtures/smart-test-issuer/`) —
    /// public test material by design, never production key material.
    fn test_mint(roles: &[String]) -> crate::ixit::BearerMint {
        let key_file = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
            .join("fixtures/smart-test-issuer/cnf-smart-test.key.pem");
        serde_json::from_value(serde_json::json!({
            "issuer": "https://as.cnf.test",
            "audience": "cnf-smart-sut",
            "subject": "cnf-smart-app",
            "roles": roles,
            "key_file": key_file,
            "kid": "cnf-smart-test",
            "ttl_seconds": 300
        }))
        .unwrap()
    }

    /// Verifies a minted token against the COMMITTED PUBLIC JWKS, so the key
    /// pair itself is under test, not just the encoder.
    fn decode_against_committed_jwks(token: &str) -> serde_json::Map<String, Value> {
        let header = jsonwebtoken::decode_header(token).unwrap();
        assert_eq!(header.alg, jsonwebtoken::Algorithm::RS256);
        assert_eq!(header.kid.as_deref(), Some("cnf-smart-test"));

        let jwks_path = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
            .join("fixtures/smart-test-issuer/jwks.json");
        let jwks: jsonwebtoken::jwk::JwkSet =
            serde_json::from_str(&std::fs::read_to_string(jwks_path).unwrap()).unwrap();
        let jwk = jwks.find("cnf-smart-test").unwrap();
        let key = jsonwebtoken::DecodingKey::from_jwk(jwk).unwrap();
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
        validation.set_issuer(&["https://as.cnf.test"]);
        validation.set_audience(&["cnf-smart-sut"]);
        jsonwebtoken::decode::<serde_json::Map<String, Value>>(token, &key, &validation)
            .unwrap()
            .claims
    }

    /// The minted token is a real RS256 JWT a resource server's validator
    /// reads: `kid`-tagged header, the registered claims, the master08
    /// `scope` claim space-delimited (RFC 6749 §3.3), and the RBAC role claim.
    #[test]
    fn mint_signs_a_scope_carrying_rs256_token() {
        let mint = test_mint(&["USER".to_owned()]);
        let token = mint_access_token(
            &mint,
            None,
            None,
            &["user/template-*.r".to_owned(), "openid".to_owned()],
        )
        .unwrap();
        let claims = decode_against_committed_jwks(&token);

        assert_eq!(claims["sub"], Value::from("cnf-smart-app"));
        assert_eq!(claims["scope"], Value::from("user/template-*.r openid"));
        assert_eq!(claims["realm_access"]["roles"][0], Value::from("USER"));
        assert!(claims["exp"].as_i64().unwrap() > claims["iat"].as_i64().unwrap());
    }

    /// An empty scope declaration mints the scope-less token the fail-closed
    /// deny branch needs (master08 §Scopes ¶2), so the claim is present and
    /// empty.
    #[test]
    fn mint_emits_an_empty_scope_claim_for_a_scopeless_token() {
        let token = mint_access_token(&test_mint(&["USER".to_owned()]), None, None, &[]).unwrap();
        let claims = decode_against_committed_jwks(&token);
        assert_eq!(claims["scope"], Value::from(""));
    }

    /// `bearer_mint` without a declared lane is an authoring defect, never a
    /// silently un-authenticated request.
    #[test]
    fn bearer_mint_without_a_lane_is_an_error() {
        let ixit: Ixit = serde_json::from_value(serde_json::json!({
            "instances": { "sut": { "base_url": "http://x", "auth": { "mode": "bearer_mint" } } }
        }))
        .unwrap();
        let no_scopes: &[String] = &[];
        let error = lane_auth_header(
            ixit.smart.as_ref(),
            &AuthMode::BearerMint {
                subject: None,
                roles: None,
                default_scopes: Vec::new(),
            },
            Some(no_scopes),
        )
        .unwrap_err();
        assert!(error.contains("no `smart` lane"), "{error}");
    }
}
