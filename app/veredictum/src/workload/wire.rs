// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The wire form of the subject lookup and the query family.
//!
//! These are the operations a front end sends that the earlier instruments
//! lacked: the EHR lookup by subject, and the four released query executions
//! with the stored-query registration they depend on.
//!
//! A [`WireRequest`] is client-agnostic: a method, a path, query pairs,
//! headers and an optional body, so a blocking and an async client send the
//! same bytes. Each builder cites the released operation it realizes.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::ids::QueryName;
use crate::workload::QueryClass;
use crate::workload::QueryScope;
use crate::workload::ops::{QueryTransport, ReleasedOperation, STORED_QUERY_STORE, WorkloadOp};

/// The namespace every stored query class is registered under, a reverse
/// domain name as ITS-REST `docs/query/Qualified_query_name.md` recommends.
pub const STORED_QUERY_NAMESPACE: &str = "org.veredictum.perf";

/// The version every stored query class is registered at (SEMVER, ITS-REST
/// `docs/query/Qualified_query_name.md`).
pub const STORED_QUERY_VERSION: &str = "1.0.0";

/// Query parameter names the query executions use for themselves, which a
/// query class may not name (ITS-REST `docs/query/Request.md` §Common Headers
/// and Query Parameters, and `parameters/query/q.yaml`).
pub const RESERVED_PARAMETERS: &[&str] = &["q", "ehr_id", "offset", "fetch"];

/// One value bound to a query parameter on one arrival.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum ParamValue {
    /// A string: an identifier or a text value.
    Text(String),
    /// An integer.
    Integer(i64),
}

impl ParamValue {
    /// Returns the value as it travels in a URL query pair.
    #[must_use]
    pub fn as_query_value(&self) -> String {
        match self {
            ParamValue::Text(text) => text.clone(),
            ParamValue::Integer(n) => n.to_string(),
        }
    }
}

/// A request body and its media type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireBody {
    /// The `Content-Type`.
    pub content_type: &'static str,
    /// The bytes.
    pub bytes: Vec<u8>,
}

/// One request, ready for any HTTP client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireRequest {
    /// The released operation it realizes.
    pub operation: ReleasedOperation,
    /// The path, relative to the service base URL, with every template
    /// variable bound.
    pub path: String,
    /// The URL query pairs, in order.
    pub query: Vec<(String, String)>,
    /// The request headers beyond the client's own.
    pub headers: Vec<(&'static str, String)>,
    /// The body, for a method that carries one.
    pub body: Option<WireBody>,
}

impl WireRequest {
    /// Returns the request's method.
    ///
    /// # Errors
    /// [`WireError::Method`] for a method token outside RFC 9110's grammar, a
    /// defect in [`crate::workload::ops`].
    pub fn method(&self) -> Result<reqwest::Method, WireError> {
        reqwest::Method::from_bytes(self.operation.method.as_bytes())
            .map_err(|_invalid| WireError::Method(self.operation.method))
    }

    /// Returns the absolute URL against the service base URL.
    ///
    /// # Errors
    /// [`WireError::Url`] when the base and the path do not form a URL.
    pub fn url(&self, base: &str) -> Result<reqwest::Url, WireError> {
        let mut url = reqwest::Url::parse(&format!("{}{}", base.trim_end_matches('/'), self.path))
            .map_err(|e| WireError::Url(e.to_string()))?;
        if !self.query.is_empty() {
            url.query_pairs_mut().extend_pairs(&self.query);
        }
        Ok(url)
    }
}

/// Why a request cannot be built.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WireError {
    /// A parameter the class declares has no value on this arrival.
    #[error("query parameter {0} has no value")]
    Unbound(String),
    /// A single-subject class was sent without its EHR.
    #[error("a single-subject query is sent with its EHR id")]
    NoEhr,
    /// The stored POST body requires `fetch` and the class declares none.
    #[error("a stored query POST carries fetch, and the class declares none")]
    NoFetch,
    /// The body does not serialize.
    #[error("the body does not serialize: {0}")]
    Body(String),
    /// The method token is not a method.
    #[error("{0} is not an HTTP method")]
    Method(&'static str),
    /// The URL does not form.
    #[error("the URL does not form: {0}")]
    Url(String),
}

/// Returns the qualified name a stored query class is registered under.
#[must_use]
pub fn stored_query_name(name: &QueryName) -> String {
    format!("{STORED_QUERY_NAMESPACE}::{name}")
}

/// Builds `GET /ehr?subject_id=…&subject_namespace=…`.
///
/// ITS-REST `specifications/operations/ehr_get_by_subject.yaml`: "Retrieve
/// the EHR with the specified `subject_id` and `subject_namespace`", both
/// required query parameters (`parameters/query/subject_id.yaml`,
/// `subject_namespace.yaml`).
#[must_use]
pub fn ehr_read_by_subject(subject_id: &str, subject_namespace: &str) -> Option<WireRequest> {
    let operation = WorkloadOp::EhrReadBySubject.released()?;
    Some(WireRequest {
        operation,
        path: operation.path.to_owned(),
        query: vec![
            ("subject_id".to_owned(), subject_id.to_owned()),
            ("subject_namespace".to_owned(), subject_namespace.to_owned()),
        ],
        headers: vec![("Accept", "application/json".to_owned())],
        body: None,
    })
}

