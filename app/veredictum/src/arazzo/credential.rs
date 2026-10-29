// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Where a run's credentials come from, and the transport that is the one
//! place their values exist.
//!
//! A credential is read from the environment, never from an argument: a
//! variable `NAME`, or a file named by `NAME_FILE`, the convention container
//! secrets are mounted under. The caller locates each one up front
//! ([`CredentialSource::locate`]), which checks where it comes from without
//! reading it. Its value is read once, when [`CredentialedTransport`] is
//! built, and stays inside that transport. The evaluator sends a principal's
//! step with that principal's marker as its `Authorization` value
//! ([`ORDINARY_MARKER`], [`ADMINISTRATIVE_MARKER`]), and a scoped step with
//! its scope set's marker ([`crate::arazzo::scope::marker`]); the transport
//! puts the credential or token in the marker's place on the wire, so the
//! plan, the results and the recording only ever hold the marker. The
//! scoped tokens are declared in the IXIT ([`ScopedSources`]). No openEHR
//! spec governs this — our own design.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use base64::Engine as _;
use serde::Deserialize;
use serde_json::Value;

use crate::arazzo::scope::{self, Coverage, ScopeSet};
use crate::bench::client::AuthKind;
use crate::exec::transport::{RawResponse, Transport};
use crate::vocab::HttpMethod;

/// The `Authorization` value a step sent as the ordinary principal carries
/// until the transport puts the credential in its place.
pub const ORDINARY_MARKER: &str = "[the ordinary principal's credential]";

/// The `Authorization` value a step sent as the administrative principal
/// carries until the transport puts the credential in its place.
pub const ADMINISTRATIVE_MARKER: &str = "[the administrative principal's credential]";

/// The suffix of the variable that names a file holding a secret.
pub const FILE_SUFFIX: &str = "_FILE";

/// The user of the administrative principal under `basic`.
pub const ADMIN_USER_ENV: &str = "VEREDICTUM_CDR_ADMIN_USER";

/// The password of the administrative principal under `basic`.
pub const ADMIN_PASSWORD_ENV: &str = "VEREDICTUM_CDR_ADMIN_PASSWORD";

/// The token of the administrative principal under `bearer`.
pub const ADMIN_TOKEN_ENV: &str = "VEREDICTUM_CDR_ADMIN_TOKEN";

/// The IXIT member that declares the tokens a scoped step presents.
pub const SCOPED_TOKENS_MEMBER: &str = "scoped_tokens";

/// The prefix every variable a supplied scoped token is read from carries,
/// so an IXIT can name no other variable of the environment.
pub const SCOPED_TOKEN_PREFIX: &str = "VEREDICTUM_SCOPED_TOKEN_";

/// The RSA private key, in PEM, a declared test issuer signs with.
pub const SCOPED_KEY_ENV: &str = "VEREDICTUM_SCOPED_TOKEN_KEY";

/// The principal a credential belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The run's ordinary principal, which reads `VEREDICTUM_CDR_USER`,
    /// `VEREDICTUM_CDR_PASSWORD` and `VEREDICTUM_CDR_TOKEN`.
    Ordinary,
    /// The administrative principal, which reads `VEREDICTUM_CDR_ADMIN_USER`,
    /// `VEREDICTUM_CDR_ADMIN_PASSWORD` and `VEREDICTUM_CDR_ADMIN_TOKEN`.
    Administrative,
}

impl Role {
    /// The variables this principal's `basic` user and password and its
    /// `bearer` token are read from.
    #[must_use]
    pub fn variables(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Ordinary => (
                crate::identity::USER_ENV,
                crate::identity::PASSWORD_ENV,
                crate::identity::TOKEN_ENV,
            ),
            Self::Administrative => (ADMIN_USER_ENV, ADMIN_PASSWORD_ENV, ADMIN_TOKEN_ENV),
        }
    }

    /// The command-line flag that chooses how this principal logs in.
    #[must_use]
    pub fn flag(self) -> &'static str {
        match self {
            Self::Ordinary => "--auth",
            Self::Administrative => "--admin-auth",
        }
    }

    /// The marker this principal's steps carry.
    #[must_use]
    pub fn marker(self) -> &'static str {
        match self {
            Self::Ordinary => ORDINARY_MARKER,
            Self::Administrative => ADMINISTRATIVE_MARKER,
        }
    }
}

