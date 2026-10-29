// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The server's own identity, read from the ITS-REST System OPTIONS operation.
//!
//! ITS-REST `specifications/operations/options.yaml` ("Options and
//! Conformance") says "Services SHOULD respond to this method with the
//! appropriate HTTP codes, headers and potentially with a payload revealing
//! more details about themselves", and its `200` answer carries the `Options`
//! document of `specifications/schemas/others/Options.yaml`, every member of
//! which is optional. The released `system.openapi.yaml` serves the operation
//! on `/`, the API root.
//!
//! A SHOULD permits a server not to offer the document, so this read is a
//! probe: its answer is one of three [`IdentityRead`] outcomes, none of them a
//! finding, and it never counts toward conformance. What it feeds is the
//! record's identity: [`merge`] takes each field the server stated and asks
//! the operator only for the rest.

use std::collections::BTreeMap;

use reqwest::StatusCode;
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::arazzo::credential::{
    CredentialError, CredentialSource, CredentialedTransport, ORDINARY_MARKER, Role,
};
use crate::exec::transport::Transport;
use crate::ixit::{AuthMode, Ixit};
use crate::party::{IdentityField, Sut};
use crate::transcript::{RecordedExchange, RecordedRequest, RecordedResponse, recorded_headers};
use crate::vocab::HttpMethod;

/// The path the operation is served on, relative to the base URL (released
/// `system.openapi.yaml`, `paths: /`).
pub const PATH: &str = "/";

/// Where `--auth basic` reads the CDR user from, in `test` and `identify`.
pub const USER_ENV: &str = "VEREDICTUM_CDR_USER";

/// Where `--auth basic` reads the CDR password from, in `test` and
/// `identify`.
pub const PASSWORD_ENV: &str = "VEREDICTUM_CDR_PASSWORD";

/// Where `--auth bearer` reads the CDR token from, in `test` and `identify`.
pub const TOKEN_ENV: &str = "VEREDICTUM_CDR_TOKEN";

/// The media type the request asks for (`options.yaml` names the
/// `Accept_JSON` header parameter).
const ACCEPT_JSON: &str = "application/json";

/// The members of the `Options` document (`schemas/others/Options.yaml`).
///
/// Every member is optional. A member the schema does not name is ignored,
/// because the schema does not forbid one; a named member of the wrong JSON
/// type, `null` included, makes the answer not an `Options` document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionsDocument {
    /// The product name.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub solution: Option<String>,
    /// The product version.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub solution_version: Option<String>,
    /// The vendor.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub vendor: Option<String>,
    /// The ITS-REST release the server implements.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub restapi_specs_version: Option<String>,
    /// The conformance profile the server states.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub conformance_profile: Option<String>,
    /// The endpoints the server lists.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub endpoints: Option<Vec<String>>,
}

/// Reads a member that is present, refusing `null`: the schema types every
/// member, and `null` is none of those types.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// What the server answered to `OPTIONS <base>/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum IdentityRead {
    /// A `200` carrying an `Options` document.
    Offered {
        /// The status code, always 200.
        status: u16,
        /// The document, with whatever subset of its members the server sent.
        options: OptionsDocument,
    },
    /// Any status outside 2xx: the server does not offer the document, which
    /// the operation's SHOULD permits.
    NotOffered {
        /// The status code the server answered with.
        status: u16,
    },
    /// A 2xx whose body is not an `Options` document.
    NotOptionsDocument {
        /// The status code the server answered with.
        status: u16,
        /// Why the body is not one, in the parser's own words.
        reason: String,
    },
}

impl IdentityRead {
    /// The document, when the server offered one.
    #[must_use]
    pub fn options(&self) -> Option<&OptionsDocument> {
        match self {
            IdentityRead::Offered { options, .. } => Some(options),
            IdentityRead::NotOffered { .. } | IdentityRead::NotOptionsDocument { .. } => None,
        }
    }

    /// One line saying what the server answered, for a console.
    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            IdentityRead::Offered { .. } => {
                String::from("the server states its identity through System OPTIONS")
            }
            IdentityRead::NotOffered { status } => {
                format!("the server does not offer System OPTIONS (HTTP {status})")
            }
            IdentityRead::NotOptionsDocument { status, reason } => {
                format!(
                    "the server's System OPTIONS answer (HTTP {status}) is not an Options document: {reason}"
                )
            }
        }
    }
}

