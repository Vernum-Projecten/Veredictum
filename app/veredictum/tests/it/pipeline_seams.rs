// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The consumable pipeline seams (`veredictum::pipeline`): serving the
//! published schema set, reading and writing the party documents, the
//! statement schema's refusals, and the typed failures each seam reports.
//!
//! Every input here is a file already committed to this repository or a
//! temporary one, so the seams are exercised the way a second consumer of the
//! library would drive them, with no system under test involved.

use std::path::{Path, PathBuf};

use veredictum::pipeline::{Error, assets, read_json, to_json_document};

/// Anything a seam or a fixture read can fail with, so a test body
/// propagates plumbing failures with `?` instead of unwrapping them
/// (<https://doc.rust-lang.org/book/ch11-01-writing-tests.html>).
type Fallible = Result<(), Box<dyn std::error::Error>>;

/// The repository root: the crate sits two levels under it.
fn repo_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

// ── the asset seams ────────────────────────────────────────────────────────

/// The schema seam serves exactly the published set: the same names and the
/// same bytes as the committed `schemas/` directory a party validates against.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_schema_seam_serves_the_committed_published_set() -> Fallible {
    let dir = repo_root().join("schemas");
    let files = assets::schema_files()?;
    assert!(!files.is_empty());
    for file in &files {
        let committed = std::fs::read_to_string(dir.join(&file.name))?;
        assert_eq!(
            committed, file.body,
            "{} differs from the committed schema",
            file.name
        );
    }
    Ok(())
}

// ── the shared readers ─────────────────────────────────────────────────────

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_json_reader_names_what_it_was_reading() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let absent = dir.path().join("absent.json");
    let error = read_json::<serde_json::Value>(&absent, "topology")
        .expect_err("an absent file cannot be read");
    match &error {
        Error::Read { path, .. } => assert_eq!(path, &absent),
        other => panic!("expected a read failure, got {other}"),
    }
    assert!(error.to_string().starts_with("cannot read "), "{error}");

    let broken = dir.path().join("broken.json");
    std::fs::write(&broken, "[")?;
    let error = read_json::<serde_json::Value>(&broken, "topology")
        .expect_err("a truncated document does not parse");
    assert!(matches!(error, Error::Parse { .. }), "{error}");
    assert!(error.to_string().starts_with("topology: "), "{error}");
    Ok(())
}

/// Every artifact this instrument writes is pretty JSON with a trailing
/// newline, and the parent directory is created before the write.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn documents_are_pretty_json_written_under_a_created_parent() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let path = dir.path().join("deep/nested/out.json");
    let body = to_json_document(&serde_json::json!({ "a": [1, 2] }), "test")?;
    assert_eq!(body, "{\n  \"a\": [\n    1,\n    2\n  ]\n}\n");

    veredictum::pipeline::ensure_parent_dir(&path)?;
    veredictum::pipeline::write_file(&path, &body)?;
    assert_eq!(std::fs::read_to_string(&path)?, body);

    // A path with no parent to create is not a failure, whether the parent is
    // the working directory or the path names nothing at all.
    veredictum::pipeline::ensure_parent_dir(Path::new("bare.json"))?;
    veredictum::pipeline::ensure_parent_dir(Path::new(""))?;

    let error = veredictum::pipeline::write_file(&dir.path().join("no/such/dir.json"), &body)
        .expect_err("writing into an absent directory fails");
    assert!(matches!(error, Error::Write { .. }), "{error}");
    Ok(())
}