/// Why a credential cannot be located or read.
#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    /// Neither the variable nor its file variable is set to a non-empty
    /// value.
    #[error(
        "the credential is read from {variable} or from the file {file_variable} names, and \
         neither is set (an empty value counts as unset)"
    )]
    Missing {
        /// The variable.
        variable: String,
        /// The variable naming a file.
        file_variable: String,
    },
    /// A login mode's credential is not supplied. The error names the flag,
    /// the mode and every variable the mode reads.
    #[error(
        "{flag} {mode} needs {needs}, and {variable} is not set, nor is {file_variable} (an \
         empty value counts as unset)"
    )]
    Unsupplied {
        /// The flag that chose the mode (`--auth` or `--admin-auth`).
        flag: &'static str,
        /// The mode (`basic` or `bearer`).
        mode: &'static str,
        /// The variables the mode reads, with their `_FILE` alternative.
        needs: String,
        /// The variable that is not set.
        variable: String,
        /// The variable naming a file, which is not set either.
        file_variable: String,
    },
    /// Both the variable and its file variable are set.
    #[error("both {variable} and {file_variable} are set; set one")]
    Both {
        /// The variable.
        variable: String,
        /// The variable naming a file.
        file_variable: String,
    },
    /// The file a file variable names cannot be read.
    #[error("{file_variable} names `{}`, which cannot be read: {source}", path.display())]
    Unreadable {
        /// The variable naming the file.
        file_variable: String,
        /// The file.
        path: PathBuf,
        /// The filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// The variable is set but does not read as text.
    #[error("{0} is set but is not valid Unicode")]
    NotUnicode(String),
    /// The IXIT's `scoped_tokens` member does not declare scoped tokens.
    #[error("the IXIT's `{SCOPED_TOKENS_MEMBER}` member: {0}")]
    Declaration(String),
    /// The test issuer's signing key is not an RSA private key in PEM.
    #[error("{SCOPED_KEY_ENV} does not hold an RSA private key in PEM: {0}")]
    Key(String),
}

/// Where one secret is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Locator {
    /// This environment variable.
    Variable(String),
    /// The file a `_FILE` variable names.
    File {
        /// The variable that names the file.
        variable: String,
        /// The file.
        path: PathBuf,
    },
}

/// Where a principal's credential is read from: a locator that holds no
/// secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialSource {
    /// The principal presents no credential.
    None,
    /// HTTP Basic, from a user and a password.
    Basic {
        /// Where the user is read from.
        user: Locator,
        /// Where the password is read from.
        password: Locator,
    },
    /// A bearer token.
    Bearer {
        /// Where the token is read from.
        token: Locator,
    },
}

impl CredentialSource {
    /// Locates the credential `kind` presents for `role` in the process
    /// environment, without reading it.
    ///
    /// # Errors
    /// [`CredentialError`] when a secret's variable and file variable are
    /// both set or both unset, or the file cannot be opened.
    pub fn locate(kind: AuthKind, role: Role) -> Result<Self, CredentialError> {
        Self::locate_with(kind, role, &|name| std::env::var_os(name))
    }

    /// [`Self::locate`] over `lookup` in place of the process environment.
    ///
    /// # Errors
    /// As [`Self::locate`].
    pub fn locate_with(
        kind: AuthKind,
        role: Role,
        lookup: &dyn Fn(&str) -> Option<OsString>,
    ) -> Result<Self, CredentialError> {
        let (user, password, token) = role.variables();
        let needs = match kind {
            AuthKind::None => String::new(),
            AuthKind::Basic => format!("{user} and {password} (or their {FILE_SUFFIX} forms)"),
            AuthKind::Bearer => format!("{token} (or its {FILE_SUFFIX} form)"),
        };
        let locate = |variable: &str| {
            locate_source(variable, lookup).map_err(|error| match error {
                CredentialError::Missing {
                    variable,
                    file_variable,
                } => CredentialError::Unsupplied {
                    flag: role.flag(),
                    mode: kind.as_str(),
                    needs: needs.clone(),
                    variable,
                    file_variable,
                },
                other => other,
            })
        };
        Ok(match kind {
            AuthKind::None => Self::None,
            AuthKind::Basic => Self::Basic {
                user: locate(user)?,
                password: locate(password)?,
            },
            AuthKind::Bearer => Self::Bearer {
                token: locate(token)?,
            },
        })
    }

    /// Whether the principal presents a credential at all.
    #[must_use]
    pub fn is_supplied(&self) -> bool {
        !matches!(self, Self::None)
    }
}

/// Locates one secret. A variable set to the empty string counts as unset:
/// a CI system expands a secret it does not hold to an empty value, and an
/// empty credential presented to a server is a missing one.
fn locate_source(
    variable: &str,
    lookup: &dyn Fn(&str) -> Option<OsString>,
) -> Result<Locator, CredentialError> {
    let file_variable = format!("{variable}{FILE_SUFFIX}");
    let set = |name: &str| lookup(name).filter(|value| !value.is_empty());
    match (set(variable).is_some(), set(&file_variable)) {
        (true, None) => Ok(Locator::Variable(variable.to_owned())),
        (false, Some(path)) => {
            let path = PathBuf::from(path);
            std::fs::File::open(&path).map_err(|source| CredentialError::Unreadable {
                file_variable: file_variable.clone(),
                path: path.clone(),
                source,
            })?;
            Ok(Locator::File {
                variable: file_variable,
                path,
            })
        }
        (true, Some(_)) => Err(CredentialError::Both {
            variable: variable.to_owned(),
            file_variable,
        }),
        (false, None) => Err(CredentialError::Missing {
            variable: variable.to_owned(),
            file_variable,
        }),
    }
}

