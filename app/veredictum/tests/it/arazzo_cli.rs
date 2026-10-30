// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `test` subcommand over the committed Arazzo catalogue, against the
//! fake SUT: a passing, a failed and an errored run and their exit statuses,
//! skipped and warning rows staying green, the offline re-judgement
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
use wiremock::matchers::{any, method, path};
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
    "VEREDICTUM_CDR_USER",
    "VEREDICTUM_CDR_USER_FILE",
    "VEREDICTUM_CDR_PASSWORD",
    "VEREDICTUM_CDR_PASSWORD_FILE",
    "VEREDICTUM_CDR_TOKEN",
    "VEREDICTUM_CDR_TOKEN_FILE",
    "VEREDICTUM_CDR_ADMIN_USER",
    "VEREDICTUM_CDR_ADMIN_USER_FILE",
    "VEREDICTUM_CDR_ADMIN_PASSWORD",
    "VEREDICTUM_CDR_ADMIN_PASSWORD_FILE",
    "VEREDICTUM_CDR_ADMIN_TOKEN",
    "VEREDICTUM_CDR_ADMIN_TOKEN_FILE",
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
    let (sent, identity_read) = sut.catalogue_requests();
    assert_eq!(sent.len(), 5);
    assert!(
        identity_read,
        "the run exchanged no System OPTIONS and read none"
    );
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
        stdout.contains("5 row(s): 5 passed, 0 failed, 0 warning, 0 errored, 0 skipped"),
        "{stdout}"
    );
    assert!(
        stdout.contains("\nVeredictum results: "),
        "the run ends with the terminal view: {stdout}"
    );
    assert!(
        !stdout.contains('\u{1b}'),
        "a pipe is not a terminal: {stdout}"
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
fn skipped_and_warning_rows_exit_zero() -> Fallible {
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
    let (sent, identity_read) = sut.catalogue_requests();
    assert!(sent.is_empty(), "a skipped document sent a request");
    assert!(
        identity_read,
        "the run exchanged no System OPTIONS and read none"
    );

    let warning = dir.path().join("warning");
    let output = test_command(
        &[
            "--document",
            "system_options",
            "--base-url",
            &sut.base_url(),
            "--out",
            &warning.to_string_lossy(),
        ],
        &[],
    )?;
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = read_json(&warning.join("results.json"))?;
    assert_eq!(statuses(&results), vec!["failed"; 3]);
    assert_eq!(whose(&results), vec!["warning"; 3]);
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
            ("VEREDICTUM_CDR_ADMIN_USER", "root"),
            ("VEREDICTUM_CDR_ADMIN_PASSWORD_FILE", &password),
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
    let refusals: [(&[&str], &str); 8] = [
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
            "--auth basic needs VEREDICTUM_CDR_USER and VEREDICTUM_CDR_PASSWORD (or their _FILE \
             forms)",
        ),
        (
            &["--base-url", &url, "--out", &out, "--admin-auth", "bearer"],
            "--admin-auth bearer needs VEREDICTUM_CDR_ADMIN_TOKEN (or its _FILE form)",
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
        &[("VEREDICTUM_CDR_TOKEN", "")],
        &[
            ("VEREDICTUM_CDR_TOKEN", ""),
            ("VEREDICTUM_CDR_TOKEN_FILE", ""),
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
            stderr.contains("VEREDICTUM_CDR_TOKEN"),
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

/// The document whose workflows read the EHR in canonical XML, which needs
/// the `canonical-xml` format and the arm that serves it.
const EHR_XML: &str = "ehr_xml";

/// A document needing a format the statement's its-rest profile does not
/// claim is skipped `format-not-claimed` and sends nothing; once the
/// profile claims the format, the same document runs.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_document_needing_an_unclaimed_format_is_skipped_format_not_claimed() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let drive =
        |formats: &[&str], out: &str| -> Result<(FakeSut, Value), Box<dyn std::error::Error>> {
            let path = dir.path().join(format!("{out}.json"));
            let document = json!({
                "claims": { "capabilities": ["ehr"] },
                "options": ["ehr-xml-supported"],
                "tech_profiles": [{ "its": "its-rest", "formats": formats }]
            });
            std::fs::write(&path, serde_json::to_string(&document)?)?;
            let sut = answering(ResponseTemplate::new(500));
            let out = dir.path().join(out);
            test_command(
                &[
                    "--document",
                    EHR_XML,
                    "--statement",
                    &path.to_string_lossy(),
                    "--base-url",
                    &sut.base_url(),
                    "--out",
                    &out.to_string_lossy(),
                ],
                &[],
            )?;
            let results = read_json(&out.join("results.json"))?;
            Ok((sut, results))
        };

    let (sut, results) = drive(&["canonical-json"], "unclaimed")?;
    let rows = rows_of(&results, EHR_XML);
    assert!(!rows.is_empty());
    for row in &rows {
        assert_eq!(row.get("status"), Some(&json!("skipped")), "{row}");
        assert_eq!(
            row.pointer("/skip/reason"),
            Some(&json!("format-not-claimed")),
            "{row}"
        );
        assert_eq!(
            row.pointer("/skip/why/section"),
            Some(&json!("#/properties/tech_profiles")),
            "{row}"
        );
    }
    let (sent, identity_read) = sut.catalogue_requests();
    assert!(sent.is_empty(), "a skipped document sent a request");
    assert!(
        identity_read,
        "the run exchanged no System OPTIONS and read none"
    );

    let (sut, results) = drive(&["canonical-json", "canonical-xml"], "claimed")?;
    let rows = rows_of(&results, EHR_XML);
    assert!(!rows.is_empty());
    assert!(
        rows.iter()
            .all(|row| row.pointer("/skip/reason") != Some(&json!("format-not-claimed"))),
        "a claimed format still skipped the document"
    );
    assert!(
        !sut.requests().is_empty(),
        "the claimed document sent nothing"
    );
    Ok(())
}

/// A fake SUT answering System OPTIONS (`OPTIONS /`) with `options`, and
/// every other request with the `401` challenge [`AUTHENTICATION`] expects.
fn stating(options: &Value) -> FakeSut {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("OPTIONS"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(options.clone()))
            .with_priority(1),
    );
    sut.mount(Mock::given(any()).respond_with(
        ResponseTemplate::new(401).insert_header("WWW-Authenticate", "Bearer realm=\"cdr\""),
    ));
    sut
}

/// A statement selecting [`AUTHENTICATION`] and declaring `spec_versions`,
/// written into `dir`.
fn declaring(dir: &Path, spec_versions: &Value) -> Result<String, Box<dyn std::error::Error>> {
    let file = dir.join("statement.json");
    let document = json!({
        "claims": { "capabilities": [AUTHENTICATION_CAPABILITY] },
        "options": [AUTHENTICATION_ARM],
        "spec_versions": spec_versions
    });
    std::fs::write(&file, serde_json::to_string(&document)?)?;
    Ok(file.to_string_lossy().into_owned())
}

/// Drives [`AUTHENTICATION`] under `statement` against `sut` into `out`.
fn drive_declared(
    statement: &str,
    sut: &FakeSut,
    out: &Path,
) -> Result<Output, Box<dyn std::error::Error>> {
    Ok(test_command(
        &[
            "--document",
            AUTHENTICATION,
            "--statement",
            statement,
            "--base-url",
            &sut.base_url(),
            "--out",
            &out.to_string_lossy(),
        ],
        &[],
    )?)
}

/// The violations of `results` against the published results schema.
fn results_violations(results: &Value) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let validator = jsonschema::validator_for(&veredictum::schema::arazzo_results_schema())?;
    Ok(validator
        .iter_errors(results)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect())
}

/// A run that exchanges no System OPTIONS reads it once after the catalogue,
/// records the server's `restapi_specs_version` in one place, and notes that
/// it differs from the statement's `spec_versions.its_rest` without changing
/// a row. The re-judgement asks the server nothing and writes the same bytes.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_stated_rest_release_is_recorded_once_and_reviewed_against_the_statement() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let out = dir.path().join("run");
    let sut = stating(&json!({ "solution": "cdr", "restapi_specs_version": "1.0.3" }));
    let statement = declaring(dir.path(), &json!({ "its_rest": "1.1.0" }))?;
    let output = drive_declared(&statement, &sut, &out)?;
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = read_json(&out.join("results.json"))?;
    assert_eq!(statuses(&results), vec!["passed"; 5]);
    assert_eq!(results_violations(&results)?, Vec::<String>::new());
    assert_eq!(results.pointer("/server/read"), Some(&json!("read")));
    assert_eq!(
        results.pointer("/server/answer/options/restapi_specs_version"),
        Some(&json!("1.0.3"))
    );
    assert_eq!(
        results.get("review"),
        Some(&json!([{
            "kind": "restapi-version-differs",
            "declared": "1.1.0",
            "stated": "1.0.3"
        }]))
    );
    let text = std::fs::read_to_string(out.join("results.json"))?;
    assert_eq!(text.matches("restapi_specs_version").count(), 1, "{text}");
    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        stdout.contains("REVIEW   restapi-version-differs"),
        "{stdout}"
    );
    let recording = read_json(&out.join("recording.har"))?;
    assert_eq!(
        recording.pointer("/log/_veredictum/identity"),
        results.get("server")
    );

    let asked = sut.requests().len();
    let again = dir.path().join("rejudged");
    let rejudged = rejudge(dir.path(), AUTHENTICATION, &out, &again)?;
    assert_eq!(rejudged.status.code(), Some(0), "{rejudged:?}");
    assert_eq!(
        std::fs::read(again.join("results.json"))?,
        text.into_bytes()
    );
    assert_eq!(
        sut.requests().len(),
        asked,
        "the re-judgement asked the server"
    );
    Ok(())
}

