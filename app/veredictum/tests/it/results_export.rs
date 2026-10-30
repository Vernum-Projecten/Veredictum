// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The views of an Arazzo results document: JUnit XML, CTRF 0.2.0, the
//! Markdown summary and the terminal view. Each is pinned by a reviewed
//! snapshot over one document that carries every status, every party a red
//! row can belong to, a cited upstream report and text that must be escaped,
//! and the Markdown and terminal views again over a document with an outline;
//! the CTRF view is held to the vendored CTRF 0.2.0 schema, no view counts an
//! advisory row as failed, and every view is byte-deterministic.

#![expect(
    clippy::expect_used,
    reason = "test-support helpers (not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them) are panic-idiomatic: a broken fixture must abort the test loudly, Book ch11"
)]

use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use serde_json::Value;
use veredictum::arazzo::document::ArazzoDocument;
use veredictum::arazzo::evaluate::{Plan, rejudge};
use veredictum::arazzo::export::{ADVISORY_LINE, CTRF_SPEC_VERSION, ctrf, junit, markdown};
use veredictum::arazzo::har::Har;
use veredictum::arazzo::results::{
    Engine, ExchangeRef, Explanation, ResultRow, Results, Skip, SkipReason, Status, Whose, Why,
};
use veredictum::arazzo::rows::Row;
use veredictum::arazzo::structure::{
    ArmRef, ChapterDecl, FolderDecl, Formalism, Outline, OutlineArm, OutlineOption, OutlineReport,
    Place, ReportState, SectionDecl, Standing, Theme, Tier,
};
use veredictum::arazzo::terminal::{ShowOptions, render};
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
        outline: None,
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
            ("other", "advisory"),
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
        [5, 1, 1, 2, 1, 4].map(Value::from).each_ref()
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
                    "system-out" => "system-out",
                    other => panic!("unexpected {other}"),
                });
            }
            _ => {}
        }
    }
    assert_eq!(
        children,
        ["none", "failure", "system-out", "error", "skipped"]
    );
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

    let summary = markdown(&results);
    let why = failed.explanation.as_ref().unwrap();
    for part in [
        "<summary><b>Failed</b>: <code>ehr/create_twice</code> row 1, unplaced</summary>"
            .to_owned(),
        format!("| **Step** | `{}` |", why.step),
        format!(
            "| **What was checked** | {} |",
            why.what.replace('_', "\\_")
        ),
        format!("| **Expected** | {} |", why.expected),
        format!("| **Received** | {} |", why.received),
        "| **Exchange** | `PUT http://sut/ehr/3f2a` answered 201, recording entry 2 |".to_owned(),
        "| **Whose problem** | The server. A failed row counts against the claimed capability. |"
            .to_owned(),
        "> Can happen when the supplied \\`ehr\\_id\\` is already used by an existing EHR."
            .to_owned(),
        format!("[#42]({}/issues/42)", env!("CARGO_PKG_REPOSITORY")),
    ] {
        assert!(summary.contains(&part), "missing {part}\n{summary}");
    }
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

#[test]
fn an_advisory_row_counts_as_failed_in_no_view() {
    let results = fixture();
    let xml = junit(&results).unwrap();
    let mut reader = Reader::from_str(&xml);
    let mut failures = Vec::new();
    loop {
        match reader.read_event().unwrap() {
            Event::Eof => break,
            Event::Start(e) if e.name().as_ref() == "testsuite" => {
                let name = e.try_get_attribute("name").unwrap().unwrap();
                let count = e.try_get_attribute("failures").unwrap().unwrap();
                failures.push((name.value.into_owned(), count.value.into_owned()));
            }
            _ => {}
        }
    }
    assert!(
        failures.contains(&("ehr/etag".to_owned(), "0".to_owned())),
        "{failures:?}"
    );
    assert!(
        xml.contains("<testsuites name=\"ehr\" tests=\"5\" failures=\"1\""),
        "{xml}"
    );
    assert!(xml.contains(ADVISORY_LINE), "{xml}");

    let document: Value = serde_json::from_str(&ctrf(&results).unwrap()).unwrap();
    assert_eq!(document["results"]["summary"]["failed"], 1);
    let etag = &document["results"]["tests"][2];
    assert_eq!(etag["extra"]["case"], "ehr/etag");
    assert_eq!(etag["status"], "other");
    assert_eq!(etag["rawStatus"], "advisory");

    let summary = markdown(&results);
    assert!(
        summary.contains("| **Every row** | **1** | **1** | **1** | **1** | **1** | **5** |"),
        "one failed and one advisory row, counted apart: {summary}"
    );
    assert!(summary.contains("## Advisory rows"), "{summary}");
    assert!(
        summary.contains(
            "| `ehr/etag row 0` | `read` | specs/openehr/ITS-REST/specifications/responses/409\\_EHR\\_with\\_id.yaml, description |"
        ),
        "{summary}"
    );
    assert!(
        summary.contains("<summary><b>Advisory</b>: <code>ehr/etag</code>"),
        "{summary}"
    );
}