/// Reads one secret. One trailing line ending is dropped from a file, the
/// one an editor or an `echo` leaves; anything else in it is the secret.
fn read(source: &Locator) -> Result<String, CredentialError> {
    match source {
        Locator::Variable(variable) => {
            let missing = || CredentialError::Missing {
                variable: variable.clone(),
                file_variable: format!("{variable}{FILE_SUFFIX}"),
            };
            match std::env::var(variable) {
                Ok(value) if value.is_empty() => Err(missing()),
                Ok(value) => Ok(value),
                Err(std::env::VarError::NotPresent) => Err(missing()),
                Err(std::env::VarError::NotUnicode(_)) => {
                    Err(CredentialError::NotUnicode(variable.clone()))
                }
            }
        }
        Locator::File { variable, path } => {
            let text =
                std::fs::read_to_string(path).map_err(|source| CredentialError::Unreadable {
                    file_variable: variable.clone(),
                    path: path.clone(),
                    source,
                })?;
            let text = text.strip_suffix('\n').unwrap_or(&text);
            Ok(text.strip_suffix('\r').unwrap_or(text).to_owned())
        }
    }
}

/// One principal's `Authorization` value. It prints as `<redacted>` and has
/// no `Display`, so it cannot reach a log, a result or a recording by
/// formatting.
pub struct Credential(String);

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Credential(<redacted>)")
    }
}

impl Credential {
    /// Reads the credential `source` locates; `None` when it locates none.
    ///
    /// # Errors
    /// [`CredentialError`] when a secret no longer reads.
    pub fn read(source: &CredentialSource) -> Result<Option<Self>, CredentialError> {
        Ok(match source {
            CredentialSource::None => None,
            CredentialSource::Basic { user, password } => {
                let pair = format!("{}:{}", read(user)?, read(password)?);
                let encoded = base64::engine::general_purpose::STANDARD.encode(pair);
                Some(Self(format!("Basic {encoded}")))
            }
            CredentialSource::Bearer { token } => Some(Self(format!("Bearer {}", read(token)?))),
        })
    }
}

/// The test issuer an IXIT declares, which mints a token for any scope set.
///
/// The minted token is an RS256 JSON Web Token (RFC 7519) carrying `iss`,
/// `sub`, `aud` when declared, `iat`, `exp`, the space-delimited `scope`
/// claim (RFC 6749 §3.3) and any further claims the IXIT declares.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MintDeclaration {
    /// The `iss` claim.
    pub issuer: String,
    /// The `sub` claim.
    pub subject: String,
    /// The `aud` claim, if the deployment checks one.
    pub audience: Option<String>,
    /// The `kid` header parameter naming the key the deployment verifies
    /// with.
    pub kid: String,
    /// How long a minted token lives, in seconds.
    pub ttl_seconds: u32,
    /// Further claims, such as the role claim a deployment's own access
    /// control reads. None may name a claim the minter sets.
    #[serde(default)]
    pub claims: serde_json::Map<String, Value>,
}

/// The claims a minted token always carries, which the IXIT cannot set.
pub const MINTED_CLAIMS: [&str; 6] = ["iss", "sub", "aud", "iat", "exp", "scope"];

/// The IXIT's `scoped_tokens` member: where the tokens a scoped step
/// presents come from. It names variables and a test issuer, never a token.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedTokensDeclaration {
    /// The variable each supplied token is read from, by scope set.
    #[serde(default)]
    pub issued: BTreeMap<String, String>,
    /// The test issuer, if the deployment trusts one.
    pub mint: Option<MintDeclaration>,
}

/// Where the tokens a scoped step presents come from: a locator that holds
/// no secret.
///
/// The IXIT's `scoped_tokens` member declares them. Its `issued` map names,
/// for each scope set, the variable (or the `_FILE` variable) holding a
/// token the party's own Authorization Server issued for it; its `mint`
/// member declares a test issuer the deployment trusts, whose RSA key is read
/// from [`SCOPED_KEY_ENV`]. A supplied token wins over a minted one. No
/// openEHR spec governs this — our own design.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopedSources {
    issued: BTreeMap<ScopeSet, Locator>,
    mint: Option<(MintDeclaration, Locator)>,
}

