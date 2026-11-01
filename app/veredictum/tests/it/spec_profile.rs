// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The specification profile end to end: a development run of the committed
//! catalogue against the fake SUT reads every case citation against the
//! development snapshot, runs a case whose citation the snapshot rewords
//! under its development wording, names the profile and the snapshot in the
//! results and in every view, re-judges into the same bytes, and leaves the
//! released run untouched. `validate` reports the snapshot's absences as
//! information. The skip of a case whose cited text the snapshot lacks is
//! pinned by the catalogue planner's own unit test.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use wiremock::matchers::any;
use wiremock::{Mock, ResponseTemplate};

use crate::fake_sut::FakeSut;

/// Anything a run or a fixture read can fail with.
type Fallible = Result<(), Box<dyn std::error::Error>>;

/// A document one of whose citations the development snapshot rewords: the
/// Version Lifecycle list gained two lifecycle states.
const DOCUMENT: &str = "contribution_create";

/// The case citing that list, which carries its development wording.
const REWORDED_CASE: &str = "contribution_create/commit_contribution-incomplete_lifecycle";

fn repo() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn veredictum(args: &[&str]) -> Result<Output, std::io::Error> {
    Command::new(env!("CARGO_BIN_EXE_veredictum"))
        .args(args)
        .env_remove("VEREDICTUM_CATALOGUE")
        .env_remove("GITHUB_STEP_SUMMARY")
        .output()
}

/// A run of [`DOCUMENT`]; `profile` names the `--profile` flag, and `None`
/// passes none.
fn drive(base_url: &str, out: &Path, profile: Option<&str>) -> Result<Output, std::io::Error> {
    let root = repo().join("catalogue");
    let root = root.to_string_lossy();
    let out = out.to_string_lossy();
    let mut args = vec![
        "test",
        "--root",
        &root,
        "--document",
        DOCUMENT,
        "--base-url",
        base_url,
        "--out",
        &out,
    ];
    if let Some(profile) = profile {
        args.extend(["--profile", profile]);
    }
    veredictum(&args)
}

fn read_json(path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

fn row<'r>(results: &'r Value, case: &str) -> Option<&'r Value> {
    results
        .get("rows")?
        .as_array()?
        .iter()
        .find(|row| row.get("case") == Some(&json!(case)))
}