fn issue(number: u64) -> NonZeroU64 {
    NonZeroU64::new(number).expect("a report number is non-zero")
}

fn place(folder: &str, standing: Standing) -> Place {
    Place {
        chapter: "ehr".to_owned(),
        folder: folder.to_owned(),
        standing,
        trial: false,
        reports: Vec::new(),
        formalism: None,
    }
}

fn base(folder: &str, section: &str) -> Place {
    place(
        folder,
        Standing::Base {
            section: section.to_owned(),
        },
    )
}

fn on_arm(folder: &str, option: &str, arm: &str) -> Place {
    place(
        folder,
        Standing::Option {
            arms: vec![ArmRef {
                option: option.to_owned(),
                arm: arm.to_owned(),
            }],
        },
    )
}

fn arm(name: &str, selected: bool, documents: &[&str]) -> OutlineArm {
    OutlineArm {
        arm: name.to_owned(),
        selected,
        documents: documents.iter().map(|d| (*d).to_owned()).collect(),
    }
}

fn option(
    name: &str,
    placement: (Theme, Tier),
    report: Option<u64>,
    arms: Vec<OutlineArm>,
) -> OutlineOption {
    OutlineOption {
        option: name.to_owned(),
        description: format!("Whether the server supports {name}."),
        theme: placement.0,
        tier: placement.1,
        report: report.and_then(NonZeroU64::new),
        folders: vec!["composition".to_owned()],
        arms,
    }
}

fn section(key: &str, title: &str) -> SectionDecl {
    SectionDecl {
        section: key.to_owned(),
        title: title.to_owned(),
    }
}

fn folder(key: &str, title: &str, formalism: bool, sections: Vec<SectionDecl>) -> FolderDecl {
    FolderDecl {
        folder: key.to_owned(),
        title: title.to_owned(),
        formalism,
        sections,
    }
}

fn failed_row(case: &str, step: &str, whose: Whose, report: Option<u64>) -> ResultRow {
    let mut failed = row(case, 0, Status::Failed);
    failed.explanation = Some(red(
        step,
        "status 201",
        whose,
        Some(why("The server refuses it.", report)),
    ));
    failed
}

/// One catalogue run with an outline: BASE across two bands and an empty
/// one, a band split by formalism, a TRIAL case, a reported case, a selected
/// option, an unselected one, a recommended option whose selected arm runs
/// nothing, a spec-boundary option and a case placed nowhere.
fn outlined() -> Results {
    let mut results = fixture();
    "catalogue".clone_into(&mut results.document);
    let mut create_failed = failed_row("composition/create", "create", Whose::Server, Some(42));
    create_failed.row = 1;
    let mut flat = row("flat/create", 0, Status::Skipped);
    flat.skip = Some(Skip {
        reason: SkipReason::OptionNotSelected,
        why: why("The statement selects another arm.", None),
    });
    let mut boundary = row("ehr_status_flat/read", 0, Status::Errored);
    boundary.explanation = Some(red(
        "read",
        "the body did not parse",
        Whose::Instrument,
        Some(why("EHR_STATUS has no flat form.", Some(352))),
    ));
    results.rows = vec![
        row("composition/create", 0, Status::Passed),
        create_failed,
        failed_row("composition/read", "read", Whose::Advisory, None),
        row("composition/read", 1, Status::Passed),
        failed_row("query/stored", "validate", Whose::Server, None),
        row("content/adl14_cardinality", 0, Status::Passed),
        failed_row("content/adl2_cardinality", "commit", Whose::Server, None),
        row("composition_xml/read", 0, Status::Passed),
        flat,
        boundary,
        row("stray/case", 0, Status::Passed),
    ];
    results.outline = Some(outline());
    results
}

/// Where [`outline`] places each case.
fn places() -> BTreeMap<String, Place> {
    let mut create = base("composition", "create");
    create.reports = vec![issue(42)];
    let mut stored = base("composition", "read");
    stored.trial = true;
    let mut adl14 = base("content", "structure");
    adl14.formalism = Some(Formalism::Adl14);
    let mut adl2 = base("content", "structure");
    adl2.formalism = Some(Formalism::Adl2);
    let mut boundary_place = on_arm("composition", "ehr-status-flat", "flat-served");
    boundary_place.reports = vec![issue(352)];
    BTreeMap::from([
        ("composition/create".to_owned(), create),
        ("composition/read".to_owned(), base("composition", "read")),
        ("query/stored".to_owned(), stored),
        ("content/adl14_cardinality".to_owned(), adl14),
        ("content/adl2_cardinality".to_owned(), adl2),
        (
            "composition_xml/read".to_owned(),
            on_arm("composition", "composition-xml-read", "xml-supported"),
        ),
        (
            "flat/create".to_owned(),
            on_arm("composition", "flat-format", "flat-supported"),
        ),
        ("ehr_status_flat/read".to_owned(), boundary_place),
    ])
}

