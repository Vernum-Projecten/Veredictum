// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `test` subcommand over the committed Arazzo catalogue, against the
//! fake SUT: a passing, a failed and an errored run and their exit statuses,
//! skipped and advisory rows staying green, the offline re-judgement
//! reproducing the recorded bytes, the administrative credential sent and
//! never recorded, and the usage and input errors that stop a run.
//!
//! Each run names one document with `--document`, so it drives a handful of
//! requests; the whole catalogue is still gated first, as it is for every
//! run.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use base64::Engine as _;
use serde_json::{Value, json};
use wiremock::matchers::{any, method};
use wiremock::{Mock, ResponseTemplate};

use crate::fake_sut::{FakeSut, closed_port_url};

/// Anything a run or a fixture read can fail with.
type Fallible = Result<(), Box<dyn std::error::Error>>;

/// The document whose five workflows send an invalid credential and expect
/// `401` with a `WWW-Authenticate` challenge, selected by its option arm.
const AUTHENTICATION: &str = "authentication_required";

/// The arm that selects [`AUTHENTICATION`].
const AUTHENTICATION_ARM: &str = "authentication-required";

/// The capability folder [`AUTHENTICATION`] sits in.
const AUTHENTICATION_CAPABILITY: &str = "security";

/// A document of the demographic capability folder, which a statement
/// claiming only [`AUTHENTICATION_CAPABILITY`] leaves unclaimed.
const DEMOGRAPHIC: &str = "person";

/// Every variable a run reads a credential or the root from, removed so the
/// developer's or the runner's own environment never reaches a test.
const READ_VARS: [&str; 13] = [
    "VEREDICTUM_CATALOGUE",
    "VEREDICTUM_SUT_USER",
    "VEREDICTUM_SUT_USER_FILE",
    "VEREDICTUM_SUT_PASSWORD",
    "VEREDICTUM_SUT_PASSWORD_FILE",
    "VEREDICTUM_SUT_TOKEN",
    "VEREDICTUM_SUT_TOKEN_FILE",
    "VEREDICTUM_ADMIN_USER",
    "VEREDICTUM_ADMIN_USER_FILE",
    "VEREDICTUM_ADMIN_PASSWORD",
    "VEREDICTUM_ADMIN_PASSWORD_FILE",
    "VEREDICTUM_ADMIN_TOKEN",
    "VEREDICTUM_ADMIN_TOKEN_FILE",
];

fn catalogue() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../catalogue"))
}

/// One `veredictum test` invocation over the committed catalogue.
fn test_command(args: &[&str], envs: &[(&str, &str)]) -> Result<Output, std::io::Error> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_veredictum"));
    command
        .arg("test")
        .arg("--root")
        .arg(catalogue())
        .args(args);
    for var in READ_VARS {
        command.env_remove(var);
    }
    command.envs(envs.iter().copied()).output()
}

/// A statement claiming the capability folders `claims` and selecting
/// `arms`, written into `dir`.
fn statement(
    dir: &Path,
    claims: &[&str],
    arms: &[&str],
) -> Result<String, Box<dyn std::error::Error>> {
    let path = dir.join("statement.json");
    let document = json!({ "claims": { "capabilities": claims }, "options": arms });
    std::fs::write(&path, serde_json::to_string(&document)?)?;
    Ok(path.to_string_lossy().into_owned())
}

/// A fake SUT answering every request with `answer`.
fn answering(answer: ResponseTemplate) -> FakeSut {
    let sut = FakeSut::start();
    sut.mount(Mock::given(any()).respond_with(answer));
    sut
}

fn read_json(path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

/// The statuses of the rows of a results document, in order.
fn statuses(results: &Value) -> Vec<String> {
    results
        .get("rows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|row| {
            row.get("status")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned()
        })
        .collect()
}

