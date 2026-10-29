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
//! ([`ORDINARY_MARKER`], [`ADMINISTRATIVE_MARKER`]), and the transport puts
//! the credential in the marker's place on the wire, so the plan, the
//! results and the recording only ever hold the marker. No openEHR spec
//! governs this — our own design.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use base64::Engine as _;
use serde_json::Value;

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
pub const ADMIN_USER_ENV: &str = "VEREDICTUM_ADMIN_USER";

/// The password of the administrative principal under `basic`.
pub const ADMIN_PASSWORD_ENV: &str = "VEREDICTUM_ADMIN_PASSWORD";

/// The token of the administrative principal under `bearer`.
pub const ADMIN_TOKEN_ENV: &str = "VEREDICTUM_ADMIN_TOKEN";

/// The principal a credential belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The run's ordinary principal, which reads `VEREDICTUM_SUT_*`.
    Ordinary,
    /// The administrative principal, which reads `VEREDICTUM_ADMIN_*`.
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
}

/// Where one secret is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretSource {
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
        user: SecretSource,
        /// Where the password is read from.
        password: SecretSource,
    },
    /// A bearer token.
    Bearer {
        /// Where the token is read from.
        token: SecretSource,
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
        Ok(match kind {
            AuthKind::None => Self::None,
            AuthKind::Basic => Self::Basic {
                user: locate_secret(user, lookup)?,
                password: locate_secret(password, lookup)?,
            },
            AuthKind::Bearer => Self::Bearer {
                token: locate_secret(token, lookup)?,
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
fn locate_secret(
    variable: &str,
    lookup: &dyn Fn(&str) -> Option<OsString>,
) -> Result<SecretSource, CredentialError> {
    let file_variable = format!("{variable}{FILE_SUFFIX}");
    let set = |name: &str| lookup(name).filter(|value| !value.is_empty());
    match (set(variable).is_some(), set(&file_variable)) {
        (true, None) => Ok(SecretSource::Variable(variable.to_owned())),
        (false, Some(path)) => {
            let path = PathBuf::from(path);
            std::fs::File::open(&path).map_err(|source| CredentialError::Unreadable {
                file_variable: file_variable.clone(),
                path: path.clone(),
                source,
            })?;
            Ok(SecretSource::File {
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
fn read(source: &SecretSource) -> Result<String, CredentialError> {
    match source {
        SecretSource::Variable(variable) => {
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
        SecretSource::File { variable, path } => {
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

/// A transport that puts each principal's credential in place of its marker
/// and hands the request to the transport it wraps.
///
/// A request whose `Authorization` is anything other than a marker, such as
/// a deliberately invalid credential a step sets, is sent unchanged.
#[derive(Debug)]
pub struct CredentialedTransport<T> {
    inner: T,
    ordinary: Option<Credential>,
    administrative: Option<Credential>,
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
        })
    }
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
        let (role, held) = match headers.get("authorization").map(String::as_str) {
            Some(ORDINARY_MARKER) => (Role::Ordinary, &self.ordinary),
            Some(ADMINISTRATIVE_MARKER) => (Role::Administrative, &self.administrative),
            _ => {
                return self
                    .inner
                    .exchange(method, url, headers, body, body_is_json);
            }
        };
        let Some(Credential(value)) = held else {
            return Err(format!(
                "the {role:?} principal's credential is not supplied"
            ));
        };
        let mut sent = headers.clone();
        sent.insert("authorization".to_owned(), value.clone());
        self.inner.exchange(method, url, &sent, body, body_is_json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;
    use reqwest::StatusCode;

    fn lookup(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
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
            locate_secret("P", &lookup(&[("P", "plain")])).unwrap(),
            SecretSource::Variable("P".to_owned())
        );
        let located = locate_secret("P", &lookup(&[("P_FILE", path)])).unwrap();
        assert_eq!(read(&located).unwrap(), "s3cret");
        assert!(matches!(
            locate_secret("P", &lookup(&[("P", "x"), ("P_FILE", path)])),
            Err(CredentialError::Both { .. })
        ));
        assert!(matches!(
            locate_secret("P", &lookup(&[])),
            Err(CredentialError::Missing { .. })
        ));
        assert!(matches!(
            locate_secret("P", &lookup(&[("P_FILE", "/nonexistent/secret")])),
            Err(CredentialError::Unreadable { .. })
        ));
    }

    #[test]
    fn an_empty_variable_counts_as_unset() {
        assert!(matches!(
            locate_secret("P", &lookup(&[("P", "")])),
            Err(CredentialError::Missing { variable, .. }) if variable == "P"
        ));
        let dir = assert_fs::TempDir::new().unwrap();
        let file = dir.child("token");
        file.write_str("t0ken\n").unwrap();
        let path = file.path().to_str().unwrap();
        assert!(matches!(
            locate_secret("P", &lookup(&[("P", ""), ("P_FILE", path)])),
            Ok(SecretSource::File { .. })
        ));
    }

    #[test]
    fn an_empty_file_variable_counts_as_unset() {
        assert!(matches!(
            locate_secret("P", &lookup(&[("P_FILE", "")])),
            Err(CredentialError::Missing { file_variable, .. }) if file_variable == "P_FILE"
        ));
        assert!(matches!(
            locate_secret("P", &lookup(&[("P", "plain"), ("P_FILE", "")])),
            Ok(SecretSource::Variable(variable)) if variable == "P"
        ));
        assert!(matches!(
            locate_secret("P", &lookup(&[("P", ""), ("P_FILE", "")])),
            Err(CredentialError::Missing { .. })
        ));
    }

    #[test]
    fn each_principal_is_located_by_its_own_variables() {
        let vars = lookup(&[
            ("VEREDICTUM_SUT_USER", "alice"),
            ("VEREDICTUM_SUT_PASSWORD", "a"),
            ("VEREDICTUM_ADMIN_TOKEN", "t"),
        ]);
        assert_eq!(
            CredentialSource::locate_with(AuthKind::Basic, Role::Ordinary, &vars).unwrap(),
            CredentialSource::Basic {
                user: SecretSource::Variable("VEREDICTUM_SUT_USER".to_owned()),
                password: SecretSource::Variable("VEREDICTUM_SUT_PASSWORD".to_owned()),
            }
        );
        assert_eq!(
            CredentialSource::locate_with(AuthKind::Bearer, Role::Administrative, &vars).unwrap(),
            CredentialSource::Bearer {
                token: SecretSource::Variable("VEREDICTUM_ADMIN_TOKEN".to_owned()),
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
    fn a_credential_prints_redacted() {
        let dir = assert_fs::TempDir::new().unwrap();
        let file = dir.child("token");
        file.write_str("t0ken-value").unwrap();
        let source = CredentialSource::Bearer {
            token: SecretSource::File {
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
}