/// File references inside an ixit document resolve against the DOCUMENT, not
/// the working directory, and the raw text travels with it because the
/// campaign's digest is taken over exactly those bytes.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_ixit_reader_rebases_file_references_and_keeps_the_bytes() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let path = dir.path().join("topology/ixit.json");
    std::fs::create_dir_all(path.parent().ok_or("the fixture path has a parent")?)?;
    let text = serde_json::to_string_pretty(&serde_json::json!({
        "instances": {
            "sut": { "base_url": "http://127.0.0.1:8080/openehr/v1", "auth": { "mode": "none" } },
            "smart_platform": { "base_url": "http://127.0.0.1:8080", "auth": { "mode": "none" } }
        },
        "smart": {
            "platform_instance": "smart_platform",
            "mint": {
                "issuer": "https://as.example.test",
                "subject": "cnf-smart-app",
                "key_file": "keys/test.key.pem",
                "kid": "k1",
                "ttl_seconds": 300
            }
        }
    }))?;
    std::fs::write(&path, &text)?;

    let (ixit, kept) = veredictum::pipeline::load_ixit(&path)?;
    assert_eq!(kept, text, "the digest is taken over the file's own bytes");
    let key = ixit
        .smart
        .as_ref()
        .ok_or("the fixture declares a SMART lane")?
        .mint
        .key_file
        .clone();
    assert_eq!(
        key,
        dir.path().join("topology/keys/test.key.pem"),
        "the key reference is rebased onto the document's directory, not the working directory"
    );

    // A document that is not a topology is a named parse failure.
    let broken = dir.path().join("not-ixit.json");
    std::fs::write(&broken, r#"{"instances": 7}"#)?;
    let error = veredictum::pipeline::load_ixit(&broken).expect_err("7 is not an instance map");
    assert!(matches!(error, Error::Parse { .. }), "{error}");
    assert!(error.to_string().starts_with("ixit: "), "{error}");
    Ok(())
}

// ── the statement schema ───────────────────────────────────────────────────

/// The statement carries no performance block: a declaration naming one is
/// refused by the published schema, and the refusal names the field.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_statement_carrying_a_performance_block_is_refused_naming_the_field() -> Fallible {
    let text = std::fs::read_to_string(repo_root().join("fixtures/declaration/statement.json"))?;
    let mut statement: serde_json::Value = serde_json::from_str(&text)?;
    let validator = jsonschema::validator_for(&veredictum::schema::statement_schema())?;
    assert!(
        validator.is_valid(&statement),
        "the fixture itself is valid"
    );
    statement["performance"] =
        serde_json::json!({ "class": "POC", "environment_ref": "ixit.json#/environment" });
    let errors: Vec<String> = validator
        .iter_errors(&statement)
        .map(|e| e.to_string())
        .collect();
    assert!(!errors.is_empty(), "a performance block is refused");
    assert!(
        errors.iter().any(|e| e.contains("performance")),
        "the refusal names the field: {errors:?}"
    );
    Ok(())
}

// ── the seams' typed refusals ──────────────────────────────────────────────

/// A value that cannot be serialized is a typed failure naming what was being
/// written, and a directory that cannot be created is named too — neither
/// silently produces an empty artifact.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_unwritable_document_names_what_it_was_writing() -> Fallible {
    use std::collections::BTreeMap;

    // A map keyed by anything but a string is not a JSON object.
    let unserializable: BTreeMap<(u8, u8), u8> = BTreeMap::from([((1, 2), 3)]);
    let error = to_json_document(&unserializable, "the verdict record")
        .expect_err("a non-string map key is not JSON");
    assert!(matches!(error, Error::Serialize { .. }), "{error}");
    assert!(
        error.to_string().starts_with("the verdict record:"),
        "{error}"
    );

    // A parent directory that cannot exist, because a FILE occupies its path.
    let dir = assert_fs::TempDir::new()?;
    let occupied = dir.path().join("occupied");
    std::fs::write(&occupied, "not a directory")?;
    let error = veredictum::pipeline::ensure_parent_dir(&occupied.join("under/out.json"))
        .expect_err("a file cannot hold a directory");
    let Error::CreateDir { path, .. } = &error else {
        panic!("expected the directory failure, got {error}");
    };
    assert!(path.ends_with("under"), "{}", path.display());
    Ok(())
}

/// An ixit path that cannot be read at all is a typed READ failure naming the
/// file, kept apart from a document that reads but does not parse.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_unreadable_ixit_is_a_read_failure_not_a_parse_failure() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let absent = dir.path().join("absent-ixit.json");
    let error = veredictum::pipeline::load_ixit(&absent).expect_err("an absent ixit is unreadable");
    let Error::Read { path, .. } = &error else {
        panic!("expected the read failure, got {error}");
    };
    assert_eq!(path, &absent);
    Ok(())
}

// ── the replay seam's selection posture ────────────────────────────────────
