// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! `x-veredictum-derive` over a real socket: a create answers a `version_uid`,
//! and the next request is addressed by the `object_id` part the step
//! derives from it (BASE `object_version_id.adoc`, lexical form
//! `object_id::creating_system_id::version_tree_id`).

use assert_fs::prelude::*;
use veredictum::arazzo::document::ArazzoDocument;
use veredictum::arazzo::evaluate::{Plan, RunConfig, run_document};
use veredictum::arazzo::results::Status;
use veredictum::exec::transport::HttpTransport;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::fake_sut::FakeSut;

/// A test body propagates plumbing failures with `?`
/// (<https://doc.rust-lang.org/book/ch11-01-writing-tests.html>).
type Fallible = Result<(), Box<dyn std::error::Error>>;

const OAS: &str = "openapi: 3.0.3
info: { title: t, version: '1' }
paths:
  /ehr/{ehr_id}/composition:
    post:
      operationId: composition_create
      parameters: [{ name: ehr_id, in: path, required: true, schema: { type: string } }]
      responses: { '201': { description: created } }
  /ehr/{ehr_id}/versioned_composition/{versioned_object_uid}:
    get:
      operationId: versioned_composition_get
      parameters:
        - { name: ehr_id, in: path, required: true, schema: { type: string } }
        - { name: versioned_object_uid, in: path, required: true, schema: { type: string } }
      responses: { '200': { description: retrieved } }
";

const DOC: &str = "arazzo: 1.1.0
info: { title: t, version: '1' }
sourceDescriptions:
  - { name: ehr, url: ehr.yaml, type: openapi }
workflows:
  - workflowId: create_then_container
    x-veredictum-cite: { source: s, section: d, quote: q }
    steps:
      - stepId: commit
        operationId: $sourceDescriptions.ehr.composition_create
        parameters:
          - { name: ehr_id, in: path, value: 7d44b88c }
          - { name: Prefer, in: header, value: return=identifier }
        requestBody: { contentType: application/json, payload: { _type: COMPOSITION } }
        successCriteria: [{ condition: $statusCode == 201 }]
        outputs: { version_uid: $response.body#/uid }
        x-veredictum-derive:
          versioned_object_uid: { function: object_id, of: version_uid }
      - stepId: container
        operationId: $sourceDescriptions.ehr.versioned_composition_get
        parameters:
          - { name: ehr_id, in: path, value: 7d44b88c }
          - { name: versioned_object_uid, in: path, value: $steps.commit.outputs.versioned_object_uid }
        successCriteria: [{ condition: $statusCode == 200 }]
";

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 test shape: plumbing propagates with `?`, and the assertions are how the test fails"
)]
fn a_request_is_addressed_by_the_object_id_a_step_derives() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/v1/ehr/7d44b88c/composition"))
            .respond_with(ResponseTemplate::new(201).set_body_raw(
                r#"{"uid":"8849182c-82ad-4088-a07f-48ead4180515::sys.example.org::1"}"#,
                "application/json",
            )),
    );
    sut.mount(
        Mock::given(method("GET"))
            .and(path(
                "/v1/ehr/7d44b88c/versioned_composition/8849182c-82ad-4088-a07f-48ead4180515",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_raw("{}", "application/json")),
    );
    let dir = assert_fs::TempDir::new()?;
    dir.child("ehr.yaml").write_str(OAS)?;
    dir.child("doc.arazzo.yaml").write_str(DOC)?;
    let document = ArazzoDocument::load(&dir.path().join("doc.arazzo.yaml"))?;
    let mut transport = HttpTransport::new()?;
    let config = RunConfig::new(&format!("{}/v1", sut.base_url()));
    let (results, _) = run_document(
        &document,
        "doc",
        config,
        dir.path(),
        &Plan::default(),
        &mut transport,
    )?;
    let rows: Vec<_> = results.rows.iter().collect();
    assert_eq!(rows.len(), 1);
    let row = rows.first().ok_or("one row")?;
    assert_eq!(row.status, Status::Passed, "{row}");
    let requests = sut.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests.get(1).map(|r| r.url.path().to_owned()),
        Some(
            "/v1/ehr/7d44b88c/versioned_composition/8849182c-82ad-4088-a07f-48ead4180515"
                .to_owned()
        )
    );
    Ok(())
}
