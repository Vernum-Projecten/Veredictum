// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The server's identity read through ITS-REST System OPTIONS
//! (`specifications/operations/options.yaml`, "Options and Conformance"),
//! against the fake SUT: the `identify` seam and its binary over every answer
//! shape.

use serde_json::json;
use veredictum::bench::client::AuthKind;
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
