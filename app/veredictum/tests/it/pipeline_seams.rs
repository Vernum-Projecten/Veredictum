// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The consumable pipeline seams (`veredictum::pipeline`): judging a
//! committed measured campaign, rendering the published assets, reading the
//! party documents, and the typed failures each seam reports.
//!
//! Every input here is a file already committed to this repository — the
//! performance schedule, a party statement, the example results document —
//! so the seams are exercised the way a second consumer of the library would
//! drive them, with no system under test involved.

use std::path::{Path, PathBuf};

use veredictum::pipeline::{Error, assets, judgement, measured, read_json, to_json_document};

/// Anything a seam or a fixture read can fail with, so a test body
/// propagates plumbing failures with `?` instead of unwrapping them
/// (<https://doc.rust-lang.org/book/ch11-01-writing-tests.html>).
type Fallible = Result<(), Box<dyn std::error::Error>>;

/// The repository root: the crate sits two levels under it.
fn repo_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// The performance schedule the measured instruments read.
fn perf() -> PathBuf {
    repo_root().join("perf")
}

/// The named ICS fixture: a filled-in declaration for a product that does not
/// exist, which is the shape the judgement seam needs.
fn statement_path() -> PathBuf {
    repo_root().join("fixtures/declaration/statement.json")
}

/// The committed example results document (`examples/results.example.json`).
fn results_path() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/results.example.json"
    ))
}

/// Judges the example campaign against the committed performance schedule.
fn judge_example() -> Result<judgement::Judgement, Error> {
    let statement = statement_path();
    let results = results_path();
    let root = perf();
    judgement::judge(&judgement::JudgementRequest {
        statement: &statement,
        results: &results,
        root: &root,
    })
}

// ── the catalogue seam ──────────────────────────────────────────────────────

// ── the judgement seam ─────────────────────────────────────────────────────

/// Judging the committed campaign writes one document, `verdicts.json`, and
/// that document is the report itself: the measured class verdicts
/// re-derived from the embedded histograms.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn judging_a_committed_campaign_writes_its_verdict_record() -> Fallible {
    let judgement = judge_example()?;
    let names: Vec<&str> = judgement
        .documents
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    assert_eq!(names, [judgement::VERDICTS_FILE]);
    let record = judgement
        .documents
        .first()
        .ok_or("the judgement writes its record")?;
    assert!(
        record.body.ends_with('\n'),
        "every emitted document ends with a newline"
    );
    assert_eq!(
        record.body,
        to_json_document(&judgement.report, "verdicts")?
    );
    assert_eq!(
        judgement.report.performance.len(),
        judgement.results.measurements.len(),
        "every measurement re-derives into one verdict"
    );
    assert_eq!(
        judgement.statement.product.name, "Fixture CDR",
        "the statement travels back with the judgement"
    );
    assert_eq!(
        judgement.is_clean(),
        judgement.report.review.is_empty(),
        "`is_clean` is the static review being empty"
    );
    Ok(())
}

/// A campaign the results record does not describe is refused rather than
/// judged: the statement path must name a statement.
#[test]
fn judging_refuses_a_document_that_is_not_a_statement() {
    let results = results_path();
    let root = perf();
    // The results document is a valid party artifact of the WRONG family, so
    // the schema stage is what rejects it.
    let error = judgement::judge(&judgement::JudgementRequest {
        statement: &results,
        results: &results,
        root: &root,
    })
    .expect_err("a results document is not a statement");
    let message = error.to_string();
    assert!(matches!(error, Error::Party(_)), "{message}");
    assert!(message.contains("schema"), "{message}");
}

// ── the asset seams ────────────────────────────────────────────────────────

/// The performance visuals render the class ladder plus one latency chart per
/// measurement, and the resource and disk charts only from records that
/// actually carry those samples.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_performance_visuals_render_only_what_the_record_carries() -> Fallible {
    let root = perf();
    let results = results_path();
    let rendered = assets::performance_assets(&root, &results, None)?;
    let names: Vec<&str> = rendered.files.iter().map(|f| f.name.as_str()).collect();

    assert_eq!(names.first(), Some(&"perf-class-ladder.svg"));
    assert!(
        names.contains(&"perf-latency-class-POC.svg"),
        "the example's measured class gets its latency chart: {names:?}"
    );
    assert!(
        !names.iter().any(|n| n.starts_with("perf-resources-")),
        "the example samples no container resources, so no series is drawn: {names:?}"
    );
    for file in &rendered.files {
        assert!(file.body.starts_with("<svg"), "{} is not SVG", file.name);
    }

    let summary = rendered.summary_markdown()?;
    assert!(
        summary.contains("POC"),
        "the summary reports the measured class: {summary}"
    );
    Ok(())
}

