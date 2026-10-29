// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The `test` subcommand orders every `server: exclusive` workflow after
//! every other workflow of the run, over a catalogue root of its own.
//!
//! The root holds a document whose exclusive workflow deletes every EHR and
//! sorts first by path, a workflow in the same document, and a document in
//! another folder, both of which read an EHR the server already holds. Run
//! in path order, the delete would take that EHR away from both readers. No
//! openEHR spec governs the order; it is this instrument's own design.

use std::path::Path;
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, Request, Respond, ResponseTemplate};

use crate::fake_sut::FakeSut;

/// Anything a run or a fixture read can fail with.
type Fallible = Result<(), Box<dyn std::error::Error>>;

/// The EHR the fake server holds before the run starts.
const HELD: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

/// The citation the bulk delete's criteria carry, verbatim from the
/// vendored operation text.
const DELETE_CITE: &str = r#"
      source: specs/openehr/ITS-REST/specifications/operations/admin_ehr_delete_all.yaml
      section: description
      quote: "If the deletion is processed synchronously and completes successfully, the server returns status `204 No Content`.""#;

/// A workflow that reads [`HELD`] and expects it to be there.
const READ_HELD: &str = r#"
  - workflowId: held
    summary: The EHR the server holds is retrieved
    x-veredictum-cite:
      source: specs/openehr/ITS-REST/specifications/responses/200_EHR.yaml
      section: description
      quote: "`200 OK` is returned when the requested EHR resource is successfully retrieved."
    steps:
      - stepId: get
        operationId: $sourceDescriptions.ehr.ehr_get_by_id
        parameters:
          - { name: ehr_id, in: path, value: HELD_ID }
          - { name: Accept, in: header, value: application/json }
        successCriteria:
          - condition: $statusCode == 200
"#;

/// The head of every document: the ITS-REST sources the steps call.
const HEAD: &str = r"arazzo: 1.1.0
info: { title: Exclusive ordering, version: 0.1.0 }
sourceDescriptions:
  - { name: admin, url: ../../../specs/rest-oas/admin-validation.openapi.yaml, type: openapi }
  - { name: ehr, url: ../../../specs/rest-oas/ehr-validation.openapi.yaml, type: openapi }
workflows:";

/// The fake server's one piece of state: whether every EHR was deleted.
#[derive(Clone)]
struct Wiped(Arc<AtomicBool>);

/// The bulk delete: every EHR is gone afterwards.
struct DeleteAll(Wiped);

impl Respond for DeleteAll {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        self.0.0.store(true, Ordering::SeqCst);
        ResponseTemplate::new(204)
    }
}

/// A read of [`HELD`]: there until the bulk delete.
struct ReadHeld(Wiped);

impl Respond for ReadHeld {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        if self.0.0.load(Ordering::SeqCst) {
            ResponseTemplate::new(404)
        } else {
            ResponseTemplate::new(200).set_body_json(json!({ "ehr_id": { "value": HELD } }))
        }
    }
}

/// Writes `text` at `relative` under `root`, making its folders.
fn write(root: &Path, relative: &str, text: &str) -> std::io::Result<()> {
    let file = root.join(relative);
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(file, text)
}

/// A catalogue root under `dir`, beside a link to the vendored `specs/`.
#[cfg(unix)]
fn catalogue(dir: &Path) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::os::unix::fs::symlink(repo.join("specs").canonicalize()?, dir.join("specs"))?;
    let root = dir.join("catalogue");
    write(
        &root,
        "cases/surface.yaml",
        "scope:\n  - { api: system, tag: Options }\n",
    )?;
    write(
        &root,
        "cases/system/system_options.arazzo.yaml",
        &std::fs::read_to_string(repo.join("catalogue/cases/system/system_options.arazzo.yaml"))?,
    )?;
    let read_held = READ_HELD.replace("HELD_ID", HELD);
    let wipe = format!(
        r"{HEAD}
  - workflowId: wipe
    summary: A bulk delete naming no EHR deletes every EHR
    x-veredictum-cite:{DELETE_CITE}
    inputs:
      type: object
      properties:
        ehr_id: {{ type: string }}
    steps:
      - stepId: delete
        operationId: $sourceDescriptions.admin.admin_ehr_delete_all
        x-veredictum-principal: administrative
        successCriteria:
          - condition: $statusCode == 204
      - stepId: get
        operationId: $sourceDescriptions.ehr.ehr_get_by_id
        parameters:
          - {{ name: ehr_id, in: path, value: $inputs.ehr_id }}
          - {{ name: Accept, in: header, value: application/json }}
        successCriteria:
          - condition: $statusCode == 404
            x-veredictum-cite:
              source: specs/openehr/ITS-REST/specifications/responses/404_unknown_ehr_id.yaml
              section: description
              quote: '`404 Not Found` is returned when an EHR with `ehr_id` does not exist.'
{read_held}"
    );
    write(&root, "cases/admin/a_wipe.arazzo.yaml", &wipe)?;
    write(
        &root,
        "cases/admin/wipe.rows.yaml",
        &format!(
            "workflow: a_wipe/wipe\niteration: reset_per_row\nserver: exclusive\nrows:\n  - {{ ehr_id: {HELD} }}\n"
        ),
    )?;
    write(
        &root,
        "cases/ehr/b_read.arazzo.yaml",
        &format!("{HEAD}{read_held}"),
    )?;
    Ok(root)
}