/// The outline [`outlined`] carries.
fn outline() -> Outline {
    Outline {
        chapters: vec![ChapterDecl {
            chapter: "ehr".to_owned(),
            title: "EHR".to_owned(),
            folders: vec![
                folder(
                    "composition",
                    "COMPOSITION",
                    false,
                    vec![section("create", "Create"), section("read", "Read")],
                ),
                folder(
                    "content",
                    "Semantic validation",
                    true,
                    vec![section("structure", "Structure and cardinality")],
                ),
                folder(
                    "directory",
                    "DIRECTORY",
                    false,
                    vec![section("create", "Create")],
                ),
            ],
        }],
        options: vec![
            option(
                "composition-xml-read",
                (Theme::CanonicalXml, Tier::Options),
                None,
                vec![
                    arm("xml-supported", true, &["composition_xml"]),
                    arm("xml-unsupported", false, &[]),
                ],
            ),
            option(
                "flat-format",
                (Theme::SimplifiedFormats, Tier::Options),
                None,
                vec![
                    arm("flat-supported", false, &["flat"]),
                    arm("flat-unsupported", false, &[]),
                ],
            ),
            option(
                "etag-sent",
                (Theme::LeftOpen, Tier::Recommended),
                None,
                vec![
                    arm("etag-declined", true, &[]),
                    arm("etag-supported", false, &["etag"]),
                ],
            ),
            option(
                "ehr-status-flat",
                (Theme::SimplifiedFormats, Tier::SpecBoundary),
                Some(352),
                vec![
                    arm("flat-served", true, &["ehr_status_flat"]),
                    arm("flat-not-served", false, &[]),
                ],
            ),
        ],
        reports: vec![
            OutlineReport {
                report: issue(42),
                title: "A duplicate create has no status".to_owned(),
                state: ReportState::Open,
            },
            OutlineReport {
                report: issue(352),
                title: "EHR_STATUS has no flat form".to_owned(),
                state: ReportState::Confirmed,
            },
        ],
        cases: places(),
    }
}

#[test]
fn the_markdown_summary_by_base_and_options_is_pinned() {
    let summary = markdown(&outlined());
    assert_eq!(summary, markdown(&outlined()), "byte-deterministic");
    insta::assert_snapshot!(summary);
}

#[test]
fn the_terminal_view_is_pinned() {
    let options = ShowOptions {
        color: false,
        failed: true,
    };
    let view = render(&outlined(), options);
    assert!(!view.contains('\u{1b}'), "{view}");
    insta::assert_snapshot!(view);
}

#[test]
fn the_terminal_view_without_an_outline_lists_every_case_unplaced() {
    let view = render(&fixture(), ShowOptions::default());
    assert!(view.contains("Unplaced"), "{view}");
    assert!(!view.contains("BASE"), "{view}");
    assert!(!view.contains("Failed and errored rows"), "{view}");
    insta::assert_snapshot!(view);
}

#[test]
fn the_terminal_view_never_shows_an_advisory_row_as_a_failure() {
    let view = render(
        &fixture(),
        ShowOptions {
            color: false,
            failed: true,
        },
    );
    let (_, expanded) = view.split_once("Failed and errored rows").unwrap();
    let (failures, advisory) = expanded.split_once("Advisory rows").unwrap();
    assert!(!failures.contains("ehr/etag"), "{view}");
    assert!(advisory.contains("ehr/etag"), "{view}");
    assert!(advisory.contains(ADVISORY_LINE), "{view}");
}

fn show(results: &Path, args: &[&str], no_color: Option<&str>) -> String {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_veredictum"));
    command.arg("show").arg(results).args(args);
    match no_color {
        Some(value) => command.env("NO_COLOR", value),
        None => command.env_remove("NO_COLOR"),
    };
    let output = command.output().expect("the binary runs");
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).expect("the view is UTF-8")
}

#[test]
fn the_show_command_colours_only_when_asked() {
    let dir = assert_fs::TempDir::new().unwrap();
    let path = dir.path().join("results.json");
    std::fs::write(&path, outlined().to_text().unwrap()).unwrap();
    let plain = render(&outlined(), ShowOptions::default());
    assert_eq!(show(&path, &[], None), plain, "a pipe is not a terminal");
    assert_eq!(show(&path, &["--color", "auto"], Some("1")), plain);
    assert_eq!(show(&path, &["--color", "never"], None), plain);
    let coloured = show(&path, &["--color", "always"], Some("1"));
    assert!(coloured.contains("\u{1b}[31m"), "{coloured}");
    assert!(coloured.contains("\u{1b}[32m"), "{coloured}");
    assert_eq!(
        show(&path, &["--failed", "--color", "never"], None),
        render(
            &outlined(),
            ShowOptions {
                color: false,
                failed: true,
            }
        )
    );
}

#[test]
fn the_show_command_refuses_another_shape() {
    let dir = assert_fs::TempDir::new().unwrap();
    let path = dir.path().join("results.json");
    std::fs::write(&path, "{\"outcomes\": []}").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_veredictum"))
        .arg("show")
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}