/// The schema seam serves exactly the published set: the same names and the
/// same bytes as the committed `schemas/` directory a party validates against.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_schema_seam_serves_the_committed_published_set() -> Fallible {
    let dir = repo_root().join("schemas");
    let files = assets::schema_files();
    assert!(!files.is_empty());
    for file in &files {
        let committed = std::fs::read_to_string(dir.join(&file.name))?;
        assert_eq!(
            committed, file.body,
            "{} differs from the committed schema",
            file.name
        );
    }
    Ok(())
}

// ── the shared readers ─────────────────────────────────────────────────────

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_json_reader_names_what_it_was_reading() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let absent = dir.path().join("absent.json");
    let error = read_json::<serde_json::Value>(&absent, "topology")
        .expect_err("an absent file cannot be read");
    match &error {
        Error::Read { path, .. } => assert_eq!(path, &absent),
        other => panic!("expected a read failure, got {other}"),
    }
    assert!(error.to_string().starts_with("cannot read "), "{error}");

    let broken = dir.path().join("broken.json");
    std::fs::write(&broken, "[")?;
    let error = read_json::<serde_json::Value>(&broken, "topology")
        .expect_err("a truncated document does not parse");
    assert!(matches!(error, Error::Parse { .. }), "{error}");
    assert!(error.to_string().starts_with("topology: "), "{error}");
    Ok(())
}

/// Every artifact this instrument writes is pretty JSON with a trailing
/// newline, and the parent directory is created before the write.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn documents_are_pretty_json_written_under_a_created_parent() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let path = dir.path().join("deep/nested/out.json");
    let body = to_json_document(&serde_json::json!({ "a": [1, 2] }), "test")?;
    assert_eq!(body, "{\n  \"a\": [\n    1,\n    2\n  ]\n}\n");

    veredictum::pipeline::ensure_parent_dir(&path)?;
    veredictum::pipeline::write_file(&path, &body)?;
    assert_eq!(std::fs::read_to_string(&path)?, body);

    // A path with no parent to create is not a failure, whether the parent is
    // the working directory or the path names nothing at all.
    veredictum::pipeline::ensure_parent_dir(Path::new("bare.json"))?;
    veredictum::pipeline::ensure_parent_dir(Path::new(""))?;

    let error = veredictum::pipeline::write_file(&dir.path().join("no/such/dir.json"), &body)
        .expect_err("writing into an absent directory fails");
    assert!(matches!(error, Error::Write { .. }), "{error}");
    Ok(())
}

/// File references inside an ixit document resolve against the DOCUMENT, not
/// the working directory, and the raw text travels with it because the
/// campaign's digest is taken over exactly those bytes.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_ixit_reader_rebases_file_references_and_keeps_the_bytes() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let path = dir.path().join("topology/ixit.json");
    std::fs::create_dir_all(path.parent().ok_or("the fixture path has a parent")?)?;
    let text = serde_json::to_string_pretty(&serde_json::json!({
        "instances": {
            "sut": { "base_url": "http://127.0.0.1:8080/openehr/v1", "auth": { "mode": "none" } },
            "smart_platform": { "base_url": "http://127.0.0.1:8080", "auth": { "mode": "none" } }
        },
        "smart": {
            "platform_instance": "smart_platform",
            "mint": {
                "issuer": "https://as.example.test",
                "subject": "cnf-smart-app",
                "key_file": "keys/test.key.pem",
                "kid": "k1",
                "ttl_seconds": 300
            }
        }
    }))?;
    std::fs::write(&path, &text)?;

    let (ixit, kept) = veredictum::pipeline::load_ixit(&path)?;
    assert_eq!(kept, text, "the digest is taken over the file's own bytes");
    let key = ixit
        .smart
        .as_ref()
        .ok_or("the fixture declares a SMART lane")?
        .mint
        .key_file
        .clone();
    assert_eq!(
        key,
        dir.path().join("topology/keys/test.key.pem"),
        "the key reference is rebased onto the document's directory, not the working directory"
    );

    // A document that is not a topology is a named parse failure.
    let broken = dir.path().join("not-ixit.json");
    std::fs::write(&broken, r#"{"instances": 7}"#)?;
    let error = veredictum::pipeline::load_ixit(&broken).expect_err("7 is not an instance map");
    assert!(matches!(error, Error::Parse { .. }), "{error}");
    assert!(error.to_string().starts_with("ixit: "), "{error}");
    Ok(())
}