/// One `veredictum test` invocation over `root` and the two documents.
fn test_command(root: &Path, args: &[&str]) -> Result<Output, std::io::Error> {
    Command::new(env!("CARGO_BIN_EXE_veredictum"))
        .arg("test")
        .arg("--root")
        .arg(root)
        .args(["--document", "a_wipe", "--document", "b_read"])
        .args(args)
        .env_remove("VEREDICTUM_CATALOGUE")
        .env("VEREDICTUM_CDR_ADMIN_TOKEN", "admin-token")
        .output()
}

/// The case and status of every row of a results document, in order.
fn rows(results: &Value) -> Vec<(String, String)> {
    let field = |row: &Value, name: &str| {
        row.get(name)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    results
        .get("rows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|row| (field(row, "case"), field(row, "status")))
        .collect()
}

#[test]
#[cfg(unix)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_exclusive_workflow_runs_after_every_other_and_its_rejudgement_writes_the_same_bytes()
-> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let root = catalogue(dir.path())?;
    let ixit = dir.path().join("ixit.json");
    std::fs::write(&ixit, r#"{ "environment": { "exclusive_server": true } }"#)?;
    let ixit = ixit.to_string_lossy().into_owned();
    let wiped = Wiped(Arc::new(AtomicBool::new(false)));
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("DELETE"))
            .and(path("/admin/ehr/all"))
            .respond_with(DeleteAll(wiped.clone())),
    );
    sut.mount(
        Mock::given(method("GET"))
            .and(path(format!("/ehr/{HELD}")))
            .respond_with(ReadHeld(wiped)),
    );

    let out = dir.path().join("run");
    let output = test_command(
        &root,
        &[
            "--ixit",
            &ixit,
            "--admin-auth",
            "bearer",
            "--base-url",
            &sut.base_url(),
            "--out",
            &out.to_string_lossy(),
        ],
    )?;
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results: Value = serde_json::from_str(&std::fs::read_to_string(out.join("results.json"))?)?;
    let passed = |case: &str| (case.to_owned(), "passed".to_owned());
    assert_eq!(
        rows(&results),
        vec![
            passed("a_wipe/held"),
            passed("b_read/held"),
            passed("a_wipe/wipe")
        ]
    );
    let sent: Vec<String> = sut
        .requests()
        .iter()
        .map(|request| request.method.to_string())
        .collect();
    assert_eq!(sent, ["GET", "GET", "DELETE", "GET"]);

    let again = dir.path().join("rejudged");
    let rejudged = test_command(
        &root,
        &[
            "--ixit",
            &ixit,
            "--rejudge",
            &out.join("recording.har").to_string_lossy(),
            "--out",
            &again.to_string_lossy(),
        ],
    )?;
    assert_eq!(rejudged.status.code(), Some(0), "{rejudged:?}");
    assert_eq!(
        std::fs::read(again.join("results.json"))?,
        std::fs::read(out.join("results.json"))?
    );
    Ok(())
}

/// An IXIT the published IXIT schema refuses stops `test` with exit 2 before
/// any request, naming the file and the violation; a misspelled member is
/// never read as an undeclared one.
#[test]
#[cfg(unix)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_schema_invalid_ixit_stops_the_run_naming_the_problem() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let root = catalogue(dir.path())?;
    let sut = FakeSut::start();
    for (document, named) in [
        (
            r#"{ "environment": { "exclusive_servr": true } }"#,
            "exclusive_servr",
        ),
        (
            r#"{ "environment": { "exclusive_server": "yes" } }"#,
            "/environment/exclusive_server",
        ),
        (r#"{ "instance": {} }"#, "instance"),
    ] {
        let ixit = dir.path().join("ixit.json");
        std::fs::write(&ixit, document)?;
        let out = dir.path().join("run");
        let output = test_command(
            &root,
            &[
                "--ixit",
                &ixit.to_string_lossy(),
                "--admin-auth",
                "bearer",
                "--base-url",
                &sut.base_url(),
                "--out",
                &out.to_string_lossy(),
            ],
        )?;
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("ixit.json: schema:"), "{stderr}");
        assert!(stderr.contains(named), "{named}: {stderr}");
        assert!(!out.join("results.json").exists());
    }
    assert!(sut.requests().is_empty());
    Ok(())
}
