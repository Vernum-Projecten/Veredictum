// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The GitLab CI template (`ci/gitlab/veredictum.yml`) against GitLab's own
//! CI/CD configuration schema, vendored under `ci/gitlab/schema/`.
//!
//! GitLab interpolates each `$[[ inputs.<name> ]]` before it reads the jobs
//! (<https://docs.gitlab.com/ci/inputs/>), so the test substitutes the inputs
//! the same way, once with every default and once with every input set, and
//! validates the header and the jobs document. A template that GitLab's
//! pipeline editor would refuse fails here, and an interpolation naming an
//! undeclared input is left unsubstituted and fails too.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

type Fallible = Result<(), Box<dyn std::error::Error>>;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn template() -> Result<String, std::io::Error> {
    std::fs::read_to_string(root().join("ci/gitlab/veredictum.yml"))
}

fn validator() -> Result<jsonschema::Validator, Box<dyn std::error::Error>> {
    let schema: Value = serde_json::from_str(&std::fs::read_to_string(
        root().join("ci/gitlab/schema/ci.json"),
    )?)?;
    Ok(jsonschema::validator_for(&schema)?)
}

/// The declared inputs and their defaults, rendered as GitLab renders them.
fn declared(header: &Value) -> BTreeMap<String, Option<String>> {
    header
        .pointer("/spec/inputs")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(name, spec)| {
            let default = spec.get("default").map(|d| match d {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            });
            (name.clone(), default)
        })
        .collect()
}

/// The jobs text with every declared input substituted.
fn interpolate(text: &str, values: &BTreeMap<String, String>) -> String {
    let mut out = text.to_owned();
    for (name, value) in values {
        out = out.replace(&format!("$[[ inputs.{name} ]]"), value);
    }
    out
}

fn violations(validator: &jsonschema::Validator, document: &Value) -> Vec<String> {
    validator
        .iter_errors(document)
        .map(|e| format!("{}: {e}", e.instance_path()))
        .collect()
}

fn instantiate(overrides: &[(&str, &str)]) -> Result<(Value, Value), Box<dyn std::error::Error>> {
    let text = template()?;
    let (head, jobs) = text
        .split_once("\n---\n")
        .ok_or("the template has no `---` between its header and its jobs")?;
    let header: Value = serde_saphyr::from_str(head)?;
    let mut values: BTreeMap<String, String> = BTreeMap::new();
    for (name, default) in declared(&header) {
        values.insert(name, default.unwrap_or_default());
    }
    values.insert(
        "base-url".to_owned(),
        "https://cdr.example/openehr/v1".to_owned(),
    );
    for (name, value) in overrides {
        if !values.contains_key(*name) {
            return Err(format!("{name} is not a declared input").into());
        }
        values.insert((*name).to_owned(), (*value).to_owned());
    }
    let jobs = interpolate(jobs, &values);
    if jobs.contains("$[[") {
        return Err(format!("an interpolation names an undeclared input:\n{jobs}").into());
    }
    Ok((header, serde_saphyr::from_str(&jobs)?))
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_template_with_its_defaults_is_a_valid_gitlab_ci_configuration() -> Fallible {
    let validator = validator()?;
    let (header, jobs) = instantiate(&[])?;
    assert_eq!(violations(&validator, &header), Vec::<String>::new());
    assert_eq!(violations(&validator, &jobs), Vec::<String>::new());
    let names: Vec<&String> = jobs
        .as_object()
        .map(|o| o.keys().collect())
        .unwrap_or_default();
    assert_eq!(names, ["veredictum", "veredictum-verdict"]);
    assert_eq!(
        jobs.pointer("/veredictum/artifacts/reports/junit"),
        Some(&Value::String("veredictum-results/junit.xml".to_owned()))
    );
    assert_eq!(
        jobs.pointer("/veredictum-verdict/allow_failure"),
        Some(&Value::Bool(false))
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_template_with_every_input_set_is_a_valid_gitlab_ci_configuration() -> Fallible {
    let validator = validator()?;
    let (_, jobs) = instantiate(&[
        ("statement", "conformance/statement.json"),
        ("ixit", "conformance/ixit.json"),
        ("auth", "bearer"),
        ("admin-auth", "basic"),
        ("documents", "ehr query"),
        ("profile", "released"),
        ("platform-url", "https://platform.example"),
        ("out-dir", "conformance-results"),
        ("keep-recording", "true"),
        ("fail-on-inconclusive", "false"),
        ("allow-failure", "true"),
        ("stage", "conformance"),
        ("job-name", "openehr"),
    ])?;
    assert_eq!(violations(&validator, &jobs), Vec::<String>::new());
    assert_eq!(
        jobs.pointer("/openehr-verdict/needs/0/job"),
        Some(&Value::String("openehr".to_owned()))
    );
    assert_eq!(
        jobs.pointer("/openehr-verdict/allow_failure"),
        Some(&Value::Bool(true))
    );
    assert_eq!(
        jobs.pointer("/openehr/stage"),
        Some(&Value::String("conformance".to_owned()))
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn the_schema_refuses_a_job_gitlab_would_refuse() -> Fallible {
    let validator = validator()?;
    let (_, mut jobs) = instantiate(&[])?;
    if let Some(job) = jobs.get_mut("veredictum").and_then(Value::as_object_mut) {
        job.insert(
            "allow_failure".to_owned(),
            Value::String("sometimes".to_owned()),
        );
    }
    assert!(
        !violations(&validator, &jobs).is_empty(),
        "a string allow_failure passed the vendored schema"
    );
    Ok(())
}