impl ScopedSources {
    /// Locates every token the IXIT declares in the process environment,
    /// without reading it.
    ///
    /// # Errors
    /// [`CredentialError::Declaration`] when the member does not declare
    /// scoped tokens, and the errors of [`CredentialSource::locate`] for a
    /// variable it names.
    pub fn locate(ixit: Option<&Value>) -> Result<Self, CredentialError> {
        Self::locate_with(ixit, &|name| std::env::var_os(name))
    }

    /// [`Self::locate`] over `lookup` in place of the process environment.
    ///
    /// # Errors
    /// As [`Self::locate`].
    pub fn locate_with(
        ixit: Option<&Value>,
        lookup: &dyn Fn(&str) -> Option<OsString>,
    ) -> Result<Self, CredentialError> {
        let Some(member) = ixit.and_then(|ixit| ixit.get(SCOPED_TOKENS_MEMBER)) else {
            return Ok(Self::default());
        };
        let bad = |reason: String| CredentialError::Declaration(reason);
        let declared: ScopedTokensDeclaration =
            serde_json::from_value(member.clone()).map_err(|e| bad(e.to_string()))?;
        let mut issued = BTreeMap::new();
        for (scope, variable) in &declared.issued {
            let set = ScopeSet::parse(scope).map_err(|e| bad(e.to_string()))?;
            let named = variable
                .strip_prefix(SCOPED_TOKEN_PREFIX)
                .is_some_and(|rest| {
                    !rest.is_empty()
                        && rest
                            .chars()
                            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                });
            if !named || variable == SCOPED_KEY_ENV {
                return Err(bad(format!(
                    "`{variable}` is not a variable named `{SCOPED_TOKEN_PREFIX}<NAME>` \
                     (upper-case letters, digits and `_`, and not {SCOPED_KEY_ENV})"
                )));
            }
            let source = locate_source(variable, lookup)?;
            if issued.insert(set.clone(), source).is_some() {
                return Err(bad(format!("the scope set `{set}` is declared twice")));
            }
        }
        let mint = match declared.mint {
            None => None,
            Some(mint) => {
                if mint.ttl_seconds == 0 {
                    return Err(bad("`mint.ttl_seconds` must be at least 1".to_owned()));
                }
                if let Some(claim) = MINTED_CLAIMS.iter().find(|c| mint.claims.contains_key(**c)) {
                    return Err(bad(format!(
                        "`mint.claims` sets `{claim}`, which the minter sets"
                    )));
                }
                let key = locate_source(SCOPED_KEY_ENV, lookup)?;
                Some((mint, key))
            }
        };
        Ok(Self { issued, mint })
    }

    /// The scope sets these sources cover, which names no token.
    #[must_use]
    pub fn coverage(&self) -> Coverage {
        Coverage {
            issued: self.issued.keys().cloned().collect(),
            minted: self.mint.is_some(),
        }
    }
}

/// A declared test issuer with its key read.
struct Minter {
    declaration: MintDeclaration,
    key: jsonwebtoken::EncodingKey,
}

impl fmt::Debug for Minter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Minter")
            .field("declaration", &self.declaration)
            .field("key", &"<redacted>")
            .finish()
    }
}

impl Minter {
    /// Mints a token whose `scope` claim is `set`, valid from now.
    fn mint(&self, set: &ScopeSet) -> Result<Credential, String> {
        let declaration = &self.declaration;
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
        header.kid = Some(declaration.kid.clone());
        let issued_at = jiff::Timestamp::now().as_second();
        let expires_at = issued_at.saturating_add(i64::from(declaration.ttl_seconds));
        let mut claims = declaration.claims.clone();
        claims.insert("iss".to_owned(), Value::String(declaration.issuer.clone()));
        claims.insert("sub".to_owned(), Value::String(declaration.subject.clone()));
        if let Some(audience) = &declaration.audience {
            claims.insert("aud".to_owned(), Value::String(audience.clone()));
        }
        claims.insert("iat".to_owned(), Value::from(issued_at));
        claims.insert("exp".to_owned(), Value::from(expires_at));
        claims.insert("scope".to_owned(), Value::String(set.to_string()));
        jsonwebtoken::encode(&header, &claims, &self.key)
            .map(|token| Credential(format!("Bearer {token}")))
            .map_err(|e| format!("the test issuer cannot sign a token: {e}"))
    }
}

/// The tokens scoped steps present, read once.
#[derive(Debug, Default)]
struct ScopedTokens {
    issued: BTreeMap<ScopeSet, Credential>,
    minter: Option<Minter>,
}

impl ScopedTokens {
    fn read(sources: &ScopedSources) -> Result<Self, CredentialError> {
        let mut issued = BTreeMap::new();
        for (set, source) in &sources.issued {
            issued.insert(set.clone(), Credential(format!("Bearer {}", read(source)?)));
        }
        let minter = match &sources.mint {
            None => None,
            Some((declaration, key)) => {
                let pem = read(key)?;
                let key = jsonwebtoken::EncodingKey::from_rsa_pem(pem.as_bytes())
                    .map_err(|e| CredentialError::Key(e.to_string()))?;
                Some(Minter {
                    declaration: declaration.clone(),
                    key,
                })
            }
        };
        Ok(Self { issued, minter })
    }