/// Classifies one answer to the System OPTIONS request.
///
/// `200` with a JSON object of the `Options` shape is offered; any status
/// outside 2xx is not offered; any other 2xx, or a 200 whose body is not that
/// shape, is not an `Options` document.
#[must_use]
pub fn classify(status: StatusCode, body: &str) -> IdentityRead {
    let code = status.as_u16();
    if !status.is_success() {
        return IdentityRead::NotOffered { status: code };
    }
    if status != StatusCode::OK {
        return IdentityRead::NotOptionsDocument {
            status: code,
            reason: format!(
                "the operation answers 200 with the document, and this answer is {status}"
            ),
        };
    }
    // A struct also deserializes from a JSON array by position, so the shape
    // is checked first: the schema declares `type: object`.
    if !body.trim_start().starts_with('{') {
        return IdentityRead::NotOptionsDocument {
            status: code,
            reason: String::from("the body is not a JSON object"),
        };
    }
    match serde_json::from_str::<OptionsDocument>(body) {
        Ok(options) => IdentityRead::Offered {
            status: code,
            options,
        },
        Err(e) => IdentityRead::NotOptionsDocument {
            status: code,
            reason: e.to_string(),
        },
    }
}

/// A failure to read the identity at all.
#[derive(Debug, Error)]
pub enum IdentityError {
    /// The credential the request needs could not be resolved.
    #[error("the System OPTIONS request has no credential: {0}")]
    Credential(String),
    /// The credential `identify --auth` names is not supplied or does not
    /// read.
    #[error(transparent)]
    Unsupplied(#[from] CredentialError),
    /// The exchange could not be performed.
    #[error("the System OPTIONS request was not answered: {0}")]
    Transport(String),
    /// Neither the server nor the operator supplied a mandatory field.
    #[error(
        "the server did not state its {field} through System OPTIONS and no {flag} was supplied: \
         supply {flag}"
    )]
    Missing {
        /// The field nobody supplied.
        field: &'static str,
        /// The flag that supplies it.
        flag: &'static str,
    },
}

/// The read and the exchange it took.
#[derive(Debug, Clone)]
pub struct IdentityExchange {
    /// The classified answer.
    pub read: IdentityRead,
    /// The exchange as a transcript records it, with the credential withheld.
    pub exchange: RecordedExchange,
}

/// The `Authorization` header an ixit instance's auth mode yields, resolved
/// exactly as the driver resolves it.
///
/// # Errors
/// [`IdentityError::Credential`] when a named variable is unset or a token
/// cannot be minted.
pub fn authorization(ixit: &Ixit, auth: &AuthMode) -> Result<Option<String>, IdentityError> {
    crate::exec::auth::lane_auth_header(ixit.smart.as_ref(), auth, None)
        .map_err(IdentityError::Credential)
}

/// Sends `OPTIONS <base>/` with `Accept: application/json` and classifies the
/// answer.
///
/// # Errors
/// [`IdentityError::Transport`] when the exchange cannot be performed. Every
/// answer the server gives, whatever its status, is a classified
/// [`IdentityRead`].
pub fn read(
    transport: &mut dyn Transport,
    base_url: &str,
    authorization: Option<&str>,
) -> Result<IdentityExchange, IdentityError> {
    let url = format!("{}{PATH}", base_url.trim_end_matches('/'));
    let mut headers = BTreeMap::new();
    headers.insert(String::from("Accept"), String::from(ACCEPT_JSON));
    if let Some(value) = authorization {
        headers.insert(String::from("authorization"), value.to_owned());
    }
    let response = transport
        .exchange(HttpMethod::Options, &url, &headers, None, false)
        .map_err(IdentityError::Transport)?;
    let read = classify(response.status, &response.text);
    // NOTE: no openEHR spec governs this — our own design; a body that is not
    // JSON is recorded as its text, so the transcript keeps what was answered.
    let body = serde_json::from_str(&response.text).ok().or_else(|| {
        (!response.text.is_empty()).then(|| serde_json::Value::String(response.text.clone()))
    });
    let exchange = RecordedExchange {
        seq: 1,
        row: 0,
        request: RecordedRequest {
            method: String::from("OPTIONS"),
            url,
            headers: recorded_headers(&headers),
            body: None,
        },
        response: RecordedResponse {
            status: response.status.as_u16(),
            headers: recorded_headers(&response.headers),
            body,
        },
    };
    Ok(IdentityExchange { read, exchange })
}

/// Reads one target's identity: the `identify` subcommand's seam.
///
/// The credential `auth` names is the ordinary principal's, read exactly as
/// `test` reads it: from [`USER_ENV`] and [`PASSWORD_ENV`], or [`TOKEN_ENV`],
/// or the file each one's `_FILE` variable names.
///
/// # Errors
/// [`IdentityError::Unsupplied`] when the credential is not supplied or does
/// not read, and [`IdentityError::Transport`] when no client can be built or
/// the exchange cannot be performed.
pub fn identify(
    base_url: &str,
    auth: crate::bench::client::AuthKind,
) -> Result<IdentityRead, IdentityError> {
    let source = CredentialSource::locate(auth, Role::Ordinary)?;
    let inner = crate::exec::transport::HttpTransport::new().map_err(IdentityError::Transport)?;
    let mut transport = CredentialedTransport::new(inner, &source, &CredentialSource::None)?;
    let marker = source.is_supplied().then_some(ORDINARY_MARKER);
    read(&mut transport, base_url, marker).map(|answered| answered.read)
}

