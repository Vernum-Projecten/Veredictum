// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `test` subcommand over the committed SMART on openEHR documents,
//! against the fake SUT: the discovery document read from the Platform base
//! URL, each scoped row sent with the token the IXIT supplies for its scope
//! set, a minted token carrying the row's scope claim, no token in the
//! recording, the offline re-judgement reproducing the recorded bytes, and
//! the skips and refusals of a party that declares less.

use std::path::Path;
use std::process::{Command, Output};

use base64::Engine as _;
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::fake_sut::FakeSut;

/// Anything a run or a fixture read can fail with.
type Fallible = Result<(), Box<dyn std::error::Error>>;

/// The two SMART documents of the committed catalogue.
const DOCUMENTS: [&str; 2] = ["smart_discovery", "smart_scopes"];

/// The scope sets of the granted-scope rows, in row order.
const READ_SCOPE: &str = "system/template-*.r";
const CRUD_SCOPE: &str = "system/template-*.crud";

/// The scope sets of the read-and-search rows, in row order.
const READ_SEARCH_SCOPE: &str = "system/template-*.rs";
const CRUDS_SCOPE: &str = "system/template-*.cruds";

/// The token supplied for [`READ_SCOPE`], from a variable.
const READ_TOKEN: &str = "tok-read-7f3a";

/// The token supplied for [`CRUD_SCOPE`], from a file.
const CRUD_TOKEN: &str = "tok-crud-91bc";

/// Every variable a run reads a credential or the root from, removed so the
/// developer's or the runner's own environment never reaches a test.
const READ_VARS: [&str; 13] = [
    "VEREDICTUM_CATALOGUE",
    "VEREDICTUM_CDR_TOKEN",
    "VEREDICTUM_CDR_TOKEN_FILE",
    "VEREDICTUM_SCOPED_TOKEN_READ",
    "VEREDICTUM_SCOPED_TOKEN_READ_FILE",
    "VEREDICTUM_SCOPED_TOKEN_CRUD",
    "VEREDICTUM_SCOPED_TOKEN_CRUD_FILE",
    "VEREDICTUM_SCOPED_TOKEN_SEARCH",
    "VEREDICTUM_SCOPED_TOKEN_SEARCH_FILE",
    "VEREDICTUM_SCOPED_TOKEN_CRUDS",
    "VEREDICTUM_SCOPED_TOKEN_CRUDS_FILE",
    "VEREDICTUM_SCOPED_TOKEN_KEY",
    "VEREDICTUM_SCOPED_TOKEN_KEY_FILE",
];

/// One `veredictum test` invocation over the committed catalogue, naming
/// both SMART documents.
fn test_command(args: &[&str], envs: &[(&str, &str)]) -> Result<Output, std::io::Error> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_veredictum"));
    command
        .arg("test")
        .arg("--root")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/../../catalogue"));
    for document in DOCUMENTS {
        command.args(["--document", document]);
    }
    command.args(args);
    for var in READ_VARS {
        command.env_remove(var);
    }
    command.envs(envs.iter().copied()).output()
}

/// The fake Platform and CDR: the discovery document under `/platform`, and
/// an empty ADL 1.4 template list under `/openehr/v1`.
fn platform() -> FakeSut {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/platform/.well-known/smart-configuration"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(
                    json!({
                        "issuer": "https://platform.example",
                        "services": {
                            "org.openehr.rest": { "baseUrl": "https://platform.example/openehr/v1" }
                        },
                        "capabilities": ["openehr-permission-v1"]
                    })
                    .to_string(),
                    "application/json",
                ),
            ),
    );
    sut.mount(
        Mock::given(method("GET"))
            .and(path("/openehr/v1/definition/template/adl1.4"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("[]", "application/json")),
    );
    sut
}

/// Writes `value` as JSON at `name` in `dir`, returning its path.
fn write_json(dir: &Path, name: &str, value: &Value) -> Result<String, Box<dyn std::error::Error>> {
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_string(value)?)?;
    Ok(path.to_string_lossy().into_owned())
}