    /// The `Authorization` value for `set`: the supplied token, else a
    /// minted one.
    fn token(&self, set: &ScopeSet) -> Result<String, String> {
        if let Some(Credential(value)) = self.issued.get(set) {
            return Ok(value.clone());
        }
        match &self.minter {
            Some(minter) => minter.mint(set).map(|Credential(value)| value),
            None => Err(format!("no token is supplied for the scope set `{set}`")),
        }
    }
}

/// A transport that puts each principal's credential, and each scoped
/// step's token, in place of its marker and hands the request to the
/// transport it wraps.
///
/// A request whose `Authorization` is anything other than a marker, such as
/// a deliberately invalid credential a step sets, is sent unchanged.
#[derive(Debug)]
pub struct CredentialedTransport<T> {
    inner: T,
    ordinary: Option<Credential>,
    administrative: Option<Credential>,
    scoped: ScopedTokens,
}

impl<T: Transport> CredentialedTransport<T> {
    /// Reads both principals' credentials once and wraps `inner`.
    ///
    /// # Errors
    /// [`CredentialError`] when a located secret no longer reads.
    pub fn new(
        inner: T,
        ordinary: &CredentialSource,
        administrative: &CredentialSource,
    ) -> Result<Self, CredentialError> {
        Ok(Self {
            inner,
            ordinary: Credential::read(ordinary)?,
            administrative: Credential::read(administrative)?,
            scoped: ScopedTokens::default(),
        })
    }

    /// Reads the scoped tokens `sources` locate, and the test issuer's key,
    /// once.
    ///
    /// # Errors
    /// [`CredentialError`] when a located secret no longer reads, or the
    /// test issuer's key is not an RSA private key in PEM.
    pub fn with_scoped(mut self, sources: &ScopedSources) -> Result<Self, CredentialError> {
        self.scoped = ScopedTokens::read(sources)?;
        Ok(self)
    }
}

/// The `Authorization` value of a principal's credential, if supplied.
fn held(credential: Option<&Credential>, role: Role) -> Result<String, String> {
    credential
        .map(|Credential(value)| value.clone())
        .ok_or_else(|| format!("the {role:?} principal's credential is not supplied"))
}

impl<T: Transport> Transport for CredentialedTransport<T> {
    fn exchange(
        &mut self,
        method: HttpMethod,
        url: &str,
        headers: &BTreeMap<String, String>,
        body: Option<&Value>,
        body_is_json: bool,
    ) -> Result<RawResponse, String> {
        let presented = headers.get("authorization").map(String::as_str);
        let value = match presented {
            Some(ORDINARY_MARKER) => held(self.ordinary.as_ref(), Role::Ordinary)?,
            Some(ADMINISTRATIVE_MARKER) => {
                held(self.administrative.as_ref(), Role::Administrative)?
            }
            Some(other) => match scope::from_marker(other) {
                Some(set) => self.scoped.token(&set.map_err(|e| e.to_string())?)?,
                None => {
                    return self
                        .inner
                        .exchange(method, url, headers, body, body_is_json);
                }
            },
            None => {
                return self
                    .inner
                    .exchange(method, url, headers, body, body_is_json);
            }
        };
        let mut sent = headers.clone();
        sent.insert("authorization".to_owned(), value);
        self.inner.exchange(method, url, &sent, body, body_is_json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;
    use reqwest::StatusCode;

    fn lookup(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> + use<> {
        let vars: BTreeMap<String, OsString> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), OsString::from(v)))
            .collect();
        move |name| vars.get(name).cloned()
    }

    #[test]
    fn a_secret_is_located_by_its_variable_or_its_file_variable() {
        let dir = assert_fs::TempDir::new().unwrap();
        let file = dir.child("password");
        file.write_str("s3cret\r\n").unwrap();
        let path = file.path().to_str().unwrap();
        assert_eq!(
            locate_source("P", &lookup(&[("P", "plain")])).unwrap(),
            Locator::Variable("P".to_owned())
        );
        let located = locate_source("P", &lookup(&[("P_FILE", path)])).unwrap();
        assert_eq!(read(&located).unwrap(), "s3cret");
        assert!(matches!(
            locate_source("P", &lookup(&[("P", "x"), ("P_FILE", path)])),
            Err(CredentialError::Both { .. })
        ));
        assert!(matches!(
            locate_source("P", &lookup(&[])),
            Err(CredentialError::Missing { .. })
        ));
        assert!(matches!(
            locate_source("P", &lookup(&[("P_FILE", "/nonexistent/secret")])),
            Err(CredentialError::Unreadable { .. })
        ));
    }

