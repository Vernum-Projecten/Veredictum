// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The CI views of an Arazzo results document: JUnit XML, CTRF 0.2.0 and the
//! Markdown summary. Each is pinned by a reviewed snapshot over one document
//! that carries every status, every party a red row can belong to, a cited
//! upstream report and text that must be escaped; the CTRF view is held to
//! the vendored CTRF 0.2.0 schema, and every view is byte-deterministic.

#![expect(
    clippy::expect_used,
    reason = "test-support helpers (not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them) are panic-idiomatic: a broken fixture must abort the test loudly, Book ch11"
)]

use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use serde_json::Value;
use veredictum::arazzo::document::ArazzoDocument;
use veredictum::arazzo::evaluate::{Plan, rejudge};
use veredictum::arazzo::export::{CTRF_SPEC_VERSION, ctrf, junit, markdown};
use veredictum::arazzo::har::Har;
use veredictum::arazzo::results::{
    Engine, ExchangeRef, Explanation, ResultRow, Results, Skip, SkipReason, Status, Whose, Why,
};
use veredictum::arazzo::rows::Row;
use veredictum::party::InstrumentStatus;
use veredictum::pipeline::Error;
use veredictum::pipeline::export::export_results;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn why(quote: &str, report: Option<u64>) -> Why {
    Why {
        source: "specs/openehr/ITS-REST/specifications/responses/409_EHR_with_id.yaml".to_owned(),
        section: "description".to_owned(),
        quote: quote.to_owned(),
        link: Some("https://example.org/409_EHR_with_id.yaml".to_owned()),
        report: report.and_then(NonZeroU64::new),
    }
}

fn row(case: &str, index: usize, status: Status) -> ResultRow {
    let mut inputs = Row::new();
    inputs.insert("ehr_id".to_owned(), Value::String(format!("id-{index}")));
    ResultRow {
        case: case.to_owned(),
        row: index,
        status,
        inputs,
        generated: Vec::new(),
        skip: None,
        explanation: None,
    }
}

fn red(step: &str, received: &str, whose: Whose, why: Option<Why>) -> Explanation {
    Explanation {
        step: step.to_owned(),
        what: "Create an EHR with the same ehr_id again.".to_owned(),
        why,
        expected: "status 409".to_owned(),
        received: received.to_owned(),
        exchange: Some(ExchangeRef {
            entry: 2,
            method: "PUT".to_owned(),
            url: "http://sut/ehr/3f2a".to_owned(),
            status: 201,
        }),
        whose,
    }
}

/// One document carrying every status and every party a red row belongs to.
fn fixture() -> Results {
    let mut failed = row("ehr/create_twice", 1, Status::Failed);
    failed.explanation = Some(red(
        "create-again",
        "status 201",
        Whose::Server,
        Some(why(
            "Can happen when the supplied `ehr_id` is already used by an existing EHR.",
            Some(42),
        )),
    ));
    let mut advisory = row("ehr/etag", 0, Status::Failed);
    advisory.explanation = Some(red(
        "read",
        "no ETag header",
        Whose::Advisory,
        Some(why("The server SHOULD send an ETag.", None)),
    ));
    let mut errored = row("ehr/escape", 0, Status::Errored);
    let mut instrument = red(
        "create <first> & \"second\"",
        "the body | did not parse: ```\u{1}",
        Whose::Instrument,
        None,
    );
    instrument.exchange = None;
    errored.explanation = Some(instrument);
    let mut skipped = row("ehr/admin_delete", 0, Status::Skipped);
    skipped.skip = Some(Skip {
        reason: SkipReason::NotDeclared,
        why: why("The administrative principal is not declared.", None),
    });
    Results {
        document: "ehr".to_owned(),
        engine: Engine {
            name: "veredictum".to_owned(),
            version: "0.1.7".to_owned(),
        },
        instrument_status: InstrumentStatus::PreApproval,
        base_url: "http://sut/ehrbase/rest/openehr/v1".to_owned(),
        server: None,
        review: Vec::new(),
        rows: vec![
            row("ehr/create_twice", 0, Status::Passed),
            failed,
            advisory,
            errored,
            skipped,
        ],
    }
}

