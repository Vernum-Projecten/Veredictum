// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The views of a `perf-result`: `show` and `export --markdown` render the
//! committed example record byte for byte, name a `custom` workload, the
//! unmeasured between-run noise and an operation judged below p99, and the
//! terminal view fits 80 columns; `perf-assets` draws its three charts byte
//! for byte, with a quantile that does not resolve marked apart and the
//! scout windows drawn apart from the plateaus. The example is the record
//! `make_example_perf_result` writes, held to it by
//! `scripts/checks/example-results-drift.sh`.

use std::path::{Path, PathBuf};
use std::process::Command;

use veredictum::arazzo::terminal::ShowOptions;
use veredictum::perf_result::PerfResult;
use veredictum::perf_result::charts::saturation;
use veredictum::perf_result::derive::Latency;
use veredictum::perf_result::judge::rejudge;
use veredictum::perf_result::view::{WIDTH, markdown, terminal};
use veredictum::pipeline::assets::perf_result_assets;
use veredictum::pipeline::export::{Exports, export_document, show_results};

type Fallible = Result<(), Box<dyn std::error::Error>>;

fn example() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/perf-result.example.json")
}

fn record() -> Result<PerfResult, Box<dyn std::error::Error>> {
    Ok(PerfResult::read(&std::fs::read_to_string(example())?)?)
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_example_record_re_judges_byte_for_byte() -> Fallible {
    let judgement = rejudge(&std::fs::read_to_string(example())?)?;
    assert!(judgement.reproduces(), "{:?}", judgement.mismatches);
    Ok(())
}

#[test]
fn the_terminal_view_is_pinned() -> Fallible {
    let view = show_results(
        &example(),
        ShowOptions {
            color: false,
            failed: false,
        },
    )?;
    insta::assert_snapshot!(view);
    Ok(())
}

#[test]
fn the_coloured_terminal_view_is_pinned() -> Fallible {
    insta::assert_snapshot!(terminal(&record()?, true));
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_markdown_view_is_pinned_and_is_what_export_writes() -> Fallible {
    let Exports::Perf { markdown: exported } = export_document(&example())? else {
        panic!("the example is a perf-result");
    };
    assert_eq!(exported, markdown(&record()?));
    insta::assert_snapshot!(exported);
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_terminal_view_fits_eighty_columns() -> Fallible {
    let view = terminal(&record()?, false);
    for line in view.lines() {
        assert!(
            line.chars().count() <= WIDTH,
            "{} columns: {line}",
            line.chars().count()
        );
    }
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn both_views_name_a_custom_workload_the_unmeasured_noise_and_a_low_judged_quantile() -> Fallible {
    let record = record()?;
    for view in [terminal(&record, false), markdown(&record)] {
        // Words joined back across the terminal view's wrapping.
        let view = view.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            view.contains("custom: a document the instrument does not pin"),
            "{view}"
        );
        assert!(view.contains("between-run noise unmeasured"), "{view}");
        assert!(view.contains("aql:cohort_count is judged at p50"), "{view}");
        assert!(view.contains("n/r: not resolvable"), "{view}");
        assert!(view.contains("unsteady"), "{view}");
        assert!(view.contains(&record.derived.scope), "{view}");
    }
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn export_refuses_junit_and_ctrf_for_a_perf_result() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let junit = dir.path().join("junit.xml");
    let out = Command::new(env!("CARGO_BIN_EXE_veredictum"))
        .args(["export", "--results"])
        .arg(example())
        .arg("--junit")
        .arg(&junit)
        .env_remove("GITHUB_STEP_SUMMARY")
        .output()?;
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8(out.stderr)?.contains("Markdown view only"));
    assert!(!junit.exists());

    let summary = dir.path().join("summary.md");
    let out = Command::new(env!("CARGO_BIN_EXE_veredictum"))
        .args(["export", "--results"])
        .arg(example())
        .arg("--markdown")
        .arg(&summary)
        .env_remove("GITHUB_STEP_SUMMARY")
        .output()?;
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(std::fs::read_to_string(&summary)?, markdown(&record()?));
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_document_of_an_unknown_family_is_refused_by_name() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let path = dir.path().join("other.json");
    std::fs::write(&path, r#"{"family":"stress"}"#)?;
    let error = show_results(&path, ShowOptions::default()).expect_err("an unknown family");
    assert!(error.to_string().contains("\"stress\""), "{error}");
    Ok(())
}

/// The three charts of the example record, pinned byte for byte so a change
/// to any drawn number or mark is a reviewed snapshot change.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_three_charts_are_pinned_and_read_in_both_themes() -> Fallible {
    let assets = perf_result_assets(&example())?;
    let names: Vec<&str> = assets.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "perf-saturation.svg",
            "perf-throughput.svg",
            "perf-operations.svg"
        ]
    );
    for file in &assets.files {
        assert!(file.body.contains("@media (prefers-color-scheme: dark)"));
        assert!(
            file.body
                .starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\"")
        );
        assert!(file.body.contains("<title>"));
        insta::assert_snapshot!(file.name.clone(), file.body);
    }
    assert_eq!(assets.summary, markdown(&record()?));
    Ok(())
}

/// The quantiles of `latencies` that resolve, over p50, p95 and p99.
fn resolved<'a>(latencies: impl Iterator<Item = &'a Latency>) -> usize {
    latencies
        .map(|l| {
            [&l.p50, &l.p95, &l.p99]
                .iter()
                .filter(|q| q.is_resolved())
                .count()
        })
        .sum()
}

/// The saturation chart draws every scout window as a hollow circle and
/// every plateau as a square, marks each quantile that does not resolve
/// below the plot, and shades the knee bracket.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_saturation_chart_sets_scout_windows_apart_and_never_joins_an_unresolved_quantile() -> Fallible
{
    let record = record()?;
    let svg = saturation(&record);
    let scout = resolved(record.derived.scout.iter().map(|w| &w.latency));
    let plateaus = resolved(record.derived.plateaus.iter().map(|p| &p.latency));
    let points = (record.derived.scout.len() + record.derived.plateaus.len()) * 3;
    let unresolved = points - scout - plateaus;
    assert_eq!(svg.matches(" hollow\"/>").count(), scout);
    assert_eq!(svg.matches("width=\"7\" height=\"7\"").count(), plateaus);
    assert_eq!(svg.matches(" nr\"/>").count(), unresolved);
    assert!(
        unresolved > 0,
        "the example has quantiles that do not resolve"
    );
    assert_eq!(svg.matches("class=\"bracket\"").count(), 1);
    assert_eq!(svg.matches("class=\"fit\"").count(), 1);
    Ok(())
}