// ── the measured seam's selectors ──────────────────────────────────────────

/// Only the ladder's own rungs are windows, and a window is a whole number of
/// hours in seconds.
#[test]
fn only_a_ladder_rung_is_a_sustained_window() {
    for hours in measured::SustainedWindow::LADDER {
        let window = measured::SustainedWindow::hours(*hours)
            .unwrap_or_else(|| panic!("{hours} is a rung of the ladder"));
        assert_eq!(window.seconds(), hours * 3600);
    }
    assert!(
        measured::SustainedWindow::hours(3).is_none(),
        "3 hours is not a rung, and nothing rounds it to one"
    );
    assert!(measured::SustainedWindow::hours(0).is_none());
    assert_eq!(
        measured::SustainedWindow::default().seconds(),
        3600,
        "the default window is the shortest rung"
    );
}

/// The class selector resolves against the performance schedule's own cases,
/// and an unstocked class is a named miss rather than a silent empty run.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_class_selects_its_own_performance_case() -> Fallible {
    let schedule = measured::load_clean_schedule(&perf())?;
    let class = veredictum::perf::PerfClass::parse("POC").map_err(Error::Selector)?;
    let (path, case) = measured::performance_case_of_class(&schedule, class, "POC")?;
    assert_eq!(case.class, class);
    assert!(
        path.ends_with("class_POC.yaml") || path.to_string_lossy().contains("POC"),
        "the case comes back with the artifact it was loaded from: {}",
        path.display()
    );

    let empty = veredictum::perf_schedule::PerfSchedule::default();
    let error = measured::performance_case_of_class(&empty, class, "POC")
        .expect_err("an empty catalogue stocks no class");
    assert!(matches!(error, Error::Missing(_)), "{error}");
    assert!(error.to_string().contains("class POC"), "{error}");
    Ok(())
}

/// The journey context and the scale OPT come from the committed performance
/// schedule, and a tree without one says which piece is missing instead of
/// measuring nothing.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_measured_preamble_reads_its_inputs_from_the_committed_schedule() -> Fallible {
    let schedule = measured::load_clean_schedule(&perf())?;
    let opt = measured::scale_opt_xml(&schedule)?;
    assert!(
        opt.contains("<template"),
        "the scale corpus commits against an OPT document"
    );
    let (journeys, pack) = measured::journey_context(&schedule)?;
    assert!(!journeys.0.is_empty(), "the catalogue names journeys");
    assert!(
        !pack.templates.is_empty(),
        "the pack carries a template per journey stage"
    );

    let empty = veredictum::perf_schedule::PerfSchedule::default();
    let error = measured::scale_opt_xml(&empty).expect_err("no payload manifest, no OPT");
    assert!(matches!(error, Error::Missing(_)), "{error}");
    let error = measured::journey_context(&empty).expect_err("no catalogue, no journeys");
    assert!(error.to_string().contains("journeys.yaml"), "{error}");
    Ok(())
}

// ── the seams' typed refusals ──────────────────────────────────────────────