/// The verification pack's failing recording, re-judged with every workflow
/// outside it skipped: a document the evaluator itself wrote.
fn replayed() -> Results {
    let document = ArazzoDocument::load(&root().join("catalogue/cases/ehr/ehr.arazzo.yaml"))
        .expect("the committed document loads");
    let har = Har::read(&root().join("verification-pack/arazzo/fail.har"))
        .expect("the committed recording reads");
    let mut plan = Plan::default();
    for workflow in &document.description().workflows {
        if workflow.workflow_id != "create_ehr-same_ehr_twice-with_id" {
            plan.skips.insert(
                workflow.workflow_id.clone(),
                Skip {
                    reason: SkipReason::NotClaimed,
                    why: why("outside the recording", None),
                },
            );
        }
    }
    rejudge(&document, "ehr", &root(), None, &plan, &har).expect("the recording re-judges")
}

fn assert_ctrf_valid(text: &str) {
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(
            root().join("app/veredictum/src/arazzo/vendor/ctrf-0.2.0.schema.json"),
        )
        .expect("the vendored schema reads"),
    )
    .expect("the vendored schema parses");
    let validator = jsonschema::validator_for(&schema).expect("the CTRF schema compiles");
    let document: Value = serde_json::from_str(text).expect("the CTRF view parses");
    let errors: Vec<String> = validator
        .iter_errors(&document)
        .map(|e| format!("{}: {e}", e.instance_path()))
        .collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn the_junit_view_is_pinned() {
    insta::assert_snapshot!(junit(&fixture()).unwrap());
}

#[test]
fn the_ctrf_view_is_pinned() {
    insta::assert_snapshot!(ctrf(&fixture()).unwrap());
}

#[test]
fn the_markdown_summary_is_pinned() {
    insta::assert_snapshot!(markdown(&fixture()));
}

#[test]
fn the_ctrf_view_validates_against_the_pinned_schema() {
    for results in [fixture(), replayed()] {
        let text = ctrf(&results).unwrap();
        assert_ctrf_valid(&text);
        let document: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(document["specVersion"], CTRF_SPEC_VERSION);
        assert_eq!(
            document["results"]["tests"].as_array().map(Vec::len),
            Some(results.rows.len())
        );
    }
}