    #[test]
    fn an_empty_variable_counts_as_unset() {
        assert!(matches!(
            locate_source("P", &lookup(&[("P", "")])),
            Err(CredentialError::Missing { variable, .. }) if variable == "P"
        ));
        let dir = assert_fs::TempDir::new().unwrap();
        let file = dir.child("token");
        file.write_str("t0ken\n").unwrap();
        let path = file.path().to_str().unwrap();
        assert!(matches!(
            locate_source("P", &lookup(&[("P", ""), ("P_FILE", path)])),
            Ok(Locator::File { .. })
        ));
    }

    #[test]
    fn an_empty_file_variable_counts_as_unset() {
        assert!(matches!(
            locate_source("P", &lookup(&[("P_FILE", "")])),
            Err(CredentialError::Missing { file_variable, .. }) if file_variable == "P_FILE"
        ));
        assert!(matches!(
            locate_source("P", &lookup(&[("P", "plain"), ("P_FILE", "")])),
            Ok(Locator::Variable(variable)) if variable == "P"
        ));
        assert!(matches!(
            locate_source("P", &lookup(&[("P", ""), ("P_FILE", "")])),
            Err(CredentialError::Missing { .. })
        ));
    }

    #[test]
    fn each_principal_is_located_by_its_own_variables() {
        let vars = lookup(&[
            ("VEREDICTUM_CDR_USER", "alice"),
            ("VEREDICTUM_CDR_PASSWORD", "a"),
            ("VEREDICTUM_CDR_ADMIN_TOKEN", "t"),
        ]);
        assert_eq!(
            CredentialSource::locate_with(AuthKind::Basic, Role::Ordinary, &vars).unwrap(),
            CredentialSource::Basic {
                user: Locator::Variable("VEREDICTUM_CDR_USER".to_owned()),
                password: Locator::Variable("VEREDICTUM_CDR_PASSWORD".to_owned()),
            }
        );
        assert_eq!(
            CredentialSource::locate_with(AuthKind::Bearer, Role::Administrative, &vars).unwrap(),
            CredentialSource::Bearer {
                token: Locator::Variable("VEREDICTUM_CDR_ADMIN_TOKEN".to_owned()),
            }
        );
        let none =
            CredentialSource::locate_with(AuthKind::None, Role::Administrative, &vars).unwrap();
        assert!(!none.is_supplied());
        assert!(
            CredentialSource::locate_with(AuthKind::Basic, Role::Administrative, &vars).is_err()
        );
    }

    #[test]
    fn a_missing_credential_names_the_flag_the_mode_and_its_variables() {
        let vars = lookup(&[("VEREDICTUM_CDR_USER", "alice")]);
        let basic = CredentialSource::locate_with(AuthKind::Basic, Role::Ordinary, &vars)
            .unwrap_err()
            .to_string();
        assert_eq!(
            basic,
            "--auth basic needs VEREDICTUM_CDR_USER and VEREDICTUM_CDR_PASSWORD (or their _FILE \
             forms), and VEREDICTUM_CDR_PASSWORD is not set, nor is \
             VEREDICTUM_CDR_PASSWORD_FILE (an empty value counts as unset)"
        );
        let bearer = CredentialSource::locate_with(AuthKind::Bearer, Role::Administrative, &vars)
            .unwrap_err()
            .to_string();
        assert_eq!(
            bearer,
            "--admin-auth bearer needs VEREDICTUM_CDR_ADMIN_TOKEN (or its _FILE form), and \
             VEREDICTUM_CDR_ADMIN_TOKEN is not set, nor is VEREDICTUM_CDR_ADMIN_TOKEN_FILE (an \
             empty value counts as unset)"
        );
    }

    #[test]
    fn a_credential_prints_redacted() {
        let dir = assert_fs::TempDir::new().unwrap();
        let file = dir.child("token");
        file.write_str("t0ken-value").unwrap();
        let source = CredentialSource::Bearer {
            token: Locator::File {
                variable: "T_FILE".to_owned(),
                path: file.path().to_owned(),
            },
        };
        let credential = Credential::read(&source).unwrap().unwrap();
        let printed = format!("{credential:?}");
        assert_eq!(printed, "Credential(<redacted>)");
        let transport = CredentialedTransport {
            inner: Sent::default(),
            ordinary: Some(credential),
            administrative: None,
            scoped: ScopedTokens::default(),
        };
        assert!(!format!("{transport:?}").contains("t0ken-value"));
    }

    /// A transport that answers 200 and keeps the headers it was sent.
    #[derive(Debug, Default)]
    struct Sent(Vec<BTreeMap<String, String>>);