/// The `whose` of every red row of a results document, in order.
fn whose(results: &Value) -> Vec<String> {
    results
        .get("rows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| row.pointer("/explanation/whose").and_then(Value::as_str))
        .map(str::to_owned)
        .collect()
}

/// Drives [`AUTHENTICATION`] against `base_url` into `out` under its arm.
fn drive_authentication(
    dir: &Path,
    base_url: &str,
    out: &Path,
) -> Result<Output, Box<dyn std::error::Error>> {
    let statement = statement(dir, &[AUTHENTICATION_CAPABILITY], &[AUTHENTICATION_ARM])?;
    let out = out.to_string_lossy();
    Ok(test_command(
        &[
            "--document",
            AUTHENTICATION,
            "--statement",
            &statement,
            "--base-url",
            base_url,
            "--out",
            &out,
        ],
        &[],
    )?)
}

/// Re-judges `out/recording.har` into `again`, under the statement in `dir`.
fn rejudge(dir: &Path, document: &str, out: &Path, again: &Path) -> Result<Output, std::io::Error> {
    let recording = out.join("recording.har");
    let statement = dir.join("statement.json");
    test_command(
        &[
            "--document",
            document,
            "--statement",
            &statement.to_string_lossy(),
            "--rejudge",
            &recording.to_string_lossy(),
            "--out",
            &again.to_string_lossy(),
        ],
        &[],
    )
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_passing_run_exits_zero_and_writes_its_results_and_recording() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let out = dir.path().join("run");
    let sut = answering(
        ResponseTemplate::new(401).insert_header("WWW-Authenticate", "Bearer realm=\"cdr\""),
    );
    let output = drive_authentication(dir.path(), &sut.base_url(), &out)?;
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = read_json(&out.join("results.json"))?;
    assert_eq!(statuses(&results), vec!["passed"; 5]);
    assert_eq!(results.get("document"), Some(&json!("catalogue")));
    assert_eq!(sut.requests().len(), 5);
    let recording = read_json(&out.join("recording.har"))?;
    assert_eq!(
        recording
            .pointer("/log/entries")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(5)
    );
    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        stdout.contains("5 row(s): 5 passed, 0 failed, 0 errored, 0 skipped"),
        "{stdout}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_failed_row_exits_one_and_its_rejudgement_writes_the_same_bytes() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let out = dir.path().join("run");
    let sut = answering(ResponseTemplate::new(200));
    let output = drive_authentication(dir.path(), &sut.base_url(), &out)?;
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let results = read_json(&out.join("results.json"))?;
    assert_eq!(statuses(&results), vec!["failed"; 5]);
    assert_eq!(whose(&results), vec!["server"; 5]);
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("Expected status 401"), "{stdout}");

    let again = dir.path().join("rejudged");
    let rejudged = rejudge(dir.path(), AUTHENTICATION, &out, &again)?;
    assert_eq!(rejudged.status.code(), Some(1), "{rejudged:?}");
    assert_eq!(
        std::fs::read(again.join("results.json"))?,
        std::fs::read(out.join("results.json"))?
    );
    assert!(!again.join("recording.har").exists());
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_errored_row_exits_one_and_never_counts_as_failed() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let out = dir.path().join("run");
    let output = drive_authentication(dir.path(), &closed_port_url(), &out)?;
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let results = read_json(&out.join("results.json"))?;
    assert_eq!(statuses(&results), vec!["errored"; 5]);
    assert_eq!(whose(&results), vec!["instrument"; 5]);

    let again = dir.path().join("rejudged");
    let rejudged = rejudge(dir.path(), AUTHENTICATION, &out, &again)?;
    assert_eq!(rejudged.status.code(), Some(1), "{rejudged:?}");
    assert_eq!(
        std::fs::read(again.join("results.json"))?,
        std::fs::read(out.join("results.json"))?
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn skipped_and_advisory_rows_exit_zero() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let sut = answering(ResponseTemplate::new(404));
    let unselected = dir.path().join("unselected");
    let output = test_command(
        &[
            "--document",
            AUTHENTICATION,
            "--base-url",
            &sut.base_url(),
            "--out",
            &unselected.to_string_lossy(),
        ],
        &[],
    )?;
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = read_json(&unselected.join("results.json"))?;
    assert_eq!(statuses(&results), vec!["skipped"; 5]);
    assert_eq!(
        results.pointer("/rows/0/skip/reason"),
        Some(&json!("option-not-selected"))
    );
    assert!(sut.requests().is_empty());

    let advisory = dir.path().join("advisory");
    let output = test_command(
        &[
            "--document",
            "system_options",
            "--base-url",
            &sut.base_url(),
            "--out",
            &advisory.to_string_lossy(),
        ],
        &[],
    )?;
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = read_json(&advisory.join("results.json"))?;
    assert_eq!(statuses(&results), vec!["failed"; 3]);
    assert_eq!(whose(&results), vec!["advisory"; 3]);
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_administrative_credential_is_sent_and_never_recorded() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let statement = statement(dir.path(), &["admin"], &["admin-delete-synchronous"])?;
    // The EHR create is answered 201 and the administrative delete 204, so
    // the administrative step is reached.
    let sut = FakeSut::start();
    sut.mount(Mock::given(method("DELETE")).respond_with(ResponseTemplate::new(204)));
    sut.mount(Mock::given(any()).respond_with(ResponseTemplate::new(201)));
    let password = dir.path().join("admin-password");
    std::fs::write(&password, "s3cret-admin\n")?;
    let password = password.to_string_lossy().into_owned();

    let unsupplied = dir.path().join("unsupplied");
    let base_url = sut.base_url();
    let base = [
        "--document",
        "admin_ehr_delete",
        "--statement",
        &statement,
        "--base-url",
        &base_url,
    ];
    let output = test_command(
        &[&base[..], &["--out", &unsupplied.to_string_lossy()]].concat(),
        &[],
    )?;
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = read_json(&unsupplied.join("results.json"))?;
    let skipped = statuses(&results);
    assert!(
        !skipped.is_empty() && skipped.iter().all(|s| s == "skipped"),
        "{skipped:?}"
    );
    assert_eq!(
        results.pointer("/rows/0/skip/reason"),
        Some(&json!("not-declared"))
    );

    let out = dir.path().join("run");
    let output = test_command(
        &[
            &base[..],
            &["--admin-auth", "basic", "--out", &out.to_string_lossy()],
        ]
        .concat(),
        &[
            ("VEREDICTUM_ADMIN_USER", "root"),
            ("VEREDICTUM_ADMIN_PASSWORD_FILE", &password),
        ],
    )?;
    assert!(matches!(output.status.code(), Some(0 | 1)), "{output:?}");
    let credential = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode("root:s3cret-admin")
    );
    let sent: Vec<String> = sut
        .requests()
        .iter()
        .filter_map(|request| request.headers.get("authorization"))
        .filter_map(|value| value.to_str().ok().map(str::to_owned))
        .collect();
    assert!(sent.contains(&credential), "{sent:?}");
    let recording = std::fs::read_to_string(out.join("recording.har"))?;
    assert!(
        !recording.contains(&credential),
        "the credential was recorded"
    );
    assert!(
        !recording.contains("s3cret-admin"),
        "the password was recorded"
    );
    let recording: Value = serde_json::from_str(&recording)?;
    assert_eq!(
        recording.pointer("/log/_veredictum/administrative"),
        Some(&json!(true))
    );

    let again = dir.path().join("rejudged");
    let rejudged = rejudge(dir.path(), "admin_ehr_delete", &out, &again)?;
    assert_eq!(rejudged.status.code(), output.status.code(), "{rejudged:?}");
    assert_eq!(
        std::fs::read(again.join("results.json"))?,
        std::fs::read(out.join("results.json"))?
    );
    Ok(())
}