#[test]
fn every_status_maps_as_the_architecture_states() {
    let results = fixture();
    let document: Value = serde_json::from_str(&ctrf(&results).unwrap()).unwrap();
    let statuses: Vec<(&str, &str)> = document["results"]["tests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["status"].as_str().unwrap(),
                t["rawStatus"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        statuses,
        [
            ("passed", "passed"),
            ("failed", "failed"),
            ("failed", "failed"),
            ("other", "errored"),
            ("skipped", "skipped"),
        ]
    );
    let summary = &document["results"]["summary"];
    assert_eq!(
        [
            &summary["tests"],
            &summary["passed"],
            &summary["failed"],
            &summary["other"],
            &summary["skipped"],
            &summary["suites"],
        ],
        [5, 1, 2, 1, 1, 4].map(Value::from).each_ref()
    );

    let xml = junit(&results).unwrap();
    let mut reader = Reader::from_str(&xml);
    let mut children = Vec::new();
    let mut open_case = false;
    loop {
        match reader.read_event().unwrap() {
            Event::Eof => break,
            Event::Start(e) if e.name().as_ref() == "testcase" => open_case = true,
            Event::Empty(e) if e.name().as_ref() == "testcase" => children.push("none"),
            Event::Start(e) if open_case => {
                open_case = false;
                children.push(match e.name().as_ref() {
                    "failure" => "failure",
                    "error" => "error",
                    "skipped" => "skipped",
                    other => panic!("unexpected {other}"),
                });
            }
            _ => {}
        }
    }
    assert_eq!(children, ["none", "failure", "failure", "error", "skipped"]);
}

#[test]
fn a_red_row_carries_its_four_part_explanation_in_every_view() {
    let results = fixture();
    let failed = results.rows.get(1).unwrap();
    let explanation = failed.to_string();
    assert!(explanation.contains("What     "), "{explanation}");
    assert!(explanation.contains("Why      "), "{explanation}");
    assert!(explanation.contains("Expected "), "{explanation}");
    assert!(explanation.contains("Whose    "), "{explanation}");

    let document: Value = serde_json::from_str(&ctrf(&results).unwrap()).unwrap();
    assert_eq!(
        document["results"]["tests"][1]["message"].as_str(),
        Some(explanation.as_str())
    );
    assert_eq!(
        document["results"]["tests"][1]["extra"]["report"].as_str(),
        Some(concat!(env!("CARGO_PKG_REPOSITORY"), "/issues/42"))
    );

    let xml = junit(&results).unwrap();
    let mut reader = Reader::from_str(&xml);
    let mut inside = false;
    let mut text = String::new();
    loop {
        match reader.read_event().unwrap() {
            Event::Eof => break,
            Event::Start(e) if e.name().as_ref() == "failure" => inside = true,
            Event::End(e) if inside && e.name().as_ref() == "failure" => break,
            Event::Text(t) if inside => text.push_str(&t.xml_content(XmlVersion::Explicit1_0)),
            Event::GeneralRef(r) if inside => match r.resolve_char_ref().unwrap() {
                Some(c) => text.push(c),
                None => text.push_str(resolve_predefined_entity(&r).unwrap()),
            },
            _ => {}
        }
    }
    assert_eq!(text, explanation);

    assert!(markdown(&results).contains(&explanation));
}

#[test]
fn every_view_is_byte_deterministic() {
    let results = replayed();
    assert_eq!(junit(&results).unwrap(), junit(&replayed()).unwrap());
    assert_eq!(ctrf(&results).unwrap(), ctrf(&replayed()).unwrap());
    assert_eq!(markdown(&results), markdown(&replayed()));
}

#[test]
fn the_seam_reads_a_results_document_and_refuses_another_shape() {
    let dir = assert_fs::TempDir::new().unwrap();
    let path = dir.path().join("results.json");
    std::fs::write(&path, fixture().to_text().unwrap()).unwrap();
    let views = export_results(&path).unwrap();
    assert_eq!(views.junit, junit(&fixture()).unwrap());
    assert_eq!(views.ctrf, ctrf(&fixture()).unwrap());
    assert_eq!(views.markdown, markdown(&fixture()));

    std::fs::write(&path, "{\"outcomes\": []}").unwrap();
    assert!(matches!(export_results(&path), Err(Error::Parse { .. })));
}

#[test]
fn the_command_appends_the_summary_to_the_job_summary_file() {
    let dir = assert_fs::TempDir::new().unwrap();
    let results = dir.path().join("results.json");
    std::fs::write(&results, fixture().to_text().unwrap()).unwrap();
    let summary = dir.path().join("step-summary.md");
    std::fs::write(&summary, "an earlier step\n").unwrap();
    let junit_path = dir.path().join("out/junit.xml");
    let ctrf_path = dir.path().join("out/ctrf.json");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_veredictum"))
        .arg("export")
        .arg("--results")
        .arg(&results)
        .arg("--junit")
        .arg(&junit_path)
        .arg("--ctrf")
        .arg(&ctrf_path)
        .env("GITHUB_STEP_SUMMARY", &summary)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert_eq!(
        std::fs::read_to_string(&summary).unwrap(),
        format!("an earlier step\n{}", markdown(&fixture()))
    );
    assert_eq!(
        std::fs::read_to_string(&junit_path).unwrap(),
        junit(&fixture()).unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(&ctrf_path).unwrap(),
        ctrf(&fixture()).unwrap()
    );
}

#[test]
fn without_a_summary_file_the_command_prints_the_summary() {
    let dir = assert_fs::TempDir::new().unwrap();
    let results = dir.path().join("results.json");
    std::fs::write(&results, fixture().to_text().unwrap()).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_veredictum"))
        .arg("export")
        .arg("--results")
        .arg(&results)
        .env_remove("GITHUB_STEP_SUMMARY")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        markdown(&fixture())
    );
}
