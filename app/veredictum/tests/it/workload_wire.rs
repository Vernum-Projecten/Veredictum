// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The workload's new wire operations against a fake SUT.
//!
//! Each test sends one [`WireRequest`] over a real socket and lets the mock
//! answer only when the request carries what the released operation defines:
//! the EHR lookup by subject (ITS-REST `operations/ehr_get_by_subject.yaml`),
//! the four query executions (`query_execute_adhoc_query_body.yaml`,
//! `query_execute_adhoc_query.yaml`, `query_execute_stored_query.yaml`,
//! `query_execute_stored_query_body.yaml`) and the stored-query registration
//! (`definition_query_version_store.yaml`).

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test-support helpers (not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them) are panic-idiomatic: a broken harness must abort the test loudly, Book ch11"
)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::json;
use veredictum::ids::QueryName;
use veredictum::workload::QueryClass;
use veredictum::workload::load::{LoadContext, load_file};
use veredictum::workload::ops::QueryTransport;
use veredictum::workload::wire::{
    ParamValue, WireRequest, ehr_read_by_subject, query_execute, stored_query_store,
};
use wiremock::matchers::{body_json, body_string, header, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use crate::fake_sut::FakeSut;

const EHR: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

fn class(name: &str) -> (QueryName, QueryClass) {
    let repo = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let ctx = LoadContext::from_roots(&repo.join("perf"), &repo.join("catalogue"))
        .expect("the context loads");
    let loaded = load_file(&repo.join("perf/workloads/clinical-mix.yaml"), &ctx)
        .unwrap_or_else(|e| panic!("{e}"));
    let name = QueryName::parse(name).expect("a query name");
    let class = loaded
        .workload
        .queries
        .get(&name)
        .expect("declared")
        .clone();
    (name, class)
}

/// Sends `request` to the fake SUT and returns the status.
fn send(sut: &FakeSut, request: &WireRequest) -> u16 {
    let client = reqwest::blocking::Client::new();
    let mut builder = client.request(
        request.method().expect("a method"),
        request.url(&sut.base_url()).expect("a URL"),
    );
    for (name, value) in &request.headers {
        builder = builder.header(*name, value);
    }
    if let Some(body) = &request.body {
        builder = builder
            .header("Content-Type", body.content_type)
            .body(body.bytes.clone());
    }
    builder
        .send()
        .expect("the fake SUT answers")
        .status()
        .as_u16()
}

#[test]
fn ehr_read_by_subject_sends_both_subject_parameters() {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/ehr"))
            .and(query_param("subject_id", "ins01"))
            .and(query_param("subject_namespace", "examples"))
            .respond_with(ResponseTemplate::new(200)),
    );
    let request = ehr_read_by_subject("ins01", "examples").expect("released");
    assert_eq!(request.operation.operation_id, "ehr_get_by_subject");
    assert_eq!(send(&sut, &request), 200);
}

#[test]
fn an_adhoc_post_carries_q_fetch_and_parameters_and_scopes_by_header() {
    let (name, class) = class("patient_summary");
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/query/aql"))
            .and(header("openehr-ehr-id", EHR))
            .and(body_json(json!({
                "q": class.text,
                "fetch": 20,
                "query_parameters": { "ehr_uid": EHR }
            })))
            .respond_with(ResponseTemplate::new(200)),
    );
    let values = BTreeMap::from([("ehr_uid".to_owned(), ParamValue::Text(EHR.to_owned()))]);
    let request = query_execute(&name, &class, QueryTransport::AdhocPost, &values, Some(EHR))
        .expect("builds");
    assert_eq!(
        request.operation.operation_id,
        "query_execute_adhoc_query_body"
    );
    assert_eq!(send(&sut, &request), 200);
}