/// A server that does not state `restapi_specs_version` leaves it absent:
/// nothing fills it in, and with nothing to compare the review notes
/// nothing. The same holds for a server that does not offer System OPTIONS.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_unstated_rest_release_stays_absent_and_is_not_reviewed() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let statement = declaring(dir.path(), &json!({ "its_rest": "1.1.0" }))?;

    let out = dir.path().join("unstated");
    let sut = stating(&json!({ "solution": "cdr" }));
    let output = drive_declared(&statement, &sut, &out)?;
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = read_json(&out.join("results.json"))?;
    assert_eq!(results_violations(&results)?, Vec::<String>::new());
    assert_eq!(
        results.pointer("/server/answer"),
        Some(&json!({ "outcome": "offered", "status": 200, "options": { "solution": "cdr" } }))
    );
    assert_eq!(results.get("review"), None);
    let again = dir.path().join("unstated-rejudged");
    let rejudged = rejudge(dir.path(), AUTHENTICATION, &out, &again)?;
    assert_eq!(rejudged.status.code(), Some(0), "{rejudged:?}");
    assert_eq!(
        std::fs::read(again.join("results.json"))?,
        std::fs::read(out.join("results.json"))?
    );

    let out = dir.path().join("not-offered");
    let sut = answering(
        ResponseTemplate::new(401).insert_header("WWW-Authenticate", "Bearer realm=\"cdr\""),
    );
    let output = drive_declared(&statement, &sut, &out)?;
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = read_json(&out.join("results.json"))?;
    assert_eq!(results_violations(&results)?, Vec::<String>::new());
    assert_eq!(
        results.get("server"),
        Some(&json!({ "read": "read", "answer": { "outcome": "not_offered", "status": 401 } }))
    );
    assert_eq!(results.get("review"), None);
    Ok(())
}