    impl Transport for Sent {
        fn exchange(
            &mut self,
            _method: HttpMethod,
            _url: &str,
            headers: &BTreeMap<String, String>,
            _body: Option<&Value>,
            _body_is_json: bool,
        ) -> Result<RawResponse, String> {
            self.0.push(headers.clone());
            Ok(RawResponse {
                status: StatusCode::OK,
                headers: BTreeMap::new(),
                text: String::new(),
            })
        }
    }

    #[test]
    fn the_transport_puts_each_credential_in_place_of_its_marker() {
        let mut transport = CredentialedTransport {
            inner: Sent::default(),
            ordinary: Some(Credential("Basic b3JkaW5hcnk=".to_owned())),
            administrative: None,
            scoped: ScopedTokens::default(),
        };
        let send = |transport: &mut CredentialedTransport<Sent>, value: &str| {
            let headers = BTreeMap::from([("authorization".to_owned(), value.to_owned())]);
            transport.exchange(
                HttpMethod::Get,
                "https://sut.invalid/ehr",
                &headers,
                None,
                true,
            )
        };
        assert!(send(&mut transport, ORDINARY_MARKER).is_ok());
        assert!(send(&mut transport, "Bearer veredictum-invalid-credential").is_ok());
        assert!(send(&mut transport, ADMINISTRATIVE_MARKER).is_err());
        let delivered: Vec<&str> = transport
            .inner
            .0
            .iter()
            .filter_map(|headers| headers.get("authorization").map(String::as_str))
            .collect();
        assert_eq!(
            delivered,
            ["Basic b3JkaW5hcnk=", "Bearer veredictum-invalid-credential"]
        );
    }

    fn set(text: &str) -> ScopeSet {
        ScopeSet::parse(text).unwrap()
    }

    /// `object` with `key` set to `value`.
    fn with(object: &Value, key: &str, value: Value) -> Value {
        let mut out = object.clone();
        if let Some(map) = out.as_object_mut() {
            map.insert(key.to_owned(), value);
        }
        out
    }

    fn scoped_ixit(member: Value) -> Value {
        Value::Object(serde_json::Map::from_iter([(
            SCOPED_TOKENS_MEMBER.to_owned(),
            member,
        )]))
    }