#[test]
fn an_adhoc_get_carries_q_and_each_parameter_by_name() {
    let (name, class) = class("aql_point_lookup");
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/query/aql"))
            .and(query_param("q", class.text.as_str()))
            .and(query_param("ehr_id", EHR))
            .and(query_param("ehr_uid", EHR))
            .and(query_param("uid", "c1::sut::1"))
            .respond_with(ResponseTemplate::new(200)),
    );
    let values = BTreeMap::from([
        ("ehr_uid".to_owned(), ParamValue::Text(EHR.to_owned())),
        ("uid".to_owned(), ParamValue::Text("c1::sut::1".to_owned())),
    ]);
    let request =
        query_execute(&name, &class, QueryTransport::AdhocGet, &values, Some(EHR)).expect("builds");
    assert_eq!(request.operation.operation_id, "query_execute_adhoc_query");
    assert_eq!(send(&sut, &request), 200);
}

#[test]
fn a_stored_get_addresses_the_qualified_name_with_no_scope_for_a_population() {
    let (name, class) = class("ward_board");
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/query/org.veredictum.perf::ward_board"))
            .and(query_param("bed_1", "e1"))
            .and(query_param("fetch", "50"))
            .respond_with(ResponseTemplate::new(200)),
    );
    let values: BTreeMap<String, ParamValue> = (1..=8)
        .map(|i| (format!("bed_{i}"), ParamValue::Text(format!("e{i}"))))
        .collect();
    let request =
        query_execute(&name, &class, QueryTransport::StoredGet, &values, None).expect("builds");
    assert_eq!(request.operation.operation_id, "query_execute_stored_query");
    assert!(
        !request.query.iter().any(|(k, _)| k == "ehr_id"),
        "a population query carries no EHR scope"
    );
    assert_eq!(send(&sut, &request), 200);
}

#[test]
fn a_stored_post_carries_the_required_query_body() {
    let (name, class) = class("ward_board");
    let sut = FakeSut::start();
    let parameters: serde_json::Map<String, serde_json::Value> = (1..=8)
        .map(|i| (format!("bed_{i}"), json!(format!("e{i}"))))
        .collect();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/query/org.veredictum.perf::ward_board"))
            .and(body_json(json!({
                "offset": 0,
                "fetch": 50,
                "query_parameters": parameters
            })))
            .respond_with(ResponseTemplate::new(200)),
    );
    let values: BTreeMap<String, ParamValue> = (1..=8)
        .map(|i| (format!("bed_{i}"), ParamValue::Text(format!("e{i}"))))
        .collect();
    let request =
        query_execute(&name, &class, QueryTransport::StoredPost, &values, None).expect("builds");
    assert_eq!(
        request.operation.operation_id,
        "query_execute_stored_query_body"
    );
    assert_eq!(send(&sut, &request), 200);
}

#[test]
fn an_integer_parameter_travels_as_a_json_number() {
    let (name, class) = class("cohort_count");
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/query/aql"))
            .and(body_json(json!({
                "q": class.text,
                "query_parameters": { "systolic": 140 }
            })))
            .respond_with(ResponseTemplate::new(200)),
    );
    let values = BTreeMap::from([("systolic".to_owned(), ParamValue::Integer(140))]);
    let request =
        query_execute(&name, &class, QueryTransport::AdhocPost, &values, None).expect("builds");
    assert_eq!(send(&sut, &request), 200);
}

#[test]
fn a_stored_query_is_registered_as_text_at_its_version() {
    let (name, class) = class("patient_summary");
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("PUT"))
            .and(path(
                "/definition/query/org.veredictum.perf::patient_summary/1.0.0",
            ))
            .and(header("Content-Type", "text/plain"))
            .and(body_string(class.text.clone()))
            .respond_with(ResponseTemplate::new(200)),
    );
    let request = stored_query_store(&name, &class);
    assert_eq!(send(&sut, &request), 200);
}

#[test]
fn an_unbound_parameter_and_a_missing_ehr_are_refused_before_sending() {
    let (name, class) = class("patient_summary");
    let none = BTreeMap::new();
    assert!(query_execute(&name, &class, QueryTransport::AdhocPost, &none, Some(EHR)).is_err());
    let values = BTreeMap::from([("ehr_uid".to_owned(), ParamValue::Text(EHR.to_owned()))]);
    assert!(query_execute(&name, &class, QueryTransport::AdhocPost, &values, None).is_err());
}