/// A run whose own workflows exchange System OPTIONS records the identity
/// from that exchange, naming its recording entry, and sends no read of its
/// own; a statement declaring the stated release gets no note.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_run_that_exchanges_system_options_records_that_exchange() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let out = dir.path().join("run");
    let sut = stating(&json!({ "restapi_specs_version": "1.0.3" }));
    let statement = dir.path().join("statement.json");
    std::fs::write(
        &statement,
        serde_json::to_string(&json!({
            "claims": { "capabilities": ["system"] },
            "spec_versions": { "its_rest": "1.0.3" }
        }))?,
    )?;
    let output = test_command(
        &[
            "--document",
            "system_options",
            "--statement",
            &statement.to_string_lossy(),
            "--base-url",
            &sut.base_url(),
            "--out",
            &out.to_string_lossy(),
        ],
        &[],
    )?;
    let results = read_json(&out.join("results.json"))?;
    assert_eq!(results_violations(&results)?, Vec::<String>::new());
    assert_eq!(results.pointer("/server/read"), Some(&json!("exchange")));
    assert_eq!(results.pointer("/server/entry"), Some(&json!(0)));
    assert_eq!(
        results.pointer("/server/answer/options/restapi_specs_version"),
        Some(&json!("1.0.3"))
    );
    assert_eq!(results.get("review"), None);
    let recording = read_json(&out.join("recording.har"))?;
    assert_eq!(recording.pointer("/log/_veredictum/identity"), None);
    assert_eq!(
        Some(sut.requests().len()),
        recording
            .pointer("/log/entries")
            .and_then(Value::as_array)
            .map(Vec::len),
        "the run sent a read of its own"
    );

    let again = dir.path().join("rejudged");
    let rejudged = rejudge(dir.path(), "system_options", &out, &again)?;
    assert_eq!(rejudged.status.code(), output.status.code(), "{rejudged:?}");
    assert_eq!(
        std::fs::read(again.join("results.json"))?,
        std::fs::read(out.join("results.json"))?
    );
    Ok(())
}