/// The identity values the operator supplied on the command line.
#[derive(Debug, Clone, Copy, Default)]
pub struct OperatorIdentity<'a> {
    /// The `--sut-name` value, when supplied.
    pub name: Option<&'a str>,
    /// The `--sut-version` value, when supplied.
    pub version: Option<&'a str>,
}

/// An operator value the server's own value overrode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overridden {
    /// The field, as the record names it.
    pub field: &'static str,
    /// The value the server stated, which the record carries.
    pub server: String,
    /// The value the operator supplied, which the record does not carry.
    pub operator: String,
}

impl Overridden {
    /// The one-line notice a run prints.
    #[must_use]
    pub fn notice(&self) -> String {
        format!(
            "the server states its {} as \"{}\" through System OPTIONS, so the record carries that \
             and not the operator's \"{}\"",
            self.field, self.server, self.operator
        )
    }
}

/// A value that carries something: blank text identifies nothing.
fn stated(value: Option<&str>) -> Option<&str> {
    value.filter(|text| !text.trim().is_empty())
}

/// One mandatory field: the server's value when it stated one, the
/// operator's otherwise.
fn field(
    server: Option<&str>,
    operator: Option<&str>,
    field: &'static str,
    flag: &'static str,
    overridden: &mut Vec<Overridden>,
) -> Result<IdentityField, IdentityError> {
    match (stated(server), stated(operator)) {
        (Some(server), operator) => {
            if let Some(operator) = operator
                && operator != server
            {
                overridden.push(Overridden {
                    field,
                    server: server.to_owned(),
                    operator: operator.to_owned(),
                });
            }
            Ok(IdentityField::server(server))
        }
        (None, Some(operator)) => Ok(IdentityField::operator(operator)),
        (None, None) => Err(IdentityError::Missing { field, flag }),
    }
}

/// Merges what the server stated with what the operator supplied.
///
/// A field the server stated carries the server's value with source
/// `server`; the operator supplies only the fields it did not state. An
/// operator value that differs from the server's is overridden and returned,
/// so the caller can say so. `vendor` and `restapi_specs_version` are
/// recorded only when the server stated them.
///
/// # Errors
/// [`IdentityError::Missing`] when neither side supplies the name or the
/// version.
pub fn merge(
    options: Option<&OptionsDocument>,
    operator: OperatorIdentity<'_>,
) -> Result<(Sut, Vec<Overridden>), IdentityError> {
    let mut overridden = Vec::new();
    let server = options.cloned().unwrap_or_default();
    let name = field(
        server.solution.as_deref(),
        operator.name,
        "name",
        "--sut-name",
        &mut overridden,
    )?;
    let version = field(
        server.solution_version.as_deref(),
        operator.version,
        "version",
        "--sut-version",
        &mut overridden,
    )?;
    let sut = Sut {
        name,
        version,
        vendor: stated(server.vendor.as_deref()).map(IdentityField::server),
        restapi_specs_version: stated(server.restapi_specs_version.as_deref())
            .map(IdentityField::server),
    };
    Ok((sut, overridden))
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "Result-returning tests in the Book ch11 shape, each asserting; \
              clippy offers no allow-in-tests knob for this lint"
)]
mod tests {
    use crate::party::IdentitySource;

    use super::*;

