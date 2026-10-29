// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! `veredictum verify` over a certificate the lib issued with the committed
//! test key: the exit status carries the verdict on the certificate, and a
//! file it cannot read or a key set it refuses is a runner error.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use veredictum::certificate::{
    CERTIFICATE_FILE, CertificatePayload, Kid, RecordedRun, SigningKey, issue,
};
use veredictum::party::{IdentityField, Sut};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn repo() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).to_path_buf()
}

fn verify(certificate: &Path, jwks: &Path) -> Result<Output, std::io::Error> {
    Command::new(env!("CARGO_BIN_EXE_veredictum"))
        .arg("verify")
        .arg(certificate)
        .arg("--jwks")
        .arg(jwks)
        .output()
}

/// A certificate over a results document with one passed row, issued with
/// the committed test key, written into `dir`.
fn issued(dir: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let results = format!(
        r#"{{ "document": "catalogue", "engine": {{ "name": "veredictum", "version": "{}" }}, "instrument_status": "{}", "base_url": "http://sut.example/openehr/v1", "rows": [ {{ "case": "ehr/a", "row": 0, "status": "passed", "inputs": {{}}, "generated": [] }} ] }}"#,
        env!("CARGO_PKG_VERSION"),
        veredictum::party::InstrumentStatus::CURRENT.token()
    );
    let payload = CertificatePayload::of_run(&RecordedRun {
        results: results.as_bytes(),
        recording: b"{}",
        sut: Sut {
            name: IdentityField::server("ExampleCDR"),
            version: IdentityField::server("2.1.0"),
            vendor: None,
            restapi_specs_version: None,
        },
        catalogue: &repo().join("fixtures/declaration"),
        issued_at: "2026-10-29T12:00:00Z".parse()?,
    })?;
    let key = SigningKey::from_pkcs8_pem(
        Kid::parse("veredictum-test")?,
        &std::fs::read(repo().join("fixtures/certificates/test-signing.key.pem"))?,
    )?;
    let path = dir.join(CERTIFICATE_FILE);
    std::fs::write(&path, issue(&payload, &key)?.to_text())?;
    Ok(path)
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
fn a_valid_certificate_verifies_and_prints_what_it_states() -> TestResult {
    let dir = assert_fs::TempDir::new()?;
    let out = verify(
        &issued(dir.path())?,
        &repo().join("fixtures/certificates/jwks.json"),
    )?;
    let stdout = String::from_utf8(out.stdout)?;
    assert!(out.status.success(), "{stdout}");
    assert!(
        stdout.contains("verified with key veredictum-test"),
        "{stdout}"
    );
    assert!(stdout.contains("ExampleCDR (server)"), "{stdout}");
    assert!(
        stdout.contains("green: 1 passed, 0 failed, 0 advisory, 0 errored, 0 skipped"),
        "{stdout}"
    );
    assert!(stdout.contains(veredictum::certificate::HONESTY_LINE));
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
fn a_modified_certificate_is_refused_with_status_one() -> TestResult {
    let dir = assert_fs::TempDir::new()?;
    let path = issued(dir.path())?;
    let text = std::fs::read_to_string(&path)?;
    let mut parts: Vec<String> = text.trim().split('.').map(str::to_owned).collect();
    let payload = String::from_utf8(URL_SAFE_NO_PAD.decode(parts.get(1).ok_or("a payload")?)?)?
        .replace("ExampleCDR", "OtherCDR");
    if let Some(segment) = parts.get_mut(1) {
        *segment = URL_SAFE_NO_PAD.encode(payload);
    }
    std::fs::write(&path, parts.join("."))?;
    let out = verify(&path, &repo().join("fixtures/certificates/jwks.json"))?;
    let stderr = String::from_utf8(out.stderr)?;
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("certificate REFUSED"), "{stderr}");
    assert!(
        out.stdout.is_empty(),
        "a refused certificate prints no payload"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
fn the_published_set_holds_no_key_yet_so_a_test_certificate_is_refused_by_it() -> TestResult {
    let dir = assert_fs::TempDir::new()?;
    let out = verify(
        &issued(dir.path())?,
        &repo().join("deploy/hosted/jwks.json"),
    )?;
    let stderr = String::from_utf8(out.stderr)?;
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("holds no key"), "{stderr}");
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ?"
)]
fn an_unreadable_file_or_a_refused_key_set_is_a_runner_error() -> TestResult {
    let dir = assert_fs::TempDir::new()?;
    let certificate = issued(dir.path())?;
    let missing = verify(&dir.path().join("absent.jws"), &certificate)?;
    assert_eq!(missing.status.code(), Some(2));
    let private = dir.path().join("private.json");
    std::fs::write(
        &private,
        std::fs::read_to_string(repo().join("fixtures/certificates/jwks.json"))?
            .replace(r#""use": "sig""#, r#""use": "sig", "d": "AAAA""#),
    )?;
    let refused = verify(&certificate, &private)?;
    let stderr = String::from_utf8(refused.stderr)?;
    assert_eq!(refused.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("private half"), "{stderr}");
    Ok(())
}