/// The `AdhocQueryExecute` body (ITS-REST
/// `specifications/schemas/query/AdhocQueryExecute.yaml`).
#[derive(Serialize)]
struct AdhocBody<'a> {
    q: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    fetch: Option<u32>,
    query_parameters: &'a BTreeMap<String, ParamValue>,
}

/// The stored-query `Query` body, whose `offset`, `fetch` and
/// `query_parameters` are all required (ITS-REST
/// `specifications/schemas/query/Query.yaml`).
#[derive(Serialize)]
struct StoredBody<'a> {
    offset: u32,
    fetch: u32,
    query_parameters: &'a BTreeMap<String, ParamValue>,
}

fn json_body<T: Serialize>(value: &T) -> Result<WireBody, WireError> {
    Ok(WireBody {
        content_type: "application/json",
        bytes: serde_json::to_vec(value).map_err(|e| WireError::Body(e.to_string()))?,
    })
}

/// Builds one arrival of a query class over `transport`.
///
/// `values` binds every declared parameter. A single-subject class carries
/// its EHR as the `ehr_id` query parameter on a `GET` and as the
/// `openehr-ehr-id` header on a `POST`; a population class carries neither
/// (ITS-REST `docs/query/Request.md` §About the `ehr_id` parameter: it "SHOULD
/// be supplied by clients when executing single EHR queries" and "MUST NOT be
/// supplied for 'population queries'").
///
/// # Errors
/// [`WireError`] when a parameter is unbound, a single-subject class has no
/// EHR, a stored `POST` has no `fetch`, or the body does not serialize.
pub fn query_execute(
    name: &QueryName,
    class: &QueryClass,
    transport: QueryTransport,
    values: &BTreeMap<String, ParamValue>,
    ehr_id: Option<&str>,
) -> Result<WireRequest, WireError> {
    if let Some(missing) = class.parameters.keys().find(|p| !values.contains_key(*p)) {
        return Err(WireError::Unbound(missing.clone()));
    }
    let bound: BTreeMap<String, ParamValue> = values
        .iter()
        .filter(|(k, _)| class.parameters.contains_key(*k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let scope = match class.scope {
        QueryScope::SingleSubject => Some(ehr_id.ok_or(WireError::NoEhr)?),
        QueryScope::Population => None,
    };
    let operation = transport.released();
    let stored_path = || format!("/query/{}", stored_query_name(name));
    let mut headers = vec![("Accept", "application/json".to_owned())];
    let mut query = Vec::new();
    let (path, body) = match transport {
        QueryTransport::AdhocPost => (
            operation.path.to_owned(),
            Some(json_body(&AdhocBody {
                q: &class.text,
                fetch: class.fetch,
                query_parameters: &bound,
            })?),
        ),
        QueryTransport::StoredPost => {
            let fetch = class.fetch.ok_or(WireError::NoFetch)?;
            (
                stored_path(),
                Some(json_body(&StoredBody {
                    offset: 0,
                    fetch,
                    query_parameters: &bound,
                })?),
            )
        }
        QueryTransport::AdhocGet => {
            query.push(("q".to_owned(), class.text.clone()));
            (operation.path.to_owned(), None)
        }
        QueryTransport::StoredGet => (stored_path(), None),
    };
    if body.is_some() {
        if let Some(ehr) = scope {
            headers.push(("openehr-ehr-id", ehr.to_owned()));
        }
    } else {
        if let Some(ehr) = scope {
            query.push(("ehr_id".to_owned(), ehr.to_owned()));
        }
        if let Some(fetch) = class.fetch {
            query.push(("fetch".to_owned(), fetch.to_string()));
        }
        query.extend(bound.iter().map(|(k, v)| (k.clone(), v.as_query_value())));
    }
    Ok(WireRequest {
        operation,
        path,
        query,
        headers,
        body,
    })
}

/// Builds the registration of a stored query class.
///
/// The request is `PUT /definition/query/{qualified_query_name}/{version}`
/// with the AQL text as a `text/plain` body (ITS-REST
/// `specifications/operations/definition_query_version_store.yaml`).
#[must_use]
pub fn stored_query_store(name: &QueryName, class: &QueryClass) -> WireRequest {
    WireRequest {
        operation: STORED_QUERY_STORE,
        path: format!(
            "/definition/query/{}/{STORED_QUERY_VERSION}",
            stored_query_name(name)
        ),
        query: Vec::new(),
        headers: vec![("Accept", "application/json".to_owned())],
        body: Some(WireBody {
            content_type: "text/plain",
            bytes: class.text.clone().into_bytes(),
        }),
    }
}