/// An excused row with no citation would claim a case was out of scope
/// without saying on what ground. Two gates refuse it, and the ORDER matters
/// to whoever reads the diagnostic: the published schema stage runs first and
/// names the document member, so the typed invariant behind it is the second
/// gate — the one a consumer building results in memory still faces.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_uncited_excused_row_is_refused_by_the_schema_and_by_the_invariant() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let mut results: veredictum::party::Results = read_json(&results_path(), "results")?;
    let mut uncited = results
        .outcomes
        .first()
        .cloned()
        .ok_or("the example records at least one outcome")?;
    uncited.status = veredictum::party::OutcomeStatus::NotApplicable;
    uncited.citation = None;
    let mut blank = uncited.clone();
    blank.status = veredictum::party::OutcomeStatus::Skipped;
    blank.citation = Some(String::new());
    results.outcomes.push(uncited);
    results.outcomes.push(blank);

    // The typed invariant names one violation per uncited row.
    let violations = results
        .check_invariants()
        .expect_err("an uncited excused row breaks the record's own invariant");
    assert_eq!(violations.len(), 2, "{violations:?}");

    // Rendered as the seam's refusal, every violation is on its own prefixed
    // line, so a caller that only prints the error still sees all of them.
    let rendered = Error::ResultsInvariants(violations).to_string();
    assert_eq!(rendered.lines().count(), 2, "{rendered}");
    for line in rendered.lines() {
        assert!(line.starts_with("results invariant: "), "{line}");
    }

    // Read from disk, the published schema refuses the same document first,
    // naming the member rather than the invariant.
    let path = dir.path().join("results.json");
    std::fs::write(&path, to_json_document(&results, "results")?)?;
    let statement = statement_path();
    let root = perf();
    let error = judgement::judge(&judgement::JudgementRequest {
        statement: &statement,
        results: &path,
        root: &root,
    })
    .expect_err("a record that breaks its own invariants is not judgeable");
    assert!(matches!(error, Error::Party(_)), "{error}");
    assert!(error.to_string().contains("citation"), "{error}");
    Ok(())
}

/// The performance visuals draw a resource series and a disk-growth chart
/// only from a record that CARRIES those samples, and the stress curve only
/// when a report is supplied — nothing is fabricated from an absent input.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_performance_visuals_draw_the_samples_a_record_carries() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let mut results: veredictum::party::Results = read_json(&results_path(), "results")?;
    let measurement = results
        .measurements
        .first_mut()
        .ok_or("the example carries a measurement")?;
    measurement.resources = Some(serde_json::from_value(serde_json::json!({
        "sample_interval_s": 5,
        "containers": [{
            "role": "sut", "name": "cdr",
            "samples": [
                { "offset_s": 0, "phase": "warmup", "cpu_pct": 12.5, "rss_bytes": 100,
                  "blk_read_bytes": 0, "blk_write_bytes": 0,
                  "net_rx_bytes": 0, "net_tx_bytes": 0 },
                { "offset_s": 5, "phase": "measured", "cpu_pct": 80.0, "rss_bytes": 200,
                  "blk_read_bytes": 10, "blk_write_bytes": 20,
                  "net_rx_bytes": 30, "net_tx_bytes": 40 }
            ]
        }],
        "disk": {
            "before_scale_seed_bytes": 1_000,
            "after_scale_seed_bytes": 5_000,
            "after_window_bytes": 6_000,
            "seed_compositions": 100
        }
    }))?);
    let sampled = dir.path().join("results-sampled.json");
    std::fs::write(&sampled, to_json_document(&results, "results")?)?;

    let root = perf();
    let rendered = assets::performance_assets(&root, &sampled, None)?;
    let names: Vec<&str> = rendered.files.iter().map(|f| f.name.as_str()).collect();
    assert!(
        names.iter().any(|n| n.starts_with("perf-resources-class-")),
        "a sampled record draws its resource series: {names:?}"
    );
    assert!(
        names.contains(&"perf-disk-growth.svg"),
        "anchored disk bytes draw the growth chart: {names:?}"
    );
    for file in &rendered.files {
        assert!(file.body.starts_with("<svg"), "{} is not SVG", file.name);
    }
    Ok(())
}