fn absent_rows(results: &Value) -> usize {
    results
        .get("rows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|row| row.pointer("/skip/reason") == Some(&json!("cited-text-absent")))
        .count()
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
#[expect(
    clippy::too_many_lines,
    reason = "one run read through the record, the re-judgement and every view, in order"
)]
fn a_run_without_a_profile_reads_the_development_snapshot_and_names_it_everywhere() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let sut = FakeSut::start();
    sut.mount(Mock::given(any()).respond_with(ResponseTemplate::new(400)));

    let released = dir.path().join("released");
    drive(&sut.base_url(), &released, Some("released"))?;
    let released_results = read_json(&released.join("results.json"))?;
    assert_eq!(
        released_results.get("specification"),
        Some(&json!({ "profile": "released" }))
    );
    assert_eq!(absent_rows(&released_results), 0);

    let out = dir.path().join("development");
    let output = drive(&sut.base_url(), &out, None)?;
    assert_ne!(output.status.code(), Some(2), "{output:?}");
    let results = read_json(&out.join("results.json"))?;
    let violations: Vec<String> =
        jsonschema::validator_for(&veredictum::schema::arazzo_results_schema())?
            .iter_errors(&results)
            .map(|error| format!("{}: {error}", error.instance_path()))
            .collect();
    assert!(violations.is_empty(), "{violations:?}");
    assert_eq!(
        results.pointer("/specification/profile"),
        Some(&json!("development"))
    );
    let components: Vec<&str> = results
        .pointer("/specification/snapshot")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| c.get("component").and_then(Value::as_str))
        .collect();
    assert_eq!(
        components,
        [
            "AM", "BASE", "ITS-JSON", "ITS-REST", "ITS-XML", "QUERY", "RM"
        ]
    );

    let reworded = row(&results, REWORDED_CASE).ok_or("the reworded case has a row")?;
    assert_ne!(
        reworded.get("status"),
        Some(&json!("skipped")),
        "a reworded citation runs under its development wording: {reworded}"
    );
    assert_eq!(
        absent_rows(&results),
        0,
        "every case of the document cites text the snapshot holds"
    );
    let links: Vec<&str> = results
        .get("rows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| row.pointer("/explanation/why/link").and_then(Value::as_str))
        .collect();
    assert!(!links.is_empty(), "the red rows carry their citations");
    assert!(
        links.iter().all(|link| !link.contains("24058992d5fa")),
        "no development row links to the release: {links:?}"
    );
    assert!(
        links
            .iter()
            .any(|link| link.contains("specifications-ITS-REST/blob/9d25ea783f79")),
        "the rows link to the snapshot's commits: {links:?}"
    );
    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        stdout.contains("Profile  development snapshot (AM master@15d7a3589d52"),
        "{stdout}"
    );
    assert!(stdout.contains("never a released result"), "{stdout}");

    let again = dir.path().join("again");
    let recording = out.join("recording.har");
    let root = repo().join("catalogue");
    veredictum(&[
        "test",
        "--root",
        &root.to_string_lossy(),
        "--document",
        DOCUMENT,
        "--rejudge",
        &recording.to_string_lossy(),
        "--out",
        &again.to_string_lossy(),
        "--profile",
        "development",
    ])?;
    assert_eq!(
        std::fs::read(again.join("results.json"))?,
        std::fs::read(out.join("results.json"))?
    );

    let results_path = out.join("results.json");
    let shown = veredictum(&["show", &results_path.to_string_lossy(), "--color", "never"])?;
    let shown = String::from_utf8(shown.stdout)?;
    assert!(
        shown.contains("Judged against the development snapshot ("),
        "{shown}"
    );
    let markdown = dir.path().join("summary.md");
    let junit = dir.path().join("junit.xml");
    let ctrf = dir.path().join("ctrf.json");
    veredictum(&[
        "export",
        "--results",
        &results_path.to_string_lossy(),
        "--markdown",
        &markdown.to_string_lossy(),
        "--junit",
        &junit.to_string_lossy(),
        "--ctrf",
        &ctrf.to_string_lossy(),
    ])?;
    let markdown = std::fs::read_to_string(markdown)?;
    assert!(
        markdown.contains("| **Specification** | development snapshot ("),
        "{markdown}"
    );
    let junit = std::fs::read_to_string(junit)?;
    assert!(
        junit.contains(r#"name="eu.veredictum.profile" value="development snapshot ("#),
        "{junit}"
    );
    let ctrf = read_json(&ctrf)?;
    assert_eq!(
        ctrf.pointer("/results/environment/extra/eu.veredictum/specification/profile"),
        Some(&json!("development"))
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn validate_reports_the_snapshot_absences_as_information() -> Fallible {
    let root = repo().join("catalogue");
    let specs = repo().join("specs/openehr");
    let output = veredictum(&[
        "validate",
        "--root",
        &root.to_string_lossy(),
        "--specs",
        &specs.to_string_lossy(),
        "--profile",
        "development",
    ])?;
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains(", 0 finding(s)\n"), "{stdout}");
    assert!(
        stdout.contains("(the cases citing them are skipped as cited-text-absent), 0 finding(s)"),
        "{stdout}"
    );
    assert!(
        stdout.contains("development profile: RM: "),
        "the counts are per component: {stdout}"
    );
    assert!(
        stdout.contains("development profile: 54 development-only workflow(s), skipped as development-only under the released profile; "),
        "the development-only and reworded cases are counted: {stdout}"
    );
    let released = veredictum(&[
        "validate",
        "--root",
        &root.to_string_lossy(),
        "--specs",
        &specs.to_string_lossy(),
    ])?;
    let released = String::from_utf8(released.stdout)?;
    assert!(!released.contains("[info]"), "{released}");
    assert!(!released.contains("profile"), "{released}");

    let without_specs = veredictum(&[
        "validate",
        "--root",
        &root.to_string_lossy(),
        "--profile",
        "development",
    ])?;
    assert_eq!(without_specs.status.code(), Some(2), "{without_specs:?}");
    let unknown = veredictum(&[
        "validate",
        "--root",
        &root.to_string_lossy(),
        "--profile",
        "draft",
    ])?;
    assert_eq!(
        unknown.status.code(),
        Some(2),
        "an unknown profile is refused"
    );
    Ok(())
}