fn read_json(path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

/// `(case, row, status, skip reason)` of every row of a results document.
fn rows(results: &Value) -> Vec<(String, u64, String, Option<String>)> {
    results
        .get("rows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|row| {
            let text = |pointer: &str| row.pointer(pointer).and_then(Value::as_str);
            (
                text("/case").unwrap_or_default().to_owned(),
                row.get("row").and_then(Value::as_u64).unwrap_or(u64::MAX),
                text("/status").unwrap_or_default().to_owned(),
                text("/skip/reason").map(str::to_owned),
            )
        })
        .collect()
}

/// The `Authorization` value of every request the fake SUT received on
/// `route`, in order; `None` for a request that carried none.
fn authorizations(sut: &FakeSut, route: &str) -> Vec<Option<String>> {
    sut.requests()
        .iter()
        .filter(|request| request.url.path() == route)
        .map(|request| {
            request
                .headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        })
        .collect()
}

/// A statement claiming the `smart` capability folder.
fn statement(dir: &Path) -> Result<String, Box<dyn std::error::Error>> {
    write_json(
        dir,
        "statement.json",
        &json!({ "claims": { "capabilities": ["smart"] }, "options": [] }),
    )
}

/// An IXIT supplying a token for each scope set of the granted-scope rows.
fn issued_ixit(dir: &Path) -> Result<String, Box<dyn std::error::Error>> {
    write_json(
        dir,
        "ixit.json",
        &json!({ "scoped_tokens": { "issued": {
            READ_SCOPE: "VEREDICTUM_SCOPED_TOKEN_READ",
            CRUD_SCOPE: "VEREDICTUM_SCOPED_TOKEN_CRUD"
        }}}),
    )
}

/// A read-and-search row skipped because the IXIT supplies no token for its
/// scope set.
fn undeclared_read_search(row: u64) -> (String, u64, String, Option<String>) {
    (
        "smart_scopes/template_list-system_scope_read_search_granted".to_owned(),
        row,
        "skipped".to_owned(),
        Some("not-declared".to_owned()),
    )
}