    /// The committed test issuer's RSA key, which is not secret.
    fn test_key() -> PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../catalogue/keys/smart-test-issuer/cnf-smart-test.key.pem")
    }

    #[test]
    fn scoped_tokens_are_located_from_the_ixit_without_being_read() {
        let dir = assert_fs::TempDir::new().unwrap();
        let file = dir.child("crud");
        file.write_str("tok-crud\n").unwrap();
        let path = file.path().to_str().unwrap();
        let vars = lookup(&[
            ("VEREDICTUM_SCOPED_TOKEN_READ", "tok-read"),
            ("VEREDICTUM_SCOPED_TOKEN_CRUD_FILE", path),
        ]);
        let ixit = scoped_ixit(serde_json::json!({ "issued": {
            "system/template-*.r": "VEREDICTUM_SCOPED_TOKEN_READ",
            "system/template-*.crud": "VEREDICTUM_SCOPED_TOKEN_CRUD"
        }}));
        let sources = ScopedSources::locate_with(Some(&ixit), &vars).unwrap();
        let coverage = sources.coverage();
        assert!(coverage.covers(&set("system/template-*.r")));
        assert!(coverage.covers(&set("system/template-*.crud")));
        assert!(!coverage.covers(&set("system/template-*.c")));
        assert!(!coverage.minted);
        assert_eq!(
            ScopedSources::locate_with(None, &vars).unwrap(),
            ScopedSources::default()
        );
        let unset = scoped_ixit(serde_json::json!({ "issued": {
            "a": "VEREDICTUM_SCOPED_TOKEN_UNSET"
        }}));
        assert!(matches!(
            ScopedSources::locate_with(Some(&unset), &vars),
            Err(CredentialError::Missing { variable, .. }) if variable == "VEREDICTUM_SCOPED_TOKEN_UNSET"
        ));
    }

    #[test]
    fn a_malformed_scoped_declaration_is_refused() {
        let vars = lookup(&[("VEREDICTUM_SCOPED_TOKEN_A", "t"), ("HOME", "/root")]);
        let mint =
            serde_json::json!({ "issuer": "i", "subject": "s", "kid": "k", "ttl_seconds": 60 });
        let refused = [
            serde_json::json!({ "issued": { "a": "HOME" } }),
            serde_json::json!({ "issued": { "a": "VEREDICTUM_SCOPED_TOKEN_" } }),
            serde_json::json!({ "issued": { "a": "VEREDICTUM_SCOPED_TOKEN_KEY" } }),
            serde_json::json!({ "issued": { "a\"": "VEREDICTUM_SCOPED_TOKEN_A" } }),
            serde_json::json!({ "issued": {
                "a b": "VEREDICTUM_SCOPED_TOKEN_A", "b a": "VEREDICTUM_SCOPED_TOKEN_A"
            }}),
            serde_json::json!({ "issuer": "i" }),
            serde_json::json!({ "mint": { "issuer": "i" } }),
            serde_json::json!({ "mint": with(&mint, "ttl_seconds", serde_json::json!(0)) }),
            serde_json::json!({ "mint": with(&mint, "claims", serde_json::json!({ "scope": "x" })) }),
        ];
        for member in refused {
            assert!(
                matches!(
                    ScopedSources::locate_with(Some(&scoped_ixit(member.clone())), &vars),
                    Err(CredentialError::Declaration(_))
                ),
                "{member}"
            );
        }
        assert!(matches!(
            ScopedSources::locate_with(
                Some(&scoped_ixit(serde_json::json!({ "mint": mint }))),
                &vars
            ),
            Err(CredentialError::Missing { variable, .. }) if variable == SCOPED_KEY_ENV
        ));
    }

    #[test]
    fn the_transport_puts_the_scoped_token_in_place_of_its_marker() {
        let mut transport = CredentialedTransport {
            inner: Sent::default(),
            ordinary: None,
            administrative: None,
            scoped: ScopedTokens {
                issued: BTreeMap::from([(set("a b"), Credential("Bearer tok-ab".to_owned()))]),
                minter: None,
            },
        };
        let send = |transport: &mut CredentialedTransport<Sent>, value: &str| {
            let headers = BTreeMap::from([("authorization".to_owned(), value.to_owned())]);
            transport.exchange(
                HttpMethod::Get,
                "https://sut.invalid/x",
                &headers,
                None,
                true,
            )
        };
        assert!(send(&mut transport, &scope::marker(&set("b a"))).is_ok());
        let refused = send(&mut transport, &scope::marker(&set("c")));
        assert!(refused.is_err_and(|e| e.contains("`c`") && !e.contains("tok-ab")));
        assert_eq!(transport.inner.0.len(), 1);
        assert_eq!(
            transport
                .inner
                .0
                .first()
                .and_then(|h| h.get("authorization"))
                .map(String::as_str),
            Some("Bearer tok-ab")
        );
    }

    #[test]
    fn a_declared_test_issuer_mints_a_token_carrying_the_scope_set() {
        let key = test_key();
        let key = key.to_str().unwrap();
        let vars = lookup(&[("VEREDICTUM_SCOPED_TOKEN_KEY_FILE", key)]);
        let ixit = scoped_ixit(serde_json::json!({ "mint": {
            "issuer": "https://issuer.example", "subject": "app", "audience": "cdr",
            "kid": "cnf-smart-test", "ttl_seconds": 300, "claims": { "roles": ["clinician"] }
        }}));
        let sources = ScopedSources::locate_with(Some(&ixit), &vars).unwrap();
        assert!(sources.coverage().covers(&set("anything")));
        let tokens = ScopedTokens::read(&sources).unwrap();
        assert!(!format!("{tokens:?}").contains("PRIVATE"));
        let value = tokens.token(&set("system/template-*.r openid")).unwrap();
        let jwt = value.strip_prefix("Bearer ").unwrap();
        let mut parts = jwt.split('.');
        let decode = |part: Option<&str>| -> Value {
            let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(part.unwrap())
                .unwrap();
            serde_json::from_slice(&bytes).unwrap()
        };
        let header = decode(parts.next());
        let claims = decode(parts.next());
        assert_eq!(header.get("alg"), Some(&Value::from("RS256")));
        assert_eq!(header.get("kid"), Some(&Value::from("cnf-smart-test")));
        assert_eq!(
            claims.get("scope"),
            Some(&Value::from("openid system/template-*.r"))
        );
        assert_eq!(claims.get("aud"), Some(&Value::from("cdr")));
        assert_eq!(claims.get("roles"), Some(&serde_json::json!(["clinician"])));
        let lifetime = claims.get("exp").and_then(Value::as_i64).unwrap()
            - claims.get("iat").and_then(Value::as_i64).unwrap();
        assert_eq!(lifetime, 300);
    }

    #[test]
    fn a_test_issuer_key_that_is_not_an_rsa_pem_is_refused() {
        let dir = assert_fs::TempDir::new().unwrap();
        let file = dir.child("key.pem");
        file.write_str("not a key").unwrap();
        let vars = lookup(&[(
            "VEREDICTUM_SCOPED_TOKEN_KEY_FILE",
            file.path().to_str().unwrap(),
        )]);
        let ixit = scoped_ixit(serde_json::json!({ "mint": {
            "issuer": "i", "subject": "s", "kid": "k", "ttl_seconds": 1
        }}));
        let sources = ScopedSources::locate_with(Some(&ixit), &vars).unwrap();
        assert!(matches!(
            ScopedTokens::read(&sources),
            Err(CredentialError::Key(_))
        ));
    }
}
