// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The server's identity read through ITS-REST System OPTIONS
//! (`specifications/operations/options.yaml`, "Options and Conformance"),
//! against the fake SUT: the `identify` seam and its binary over every answer
//! shape, and `run`'s merge of what the server states with what the operator
//! supplies.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::json;
use veredictum::bench::client::AuthKind;
use veredictum::identity::{IdentityRead, identify};
use veredictum::party::{IdentityField, IdentitySource};
use veredictum::pipeline::conformance::{RunOutcome, RunRequest, execute_run};
use veredictum::transcript::Recording;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::fake_sut::FakeSut;

/// Anything a seam or a fixture read can fail with.
type Fallible = Result<(), Box<dyn std::error::Error>>;

/// A filter no catalogue case matches, so a run reads the identity, drives
/// nothing, and finishes in the time the catalogue takes to load.
const NO_CASE: &str = "NO_CASE_MATCHES_THIS_FILTER";

fn repo_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// A fake SUT answering `OPTIONS /` with `answer`, and only when the request
/// asks for JSON.
fn serving(answer: ResponseTemplate) -> FakeSut {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("OPTIONS"))
            .and(path("/"))
            .and(header("accept", "application/json"))
            .respond_with(answer),
    );
    sut
}

fn full_document() -> serde_json::Value {
    json!({
        "solution": "openEHRSys",
        "solution_version": "v1.0",
        "vendor": "My-openEHR",
        "restapi_specs_version": "1.1.0",
        "conformance_profile": "STANDARD",
        "endpoints": ["/ehr", "/demographic", "/definition", "/query", "/admin"]
    })
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_full_options_document_is_read_member_for_member() -> Fallible {
    let sut = serving(ResponseTemplate::new(200).set_body_json(full_document()));
    let read = identify(&sut.base_url(), AuthKind::None)?;
    let IdentityRead::Offered { status, options } = read else {
        return Err(format!("expected an offered document, got {read:?}").into());
    };
    assert_eq!(status, 200);
    assert_eq!(options.solution.as_deref(), Some("openEHRSys"));
    assert_eq!(options.solution_version.as_deref(), Some("v1.0"));
    assert_eq!(options.vendor.as_deref(), Some("My-openEHR"));
    assert_eq!(options.restapi_specs_version.as_deref(), Some("1.1.0"));
    assert_eq!(options.conformance_profile.as_deref(), Some("STANDARD"));
    assert_eq!(options.endpoints.map(|e| e.len()), Some(5));
    let requests = sut.requests();
    assert_eq!(requests.len(), 1, "exactly one exchange");
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_partial_options_document_is_offered_with_what_it_carries() -> Fallible {
    let sut = serving(ResponseTemplate::new(200).set_body_json(json!({ "solution": "cdr" })));
    let read = identify(&sut.base_url(), AuthKind::None)?;
    let options = read
        .options()
        .ok_or_else(|| format!("expected an offered document, got {read:?}"))?;
    assert_eq!(options.solution.as_deref(), Some("cdr"));
    assert_eq!(options.solution_version, None);
    assert_eq!(options.endpoints, None);
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_404_is_not_offered_and_is_no_error() -> Fallible {
    let sut = serving(ResponseTemplate::new(404));
    assert_eq!(
        identify(&sut.base_url(), AuthKind::None)?,
        IdentityRead::NotOffered { status: 404 }
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_2xx_that_is_not_json_is_not_an_options_document() -> Fallible {
    let sut = serving(
        ResponseTemplate::new(200)
            .set_body_string("<html>welcome</html>")
            .insert_header("content-type", "text/html"),
    );
    let read = identify(&sut.base_url(), AuthKind::None)?;
    assert!(
        matches!(read, IdentityRead::NotOptionsDocument { status: 200, .. }),
        "{read:?}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_wrong_typed_member_is_not_an_options_document() -> Fallible {
    let sut = serving(ResponseTemplate::new(200).set_body_json(json!({
        "solution": "cdr",
        "solution_version": 2
    })));
    let read = identify(&sut.base_url(), AuthKind::None)?;
    assert!(
        matches!(read, IdentityRead::NotOptionsDocument { status: 200, .. }),
        "{read:?}"
    );
    Ok(())
}

/// The binary prints the classified answer as one JSON document on stdout.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_identify_subcommand_prints_one_identity_document() -> Fallible {
    let sut = serving(ResponseTemplate::new(200).set_body_json(full_document()));
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_veredictum"))
        .args(["identify", "--base-url", &sut.base_url()])
        .output()?;
    assert!(output.status.success(), "{output:?}");
    let document: IdentityRead = serde_json::from_slice(&output.stdout)?;
    assert_eq!(
        document.options().and_then(|o| o.solution.as_deref()),
        Some("openEHRSys")
    );
    let raw: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(raw.get("outcome"), Some(&json!("offered")));
    Ok(())
}

/// Writes an ixit whose every instance is `base_url`, unauthenticated.
fn write_ixit(dir: &Path, base_url: &str) -> Result<PathBuf, std::io::Error> {
    let document = format!(
        r#"{{
  "instances": {{
    "sut": {{ "base_url": "{base_url}", "auth": {{ "mode": "none" }} }}
  }}
}}
"#
    );
    let path = dir.join("ixit.json");
    std::fs::write(&path, document)?;
    Ok(path)
}

/// One run against `sut` with the operator's flags, collecting every
/// warning line the run raised.
fn run_against(
    sut: &FakeSut,
    name: Option<&str>,
    version: Option<&str>,
    recording: Recording,
) -> Result<(Result<RunOutcome, veredictum::pipeline::Error>, Vec<String>), std::io::Error> {
    let dir = assert_fs::TempDir::new().map_err(std::io::Error::other)?;
    let ixit = write_ixit(dir.path(), &sut.base_url())?;
    let root = repo_root().join("artifacts");
    let warnings = Mutex::new(Vec::new());
    let outcome = execute_run(
        &RunRequest {
            root: &root,
            ixit: &ixit,
            out_dir: dir.path(),
            sut_name: name,
            sut_version: version,
            filter: Some(NO_CASE),
            statement: None,
            recording,
        },
        &|warning| {
            if let Ok(mut lines) = warnings.lock() {
                lines.extend(warning.lines());
            }
        },
        &mut |_| {},
    );
    let lines = warnings.into_inner().unwrap_or_default();
    Ok((outcome, lines))
}

/// The lines a run raised about identity, and nothing about selection.
fn identity_lines(lines: &[String]) -> Vec<&String> {
    lines
        .iter()
        .filter(|line| line.contains("System OPTIONS"))
        .collect()
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_run_records_the_servers_own_identity() -> Fallible {
    let sut = serving(ResponseTemplate::new(200).set_body_json(full_document()));
    let (outcome, lines) = run_against(&sut, None, None, Recording::Off)?;
    let sut_record = outcome?.results.sut;
    assert_eq!(sut_record.name, IdentityField::server("openEHRSys"));
    assert_eq!(sut_record.version, IdentityField::server("v1.0"));
    assert_eq!(sut_record.vendor, Some(IdentityField::server("My-openEHR")));
    assert_eq!(
        sut_record.restapi_specs_version,
        Some(IdentityField::server("1.1.0"))
    );
    assert!(identity_lines(&lines).is_empty(), "{lines:?}");
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_run_against_a_server_without_options_records_the_operators_identity() -> Fallible {
    let sut = serving(ResponseTemplate::new(404));
    let (outcome, lines) = run_against(&sut, Some("my-cdr"), Some("2.0"), Recording::Off)?;
    let sut_record = outcome?.results.sut;
    assert_eq!(sut_record.name, IdentityField::operator("my-cdr"));
    assert_eq!(sut_record.version, IdentityField::operator("2.0"));
    assert_eq!(sut_record.vendor, None);
    assert_eq!(sut_record.restapi_specs_version, None);
    assert!(identity_lines(&lines).is_empty(), "{lines:?}");
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn agreeing_flags_record_the_server_and_raise_no_notice() -> Fallible {
    let sut = serving(ResponseTemplate::new(200).set_body_json(full_document()));
    let (outcome, lines) = run_against(&sut, Some("openEHRSys"), Some("v1.0"), Recording::Off)?;
    let sut_record = outcome?.results.sut;
    assert_eq!(sut_record.name.source, IdentitySource::Server);
    assert_eq!(sut_record.version.source, IdentitySource::Server);
    assert!(identity_lines(&lines).is_empty(), "{lines:?}");
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_disagreeing_flag_loses_to_the_server_with_a_notice_naming_both() -> Fallible {
    let sut = serving(ResponseTemplate::new(200).set_body_json(full_document()));
    let (outcome, lines) = run_against(&sut, Some("typo-sys"), Some("v1.0"), Recording::Off)?;
    let sut_record = outcome?.results.sut;
    assert_eq!(sut_record.name, IdentityField::server("openEHRSys"));
    let notices = identity_lines(&lines);
    assert_eq!(notices.len(), 1, "{lines:?}");
    let notice = notices.first().ok_or("one notice")?;
    assert!(
        notice.contains("\"openEHRSys\"") && notice.contains("\"typo-sys\""),
        "{notice}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_run_with_no_identity_from_either_side_refuses_to_start() -> Fallible {
    let sut = serving(ResponseTemplate::new(404));
    let (outcome, _) = run_against(&sut, None, Some("2.0"), Recording::Off)?;
    let error = outcome.err().ok_or("the run must refuse")?;
    assert!(
        matches!(
            error,
            veredictum::pipeline::Error::Identity(veredictum::identity::IdentityError::Missing {
                field: "name",
                ..
            })
        ),
        "{error}"
    );
    assert!(error.to_string().contains("--sut-name"), "{error}");
    Ok(())
}

/// The identity exchange is recorded in the transcript when the run records
/// its exchanges, beside the cases rather than inside any of them.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_recorded_run_carries_the_identity_exchange() -> Fallible {
    let sut = serving(ResponseTemplate::new(200).set_body_json(full_document()));
    let (outcome, _) = run_against(&sut, None, None, Recording::On)?;
    let outcome = outcome?;
    let document = outcome
        .transcript_document()?
        .ok_or("a recorded run with an identity exchange writes a transcript")?;
    let transcript: veredictum::transcript::RunTranscript = serde_json::from_str(&document)?;
    let exchange = transcript.identity.ok_or("the identity exchange")?;
    assert_eq!(exchange.request.method, "OPTIONS");
    assert_eq!(exchange.response.status, 200);
    assert!(transcript.cases.is_empty());

    let (unrecorded, _) = run_against(&sut, None, None, Recording::Off)?;
    assert_eq!(unrecorded?.transcript_document()?, None);
    Ok(())
}