/// A committed stress report renders its curve beside the performance
/// visuals, and the same two reports render the cross-SUT overlay — both are
/// exploration records, never conformance evidence.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_committed_stress_report_renders_its_curve_and_its_overlay() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let results: veredictum::party::Results = read_json(&results_path(), "results")?;
    let operations = results
        .measurements
        .first()
        .map(|m| m.operations.clone())
        .ok_or("the example carries a measured operation")?;
    let environment = results
        .measurements
        .first()
        .map(|m| m.environment.clone())
        .ok_or("the example carries an environment")?;

    let step = |rate: f64, stable: bool| veredictum::stress::LoadStep {
        rate,
        offered_load_sustained: rate,
        operations: operations.clone(),
        stable,
        breaches: if stable {
            Vec::new()
        } else {
            vec!["p99 3000ms > budget 1000ms".to_owned()]
        },
        generator_bound: false,
        resources: None,
    };
    let report = |max: f64| veredictum::stress::StressReport {
        corpus: "cnf.scale.10k".to_owned(),
        environment: environment.clone(),
        step_warmup_s: 10,
        step_hold_s: 30,
        p99_budget_ms: 1_000.0,
        error_budget: 0.0,
        steps: vec![step(10.0, true), step(200.0, false)],
        max_sustainable_throughput_per_s: max,
        ladder_capped: false,
        generator_bound: false,
        remark: "exploration only".to_owned(),
    };

    let left = dir.path().join("stress-left.json");
    let right = dir.path().join("stress-right.json");
    std::fs::write(&left, to_json_document(&report(10.0), "stress")?)?;
    std::fs::write(&right, to_json_document(&report(200.0), "stress")?)?;

    let rendered = assets::performance_assets(&perf(), &results_path(), Some(&left))?;
    let names: Vec<&str> = rendered.files.iter().map(|f| f.name.as_str()).collect();
    assert!(
        names.contains(&"perf-stress-curve.svg"),
        "a supplied report draws the curve: {names:?}"
    );

    let overlay = assets::stress_overlay(("left", &left), ("right", &right))?;
    assert!(overlay.starts_with("<svg"), "{overlay}");
    assert!(
        overlay.contains("left") && overlay.contains("right"),
        "{overlay}"
    );

    // A report neither seam can read is a typed failure naming the file.
    let absent = dir.path().join("absent.json");
    let error = assets::stress_overlay(("left", &absent), ("right", &right))
        .expect_err("an absent report is not renderable");
    assert!(matches!(error, Error::Read { .. }), "{error}");
    let error = assets::performance_assets(&perf(), &results_path(), Some(&absent))
        .expect_err("an absent report is not renderable");
    assert!(matches!(error, Error::Read { .. }), "{error}");
    Ok(())
}

/// A value that cannot be serialized is a typed failure naming what was being
/// written, and a directory that cannot be created is named too — neither
/// silently produces an empty artifact.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_unwritable_document_names_what_it_was_writing() -> Fallible {
    use std::collections::BTreeMap;

    // A map keyed by anything but a string is not a JSON object.
    let unserializable: BTreeMap<(u8, u8), u8> = BTreeMap::from([((1, 2), 3)]);
    let error = to_json_document(&unserializable, "the verdict record")
        .expect_err("a non-string map key is not JSON");
    assert!(matches!(error, Error::Serialize { .. }), "{error}");
    assert!(
        error.to_string().starts_with("the verdict record:"),
        "{error}"
    );

    // A parent directory that cannot exist, because a FILE occupies its path.
    let dir = assert_fs::TempDir::new()?;
    let occupied = dir.path().join("occupied");
    std::fs::write(&occupied, "not a directory")?;
    let error = veredictum::pipeline::ensure_parent_dir(&occupied.join("under/out.json"))
        .expect_err("a file cannot hold a directory");
    let Error::CreateDir { path, .. } = &error else {
        panic!("expected the directory failure, got {error}");
    };
    assert!(path.ends_with("under"), "{}", path.display());
    Ok(())
}

/// An ixit path that cannot be read at all is a typed READ failure naming the
/// file, kept apart from a document that reads but does not parse.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_unreadable_ixit_is_a_read_failure_not_a_parse_failure() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let absent = dir.path().join("absent-ixit.json");
    let error = veredictum::pipeline::load_ixit(&absent).expect_err("an absent ixit is unreadable");
    let Error::Read { path, .. } = &error else {
        panic!("expected the read failure, got {error}");
    };
    assert_eq!(path, &absent);
    Ok(())
}

// ── the replay seam's selection posture ────────────────────────────────────
