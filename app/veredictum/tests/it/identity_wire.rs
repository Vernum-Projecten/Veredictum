// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The server's identity read through ITS-REST System OPTIONS
//! (`specifications/operations/options.yaml`, "Options and Conformance"),
//! against the fake SUT: the `identify` seam and its binary over every answer
//! shape.

use serde_json::json;
use veredictum::arazzo::credential::AuthKind;
use veredictum::identity::{IdentityRead, identify};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::fake_sut::FakeSut;

/// Anything a seam or a fixture read can fail with.
type Fallible = Result<(), Box<dyn std::error::Error>>;

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

/// Every variable `identify` reads a credential from, removed so the
/// developer's or the runner's own environment never reaches a test.
const CREDENTIAL_VARS: [&str; 6] = [
    "VEREDICTUM_CDR_USER",
    "VEREDICTUM_CDR_USER_FILE",
    "VEREDICTUM_CDR_PASSWORD",
    "VEREDICTUM_CDR_PASSWORD_FILE",
    "VEREDICTUM_CDR_TOKEN",
    "VEREDICTUM_CDR_TOKEN_FILE",
];

/// One `identify` invocation against `base_url`, with only `envs` set.
fn identify_command(
    base_url: &str,
    auth: &str,
    envs: &[(&str, &str)],
) -> Result<std::process::Output, std::io::Error> {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_veredictum"));
    command.args(["identify", "--base-url", base_url, "--auth", auth]);
    for name in CREDENTIAL_VARS {
        command.env_remove(name);
    }
    command.envs(envs.iter().copied());
    command.output()
}

/// A missing credential stops `identify` before any request, and the error
/// names the mode and the variables it reads.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_missing_credential_names_the_mode_and_its_variables() -> Fallible {
    let sut = serving(ResponseTemplate::new(200).set_body_json(full_document()));
    let cases = [
        (
            "basic",
            "--auth basic needs VEREDICTUM_CDR_USER and VEREDICTUM_CDR_PASSWORD (or their _FILE \
             forms)",
        ),
        (
            "bearer",
            "--auth bearer needs VEREDICTUM_CDR_TOKEN (or its _FILE form)",
        ),
    ];
    for (auth, named) in cases {
        let output = identify_command(&sut.base_url(), auth, &[])?;
        assert_eq!(output.status.code(), Some(2), "{auth}: {output:?}");
        let stderr = String::from_utf8(output.stderr)?;
        assert!(stderr.contains(named), "{auth}: {stderr}");
    }
    assert!(
        sut.requests().is_empty(),
        "a refused identify sent a request"
    );
    Ok(())
}

/// The token is read from the file `VEREDICTUM_CDR_TOKEN_FILE` names and
/// presented as a bearer credential, and it never reaches the output.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_bearer_token_is_read_from_its_file_form() -> Fallible {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("OPTIONS"))
            .and(path("/"))
            .and(header("authorization", "Bearer t0ken-from-file"))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_document())),
    );
    let dir = assert_fs::TempDir::new()?;
    let token = dir.path().join("token");
    std::fs::write(&token, "t0ken-from-file\n")?;
    let token = token.to_string_lossy().into_owned();
    let output = identify_command(
        &sut.base_url(),
        "bearer",
        &[("VEREDICTUM_CDR_TOKEN_FILE", &token)],
    )?;
    assert!(output.status.success(), "{output:?}");
    let raw: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(raw.get("outcome"), Some(&json!("offered")));
    let printed = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!printed.contains("t0ken-from-file"), "{printed}");
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

/// Every answer shape `identify` prints validates against the published
/// `identity.schema.json`, the committed file a consumer reads.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn every_printed_identity_document_validates_against_its_schema() -> Fallible {
    let committed = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .join("schemas/identity.schema.json");
    let schema: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(committed)?)?;
    let validator = jsonschema::validator_for(&schema)?;
    let answers = [
        (
            "offered",
            ResponseTemplate::new(200).set_body_json(full_document()),
        ),
        (
            "offered",
            ResponseTemplate::new(200).set_body_json(json!({ "solution": "cdr" })),
        ),
        ("not_offered", ResponseTemplate::new(404)),
        (
            "not_options_document",
            ResponseTemplate::new(200).set_body_json(json!({ "solution_version": 2 })),
        ),
        ("not_options_document", ResponseTemplate::new(204)),
        ("invalid_status", ResponseTemplate::new(600)),
    ];
    for (outcome, answer) in answers {
        let sut = serving(answer);
        let output = identify_command(&sut.base_url(), "none", &[])?;
        assert!(output.status.success(), "{output:?}");
        let printed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(printed.get("outcome"), Some(&json!(outcome)), "{printed}");
        let findings: Vec<String> = validator
            .iter_errors(&printed)
            .map(|e| format!("{}: {e}", e.instance_path()))
            .collect();
        assert!(findings.is_empty(), "{printed}: {findings:?}");
    }
    let invalid = json!({ "outcome": "offered", "status": 404, "options": {} });
    assert!(
        !validator.is_valid(&invalid),
        "the schema refuses an offered document outside 200"
    );
    let refused = json!({ "outcome": "not_offered", "status": 600 });
    assert!(
        !validator.is_valid(&refused),
        "the schema refuses a not-offered answer outside 100..599"
    );
    Ok(())
}