    #[test]
    fn a_full_document_is_offered_with_every_member() {
        let body = r#"{"solution":"openEHRSys","solution_version":"v1.0","vendor":"My-openEHR",
            "restapi_specs_version":"1.1.0","conformance_profile":"STANDARD",
            "endpoints":["/ehr","/query"]}"#;
        let read = classify(StatusCode::OK, body);
        let options = read.options().expect("a full document is offered");
        assert_eq!(options.solution.as_deref(), Some("openEHRSys"));
        assert_eq!(options.solution_version.as_deref(), Some("v1.0"));
        assert_eq!(options.vendor.as_deref(), Some("My-openEHR"));
        assert_eq!(options.restapi_specs_version.as_deref(), Some("1.1.0"));
        assert_eq!(options.conformance_profile.as_deref(), Some("STANDARD"));
        assert_eq!(
            options.endpoints.as_deref(),
            Some(&[String::from("/ehr"), String::from("/query")][..])
        );
    }

    #[test]
    fn an_unknown_member_is_ignored() {
        let read = classify(StatusCode::OK, r#"{"solution":"x","extra":{"a":1}}"#);
        assert_eq!(
            read.options().and_then(|o| o.solution.as_deref()),
            Some("x")
        );
    }

    #[test]
    fn a_wrong_typed_or_null_member_is_not_a_document() {
        for body in [
            r#"{"solution":42}"#,
            r#"{"solution":null}"#,
            r#"{"endpoints":"/ehr"}"#,
        ] {
            assert!(
                matches!(
                    classify(StatusCode::OK, body),
                    IdentityRead::NotOptionsDocument { status: 200, .. }
                ),
                "{body} must not read as an Options document"
            );
        }
    }

    #[test]
    fn an_array_body_is_not_a_document() {
        assert!(matches!(
            classify(StatusCode::OK, "[]"),
            IdentityRead::NotOptionsDocument { .. }
        ));
    }

    #[test]
    fn a_non_2xx_is_not_offered_and_another_2xx_is_not_a_document() {
        assert_eq!(
            classify(StatusCode::NOT_FOUND, "{}"),
            IdentityRead::NotOffered { status: 404 }
        );
        assert!(matches!(
            classify(StatusCode::NO_CONTENT, ""),
            IdentityRead::NotOptionsDocument { status: 204, .. }
        ));
    }

    fn options(name: Option<&str>, version: Option<&str>) -> OptionsDocument {
        OptionsDocument {
            solution: name.map(str::to_owned),
            solution_version: version.map(str::to_owned),
            vendor: Some(String::from("Vendor")),
            ..OptionsDocument::default()
        }
    }

    #[test]
    fn server_only_identity_is_server_sourced() -> Result<(), IdentityError> {
        let document = options(Some("cdr"), Some("1.2"));
        let (sut, overridden) = merge(Some(&document), OperatorIdentity::default())?;
        assert_eq!(sut.name, IdentityField::server("cdr"));
        assert_eq!(sut.version, IdentityField::server("1.2"));
        assert_eq!(sut.vendor, Some(IdentityField::server("Vendor")));
        assert_eq!(sut.restapi_specs_version, None);
        assert!(overridden.is_empty());
        Ok(())
    }

    #[test]
    fn operator_only_identity_is_operator_sourced() -> Result<(), IdentityError> {
        let operator = OperatorIdentity {
            name: Some("cdr"),
            version: Some("1.2"),
        };
        let (sut, overridden) = merge(None, operator)?;
        assert_eq!(sut, Sut::operator("cdr", "1.2"));
        assert!(overridden.is_empty());
        Ok(())
    }

    #[test]
    fn the_operator_fills_only_what_the_server_left_out() -> Result<(), IdentityError> {
        let document = options(Some("cdr"), None);
        let operator = OperatorIdentity {
            name: None,
            version: Some("1.2"),
        };
        let (sut, _) = merge(Some(&document), operator)?;
        assert_eq!(sut.name.source, IdentitySource::Server);
        assert_eq!(sut.version, IdentityField::operator("1.2"));
        Ok(())
    }

    #[test]
    fn agreeing_values_record_the_server_and_raise_nothing() -> Result<(), IdentityError> {
        let document = options(Some("cdr"), Some("1.2"));
        let operator = OperatorIdentity {
            name: Some("cdr"),
            version: Some("1.2"),
        };
        let (sut, overridden) = merge(Some(&document), operator)?;
        assert_eq!(sut.name, IdentityField::server("cdr"));
        assert!(overridden.is_empty());
        Ok(())
    }

    #[test]
    fn a_disagreeing_operator_value_is_overridden_and_named() {
        let document = options(Some("cdr"), Some("1.2"));
        let operator = OperatorIdentity {
            name: Some("typo-cdr"),
            version: Some("1.2"),
        };
        let (sut, overridden) = merge(Some(&document), operator).expect("both fields resolve");
        assert_eq!(sut.name, IdentityField::server("cdr"));
        assert_eq!(overridden.len(), 1, "{overridden:?}");
        let notice = overridden.first().expect("one override");
        assert_eq!(notice.field, "name");
        let line = notice.notice();
        assert!(
            line.contains("\"cdr\"") && line.contains("\"typo-cdr\""),
            "{line}"
        );
    }

    #[test]
    fn nobody_supplying_a_field_refuses() {
        let document = options(Some("cdr"), None);
        let refused = merge(Some(&document), OperatorIdentity::default());
        assert!(
            matches!(
                refused,
                Err(IdentityError::Missing {
                    field: "version",
                    ..
                })
            ),
            "{refused:?}"
        );
        assert!(matches!(
            merge(None, OperatorIdentity::default()),
            Err(IdentityError::Missing { field: "name", .. })
        ));
    }
}