/// The rows of `results` whose case is in document `document`.
fn rows_of<'r>(results: &'r Value, document: &str) -> Vec<&'r Value> {
    let prefix = format!("{document}/");
    results
        .get("rows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|row| {
            row.get("case")
                .and_then(Value::as_str)
                .is_some_and(|case| case.starts_with(&prefix))
        })
        .collect()
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_unclaimed_capability_is_skipped_and_a_claimed_one_still_fails() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let statement = statement(
        dir.path(),
        &[AUTHENTICATION_CAPABILITY],
        &[AUTHENTICATION_ARM],
    )?;
    let drive = |sut: &FakeSut, out: &Path| {
        test_command(
            &[
                "--document",
                AUTHENTICATION,
                "--document",
                DEMOGRAPHIC,
                "--statement",
                &statement,
                "--base-url",
                &sut.base_url(),
                "--out",
                &out.to_string_lossy(),
            ],
            &[],
        )
    };

    let refusing = answering(
        ResponseTemplate::new(401).insert_header("WWW-Authenticate", "Bearer realm=\"cdr\""),
    );
    let green = dir.path().join("green");
    let output = drive(&refusing, &green)?;
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = read_json(&green.join("results.json"))?;
    let unclaimed = rows_of(&results, DEMOGRAPHIC);
    assert!(!unclaimed.is_empty());
    for row in &unclaimed {
        assert_eq!(row.get("status"), Some(&json!("skipped")), "{row}");
        assert_eq!(
            row.pointer("/skip/reason"),
            Some(&json!("not-claimed")),
            "{row}"
        );
        assert_eq!(
            row.pointer("/skip/why/source"),
            Some(&json!("schemas/statement.schema.json")),
            "{row}"
        );
    }
    let claimed = rows_of(&results, AUTHENTICATION);
    assert_eq!(claimed.len(), 5);
    assert!(
        claimed
            .iter()
            .all(|row| row.get("status") == Some(&json!("passed")))
    );
    assert!(
        refusing
            .requests()
            .iter()
            .all(|request| !request.url.path().contains("/demographic/")),
        "an unclaimed document sent a request"
    );

    let accepting = answering(ResponseTemplate::new(200));
    let red = dir.path().join("red");
    let output = drive(&accepting, &red)?;
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let results = read_json(&red.join("results.json"))?;
    let claimed = rows_of(&results, AUTHENTICATION);
    assert!(
        claimed
            .iter()
            .all(|row| row.get("status") == Some(&json!("failed")))
    );
    assert!(
        rows_of(&results, DEMOGRAPHIC)
            .iter()
            .all(|row| row.pointer("/skip/reason") == Some(&json!("not-claimed")))
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_run_that_cannot_start_exits_two() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let out = dir.path().join("run").to_string_lossy().into_owned();
    let written = |name: &str, document: &Value| -> Result<String, Box<dyn std::error::Error>> {
        let path = dir.path().join(name);
        std::fs::write(&path, serde_json::to_string(document)?)?;
        Ok(path.to_string_lossy().into_owned())
    };
    let unknown_arm = written(
        "unknown-arm.json",
        &json!({ "claims": { "capabilities": [AUTHENTICATION_CAPABILITY] },
                 "options": ["authentication-maybe"] }),
    )?;
    let unknown_capability = written(
        "unknown-capability.json",
        &json!({ "claims": { "capabilities": [AUTHENTICATION_CAPABILITY, "EhrOperations"] } }),
    )?;
    let no_claims = written(
        "no-claims.json",
        &json!({ "options": [AUTHENTICATION_ARM] }),
    )?;
    let url = closed_port_url();
    let refusals: [(&[&str], &str); 7] = [
        (
            &[
                "--base-url",
                &url,
                "--out",
                &out,
                "--statement",
                &unknown_capability,
            ],
            "EhrOperations",
        ),
        (
            &["--base-url", &url, "--out", &out, "--statement", &no_claims],
            "claims.capabilities",
        ),
        (
            &["--base-url", &url, "--out", &out, "--auth", "digest"],
            "auth mode",
        ),
        (
            &[
                "--base-url",
                &url,
                "--out",
                &out,
                "--document",
                "no_such_document",
            ],
            "no_such_document",
        ),
        (
            &["--base-url", &url, "--out", &out, "--auth", "basic"],
            "VEREDICTUM_SUT_USER",
        ),
        (
            &[
                "--base-url",
                &url,
                "--out",
                &out,
                "--statement",
                &unknown_arm,
            ],
            "authentication-maybe",
        ),
        (&["--out", &out], "--base-url"),
    ];
    for (args, named) in refusals {
        let output = test_command(args, &[])?;
        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
        let stderr = String::from_utf8(output.stderr)?;
        assert!(stderr.contains(named), "{args:?}: {stderr}");
    }
    assert!(!Path::new(&out).exists(), "a refused run wrote its output");
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_empty_credential_variable_is_refused_like_an_unset_one() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let out = dir.path().join("run").to_string_lossy().into_owned();
    let url = closed_port_url();
    let empties: [&[(&str, &str)]; 2] = [
        &[("VEREDICTUM_SUT_TOKEN", "")],
        &[
            ("VEREDICTUM_SUT_TOKEN", ""),
            ("VEREDICTUM_SUT_TOKEN_FILE", ""),
        ],
    ];
    for envs in empties {
        let output = test_command(
            &["--base-url", &url, "--out", &out, "--auth", "bearer"],
            envs,
        )?;
        assert_eq!(output.status.code(), Some(2), "{envs:?}: {output:?}");
        let stderr = String::from_utf8(output.stderr)?;
        assert!(
            stderr.contains("VEREDICTUM_SUT_TOKEN"),
            "{envs:?}: {stderr}"
        );
    }
    assert!(!Path::new(&out).exists(), "a refused run wrote its output");
    Ok(())
}

/// `run` names no subcommand: the catalogue is driven by `test` alone, and
/// the old spelling is a usage error rather than an alias.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn run_is_not_a_subcommand() -> Fallible {
    let out = Command::new(env!("CARGO_BIN_EXE_veredictum"))
        .args(["run", "--help"])
        .output()?;
    assert_eq!(
        out.status.code(),
        Some(2),
        "`run` must be refused as a usage error"
    );
    let help = Command::new(env!("CARGO_BIN_EXE_veredictum"))
        .arg("--help")
        .output()?;
    let listing = String::from_utf8(help.stdout)?;
    assert!(
        !listing.contains("alias"),
        "no subcommand carries an alias:\n{listing}"
    );
    Ok(())
}