/// The run's recording, after checking that neither supplied token nor a
/// scope marker reached it, the results or the run's own output.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn no_token_in(output: &Output, out: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let recording = std::fs::read_to_string(out.join("recording.har"))?;
    let results = std::fs::read_to_string(out.join("results.json"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let needles = [READ_TOKEN, CRUD_TOKEN, "a token whose scope claim"];
    for (index, needle) in needles.into_iter().enumerate() {
        assert!(
            !recording.contains(needle),
            "the recording holds needle {index}"
        );
        assert!(!results.contains(needle), "the results hold needle {index}");
        assert!(
            !stdout.contains(needle) && !stderr.contains(needle),
            "the run printed needle {index}"
        );
    }
    Ok(serde_json::from_str(&recording)?)
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn each_scoped_row_presents_its_own_token_and_the_recording_holds_none() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let sut = platform();
    let statement = statement(dir.path())?;
    let ixit = issued_ixit(dir.path())?;
    let crud_file = dir.path().join("crud-token");
    std::fs::write(&crud_file, format!("{CRUD_TOKEN}\n"))?;
    let crud_file = crud_file.to_string_lossy().into_owned();
    let out = dir.path().join("run");
    let base_url = format!("{}/openehr/v1", sut.base_url());
    let platform_url = format!("{}/platform", sut.base_url());
    let output = test_command(
        &[
            "--statement",
            &statement,
            "--ixit",
            &ixit,
            "--base-url",
            &base_url,
            "--platform-url",
            &platform_url,
            "--out",
            &out.to_string_lossy(),
        ],
        &[
            ("VEREDICTUM_SCOPED_TOKEN_READ", READ_TOKEN),
            ("VEREDICTUM_SCOPED_TOKEN_CRUD_FILE", &crud_file),
        ],
    )?;
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = read_json(&out.join("results.json"))?;
    assert_eq!(
        rows(&results),
        [
            (
                "smart_discovery/smart_configuration-document".to_owned(),
                0,
                "passed".to_owned(),
                None
            ),
            (
                "smart_scopes/template_list-system_scope_granted".to_owned(),
                0,
                "passed".to_owned(),
                None
            ),
            (
                "smart_scopes/template_list-system_scope_granted".to_owned(),
                1,
                "passed".to_owned(),
                None
            ),
            undeclared_read_search(0),
            undeclared_read_search(1),
        ]
    );
    assert_eq!(
        authorizations(&sut, "/openehr/v1/definition/template/adl1.4"),
        [
            Some(format!("Bearer {READ_TOKEN}")),
            Some(format!("Bearer {CRUD_TOKEN}"))
        ]
    );
    assert_eq!(
        authorizations(&sut, "/platform/.well-known/smart-configuration"),
        [None]
    );

    let recording = no_token_in(&output, &out)?;
    assert_eq!(
        recording.pointer("/log/_veredictum/platformUrl"),
        Some(&json!(platform_url))
    );
    assert_eq!(
        recording.pointer("/log/_veredictum/scoped"),
        Some(&json!({ "issued": [CRUD_SCOPE, READ_SCOPE] }))
    );

    let again = dir.path().join("rejudged");
    let rejudged = test_command(
        &[
            "--statement",
            &statement,
            "--ixit",
            &ixit,
            "--rejudge",
            &out.join("recording.har").to_string_lossy(),
            "--out",
            &again.to_string_lossy(),
        ],
        &[],
    )?;
    assert_eq!(rejudged.status.code(), Some(0), "{rejudged:?}");
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
fn what_the_party_does_not_declare_is_skipped_not_declared() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let sut = platform();
    let statement = statement(dir.path())?;
    let ixit = write_json(
        dir.path(),
        "ixit.json",
        &json!({ "scoped_tokens": { "issued": { READ_SCOPE: "VEREDICTUM_SCOPED_TOKEN_READ" } } }),
    )?;
    let out = dir.path().join("run");
    let base_url = format!("{}/openehr/v1", sut.base_url());
    let output = test_command(
        &[
            "--statement",
            &statement,
            "--ixit",
            &ixit,
            "--base-url",
            &base_url,
            "--out",
            &out.to_string_lossy(),
        ],
        &[("VEREDICTUM_SCOPED_TOKEN_READ", READ_TOKEN)],
    )?;
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = read_json(&out.join("results.json"))?;
    let outcome: Vec<(String, Option<String>)> = rows(&results)
        .into_iter()
        .map(|(_, _, status, reason)| (status, reason))
        .collect();
    let skipped = || ("skipped".to_owned(), Some("not-declared".to_owned()));
    assert_eq!(
        outcome,
        [
            skipped(),
            ("passed".to_owned(), None),
            skipped(),
            skipped(),
            skipped()
        ]
    );
    assert_eq!(
        results.pointer("/rows/0/skip/why/section"),
        Some(&json!("Service Discovery"))
    );
    assert_eq!(
        results.pointer("/rows/2/skip/why/section"),
        Some(&json!("Scopes"))
    );
    assert!(authorizations(&sut, "/platform/.well-known/smart-configuration").is_empty());
    assert_eq!(
        authorizations(&sut, "/openehr/v1/definition/template/adl1.4"),
        [Some(format!("Bearer {READ_TOKEN}"))]
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_declared_token_that_is_not_in_the_environment_stops_the_run_before_any_request() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let sut = platform();
    let statement = statement(dir.path())?;
    let ixit = issued_ixit(dir.path())?;
    let base_url = format!("{}/openehr/v1", sut.base_url());
    let output = test_command(
        &[
            "--statement",
            &statement,
            "--ixit",
            &ixit,
            "--base-url",
            &base_url,
            "--out",
            &dir.path().join("run").to_string_lossy(),
        ],
        &[("VEREDICTUM_SCOPED_TOKEN_READ", READ_TOKEN)],
    )?;
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("VEREDICTUM_SCOPED_TOKEN_CRUD"), "{stderr}");
    assert!(!stderr.contains(READ_TOKEN), "{stderr}");
    assert!(sut.requests().is_empty());
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_declared_test_issuer_mints_each_row_a_token_with_its_scope_claim() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let sut = platform();
    let statement = statement(dir.path())?;
    let ixit = write_json(
        dir.path(),
        "ixit.json",
        &json!({ "scoped_tokens": { "mint": {
            "issuer": "https://issuer.example", "subject": "veredictum", "audience": "cdr",
            "kid": "cnf-smart-test", "ttl_seconds": 300
        }}}),
    )?;
    let key = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../catalogue/keys/smart-test-issuer/cnf-smart-test.key.pem");
    let out = dir.path().join("run");
    let base_url = format!("{}/openehr/v1", sut.base_url());
    let output = test_command(
        &[
            "--statement",
            &statement,
            "--ixit",
            &ixit,
            "--base-url",
            &base_url,
            "--out",
            &out.to_string_lossy(),
        ],
        &[("VEREDICTUM_SCOPED_TOKEN_KEY_FILE", &key.to_string_lossy())],
    )?;
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let sent = authorizations(&sut, "/openehr/v1/definition/template/adl1.4");
    let scopes: Vec<Value> = sent
        .iter()
        .map(|value| -> Result<Value, Box<dyn std::error::Error>> {
            let token = value
                .as_deref()
                .and_then(|value| value.strip_prefix("Bearer "))
                .ok_or("no bearer token")?;
            let claims = token.split('.').nth(1).ok_or("not a JWT")?;
            let claims: Value = serde_json::from_slice(
                &base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(claims)?,
            )?;
            Ok(claims.get("scope").cloned().unwrap_or(Value::Null))
        })
        .collect::<Result<_, _>>()?;
    assert_eq!(
        scopes,
        [
            json!(READ_SCOPE),
            json!(CRUD_SCOPE),
            json!(READ_SEARCH_SCOPE),
            json!(CRUDS_SCOPE)
        ]
    );
    let recording = std::fs::read_to_string(out.join("recording.har"))?;
    for token in sent.iter().flatten() {
        assert!(!recording.contains(token.as_str()));
    }
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_ixit_naming_a_variable_outside_the_scoped_token_prefix_is_refused() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let sut = platform();
    let statement = statement(dir.path())?;
    let ixit = write_json(
        dir.path(),
        "ixit.json",
        &json!({ "scoped_tokens": { "issued": { READ_SCOPE: "HOME" } } }),
    )?;
    let base_url = format!("{}/openehr/v1", sut.base_url());
    let output = test_command(
        &[
            "--statement",
            &statement,
            "--ixit",
            &ixit,
            "--base-url",
            &base_url,
            "--out",
            &dir.path().join("run").to_string_lossy(),
        ],
        &[],
    )?;
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(sut.requests().is_empty());
    Ok(())
}
/// A CDR answering the template list per presented token: one reading of
/// master08-scopes.adoc §Resource Scopes, which maps no scope to an
/// operation (upstream report #758).
fn scope_reading_cdr(tokens: [(&str, u16); 4]) -> FakeSut {
    let sut = FakeSut::start();
    for (token, status) in tokens {
        let answer = if status == 200 {
            ResponseTemplate::new(200).set_body_raw("[]", "application/json")
        } else {
            ResponseTemplate::new(status)
        };
        sut.mount(
            Mock::given(method("GET"))
                .and(path("/openehr/v1/definition/template/adl1.4"))
                .and(header("authorization", format!("Bearer {token}").as_str()))
                .respond_with(answer),
        );
    }
    sut
}

/// The token supplied for [`READ_SEARCH_SCOPE`].
const SEARCH_TOKEN: &str = "tok-search-5d2e";

/// The token supplied for [`CRUDS_SCOPE`].
const CRUDS_TOKEN: &str = "tok-cruds-0a6f";

/// `(case, row, status)` of each scoped row of a run, and its exit code.
type ScopedRun = (Vec<(String, u64, String)>, Option<i32>);

/// Runs both SMART documents against `sut` with a token for every scope set,
/// returning `(case, row, status)` of each scoped row and the exit code.
fn every_scope_run(sut: &FakeSut) -> Result<ScopedRun, Box<dyn std::error::Error>> {
    let dir = assert_fs::TempDir::new()?;
    let statement = statement(dir.path())?;
    let ixit = write_json(
        dir.path(),
        "ixit.json",
        &json!({ "scoped_tokens": { "issued": {
            READ_SCOPE: "VEREDICTUM_SCOPED_TOKEN_READ",
            CRUD_SCOPE: "VEREDICTUM_SCOPED_TOKEN_CRUD",
            READ_SEARCH_SCOPE: "VEREDICTUM_SCOPED_TOKEN_SEARCH",
            CRUDS_SCOPE: "VEREDICTUM_SCOPED_TOKEN_CRUDS"
        }}}),
    )?;
    let out = dir.path().join("run");
    let base_url = format!("{}/openehr/v1", sut.base_url());
    let output = test_command(
        &[
            "--statement",
            &statement,
            "--ixit",
            &ixit,
            "--base-url",
            &base_url,
            "--out",
            &out.to_string_lossy(),
        ],
        &[
            ("VEREDICTUM_SCOPED_TOKEN_READ", READ_TOKEN),
            ("VEREDICTUM_SCOPED_TOKEN_CRUD", CRUD_TOKEN),
            ("VEREDICTUM_SCOPED_TOKEN_SEARCH", SEARCH_TOKEN),
            ("VEREDICTUM_SCOPED_TOKEN_CRUDS", CRUDS_TOKEN),
        ],
    )?;
    let results = read_json(&out.join("results.json"))?;
    let scoped = rows(&results)
        .into_iter()
        .filter(|(case, ..)| case.starts_with("smart_scopes/"))
        .map(|(case, row, status, _)| (case, row, status))
        .collect();
    Ok((scoped, output.status.code()))
}

/// `(case, row, status)` of one scoped row.
fn scoped_row(workflow: &str, row: u64, status: &str) -> (String, u64, String) {
    (format!("smart_scopes/{workflow}"), row, status.to_owned())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_cdr_that_lists_for_search_and_refuses_read_alone_passes_every_row() -> Fallible {
    let sut = scope_reading_cdr([
        (READ_TOKEN, 403),
        (CRUD_TOKEN, 403),
        (SEARCH_TOKEN, 200),
        (CRUDS_TOKEN, 200),
    ]);
    let (scoped, code) = every_scope_run(&sut)?;
    assert_eq!(code, Some(0));
    assert_eq!(
        scoped,
        [
            scoped_row("template_list-system_scope_granted", 0, "passed"),
            scoped_row("template_list-system_scope_granted", 1, "passed"),
            scoped_row(
                "template_list-system_scope_read_search_granted",
                0,
                "passed"
            ),
            scoped_row(
                "template_list-system_scope_read_search_granted",
                1,
                "passed"
            ),
        ]
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_cdr_that_refuses_a_read_and_search_scope_or_errs_fails_the_row() -> Fallible {
    let sut = scope_reading_cdr([
        (READ_TOKEN, 403),
        (CRUD_TOKEN, 500),
        (SEARCH_TOKEN, 403),
        (CRUDS_TOKEN, 200),
    ]);
    let (scoped, code) = every_scope_run(&sut)?;
    assert_eq!(code, Some(1));
    assert_eq!(
        scoped,
        [
            scoped_row("template_list-system_scope_granted", 0, "passed"),
            scoped_row("template_list-system_scope_granted", 1, "failed"),
            scoped_row(
                "template_list-system_scope_read_search_granted",
                0,
                "failed"
            ),
            scoped_row(
                "template_list-system_scope_read_search_granted",
                1,
                "passed"
            ),
        ]
    );
    Ok(())
}
