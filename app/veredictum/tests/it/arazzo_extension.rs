// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! A party-declared extension source: `validate` holds every use of one to a
//! shared setup that judges nothing, and a run skips a case whose setups
//! need an extension the statement does not declare as
//! `extension-not-offered`, or sends the setup to the party's own route when
//! it does. No openEHR spec governs this; it is this instrument's own design.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use veredictum::arazzo::gates::{Gate, check_catalogue};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::fake_sut::FakeSut;

/// Anything a run or a fixture read can fail with.
type Fallible = Result<(), Box<dyn std::error::Error>>;

/// The EHR the party's import route places.
const IMPORTED: &str = "4b6a1a0e-35e4-4bb4-9d0d-6a1f5b1c2d3e";

/// The party's own OpenAPI document, which declares the import route.
const PARTY_OAS: &str = "openapi: 3.0.3\ninfo: { title: Party routes, version: '1' }\npaths:\n  /admin/import/{ehr_id}:\n    post:\n      operationId: import_ehr\n      parameters:\n        - { name: ehr_id, in: path, required: true, schema: { type: string } }\n      responses:\n        '201': { description: imported }\n";

/// One shared setup over an `ehr-import` extension source: `extension` is
/// the source's extension value, `step` the extension step's extra lines and
/// `criterion` the extra lines of its one criterion.
fn setup(extension: &str, operation: &str, step: &str, criterion: &str) -> String {
    format!(
        r"arazzo: 1.1.0
info: {{ title: Shared setups, version: 0.1.0 }}
sourceDescriptions:
  - name: import
    url: extension:ehr-import
    type: openapi
    x-veredictum-extension: {extension}
workflows:
  - workflowId: import_ehr
    summary: The EHR the case reads is imported
    inputs:
      type: object
      properties:
        ehr_id: {{ type: string }}
    steps:
      - stepId: import
        operationId: $sourceDescriptions.import.{operation}
        parameters:
          - {{ name: ehr_id, in: path, value: {IMPORTED} }}
{step}        successCriteria:
          - condition: $statusCode == 201
{criterion}"
    )
}

/// The clean setup: its extension step judges nothing.
fn clean_setup() -> String {
    setup("ehr-import", "ehr-import", "", "")
}

/// The case that reads the imported EHR after the setup runs.
const CASE: &str = r"arazzo: 1.1.0
info: { title: Imported reads, version: 0.1.0 }
sourceDescriptions:
  - { name: ehr, url: ../../../specs/rest-oas/ehr-validation.openapi.yaml, type: openapi }
  - { name: setup, url: ../_setup/import.arazzo.yaml, type: arazzo }
workflows:
  - workflowId: imported
    summary: The imported EHR is retrieved
    dependsOn: [$sourceDescriptions.setup.import_ehr]
    x-veredictum-cite:
      source: specs/openehr/ITS-REST/specifications/responses/200_EHR.yaml
      section: description
      quote: '`200 OK` is returned when the requested EHR resource is successfully retrieved.'
    steps:
      - stepId: get
        operationId: $sourceDescriptions.ehr.ehr_get_by_id
        parameters:
          - { name: ehr_id, in: path, value: IMPORTED_ID }
          - { name: Accept, in: header, value: application/json }
        successCriteria:
          - condition: $statusCode == 200
";

/// Writes `text` at `relative` under `root`, making its folders.
fn write(root: &Path, relative: &str, text: &str) -> std::io::Result<()> {
    let file = root.join(relative);
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(file, text)
}

/// A catalogue root under `dir`, beside a link to the vendored `specs/`,
/// holding the shared setup `setup` and the case reading what it imports.
#[cfg(unix)]
fn catalogue(dir: &Path, setup: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
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
    write(&root, "cases/_setup/import.arazzo.yaml", setup)?;
    write(
        &root,
        "cases/ehr/imported.arazzo.yaml",
        &CASE.replace("IMPORTED_ID", IMPORTED),
    )?;
    Ok(root)
}

/// Every `arazzo-extension` finding over `root`, gated against the vendored
/// spec tree, as `(location, message)`; any finding of another gate fails.
fn extension_findings(root: &Path, dir: &Path) -> Vec<(String, String)> {
    let findings = check_catalogue(root, Some(&dir.join("specs/openehr")));
    let (extension, other): (Vec<_>, Vec<_>) = findings
        .into_iter()
        .partition(|finding| finding.gate == Gate::Extension);
    assert!(other.is_empty(), "findings of another gate: {other:#?}");
    extension
        .into_iter()
        .map(|finding| (finding.location, finding.message))
        .collect()
}

#[test]
#[cfg(unix)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_setup_only_extension_use_passes_every_gate() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let root = catalogue(dir.path(), &clean_setup())?;
    assert_eq!(extension_findings(&root, dir.path()), Vec::new());
    Ok(())
}

#[test]
#[cfg(unix)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn each_refusal_fires_on_its_seeded_defect() -> Fallible {
    let cite = "x-veredictum-cite: { source: specs/openehr/ITS-REST/specifications/responses/200_EHR.yaml, section: description, quote: '`200 OK` is returned when the requested EHR resource is successfully retrieved.' }";
    let cases = [
        (
            setup("ehr-import", "import_ehr", "", ""),
            "by another name than its extension `ehr-import`",
        ),
        (
            setup("ehr-import", "ehr-import", &format!("        {cite}\n"), ""),
            "an extension step carries a `x-veredictum-cite`",
        ),
        (
            setup(
                "ehr-import",
                "ehr-import",
                "",
                &format!("            {cite}\n"),
            ),
            "a criterion of an extension step carries a `x-veredictum-cite`",
        ),
        (
            setup(
                "ehr-import",
                "ehr-import",
                "",
                "            x-veredictum-check: { judge: json_type, context: $response.body#/ehr_id, type: object }\n",
            ),
            "carries a `x-veredictum-check` judge",
        ),
    ];
    for (setup, refusal) in cases {
        let dir = assert_fs::TempDir::new()?;
        let root = catalogue(dir.path(), &setup)?;
        let found = extension_findings(&root, dir.path());
        assert_eq!(found.len(), 1, "{refusal}: {found:#?}");
        assert!(
            found.iter().any(|(_, message)| message.contains(refusal)),
            "{refusal}: {found:#?}"
        );
    }
    Ok(())
}

#[test]
#[cfg(unix)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_unusable_extension_source_is_refused() -> Fallible {
    let cases = [
        (
            setup("ehr-export", "ehr-import", "", ""),
            "`ehr-export`, which is not an extension the catalogue defines",
        ),
        (
            clean_setup().replace("type: openapi", "type: arazzo"),
            "an extension source stands for an OpenAPI document",
        ),
    ];
    for (setup, refusal) in cases {
        let dir = assert_fs::TempDir::new()?;
        let root = catalogue(dir.path(), &setup)?;
        let findings = check_catalogue(&root, Some(&dir.path().join("specs/openehr")));
        assert!(
            findings
                .iter()
                .any(|finding| finding.gate == Gate::Extension
                    && finding.location == "#/sourceDescriptions/import"
                    && finding.message.contains(refusal)),
            "{refusal}: {findings:#?}"
        );
    }
    Ok(())
}

#[test]
#[cfg(unix)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_extension_called_from_a_case_is_refused() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let root = catalogue(dir.path(), &clean_setup())?;
    let outside = clean_setup()
        .replace("../../../", "")
        .replace("import_ehr\n", "import_in_a_case\n");
    write(&root, "cases/ehr/importing.arazzo.yaml", &outside)?;
    let found = extension_findings(&root, dir.path());
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found
            .iter()
            .any(|(location, message)| location == "import_in_a_case/import"
                && message.contains("only a workflow of a shared setup")),
        "{found:#?}"
    );
    Ok(())
}

#[test]
#[cfg(unix)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_rows_file_feeding_an_extension_setup_is_refused() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let root = catalogue(dir.path(), &clean_setup())?;
    write(
        &root,
        "cases/_setup/import_ehr.rows.yaml",
        &format!(
            "workflow: import/import_ehr\niteration: reset_per_row\nrows:\n  - {{ ehr_id: {IMPORTED} }}\n"
        ),
    )?;
    let found = extension_findings(&root, dir.path());
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found
            .iter()
            .any(|(_, message)| message.contains("a rows file feeds a workflow")),
        "{found:#?}"
    );
    Ok(())
}

/// A statement claiming both folders, declaring the import route when
/// `declared`.
fn statement(dir: &Path, declared: bool) -> Result<String, Box<dyn std::error::Error>> {
    write(dir, "party/routes.yaml", PARTY_OAS)?;
    let extensions = if declared {
        json!({ "ehr-import": { "document": "party/routes.yaml", "operation_id": "import_ehr" } })
    } else {
        json!({})
    };
    let statement = json!({
        "product": { "name": "p", "version": "1", "vendor": "v", "identifier": "urn:p" },
        "schedule_release": "cnf-2.0-w2",
        "claims": { "capabilities": ["ehr", "system"] },
        "extensions": extensions
    });
    let file = dir.join(if declared {
        "declared.json"
    } else {
        "undeclared.json"
    });
    std::fs::write(&file, statement.to_string())?;
    Ok(file.to_string_lossy().into_owned())
}

/// One `veredictum test` of the case document against `sut`.
fn run(root: &Path, statement: &str, sut: &FakeSut, out: &Path) -> Result<Output, std::io::Error> {
    Command::new(env!("CARGO_BIN_EXE_veredictum"))
        .arg("test")
        .arg("--root")
        .arg(root)
        .args(["--document", "imported", "--statement", statement])
        .args(["--base-url", &sut.base_url()])
        .arg("--out")
        .arg(out)
        .env_remove("VEREDICTUM_CATALOGUE")
        .output()
}

/// The one row of the case, as `(status, skip reason, whose)`.
fn the_row(out: &Path) -> Result<(String, String, String), Box<dyn std::error::Error>> {
    let results: Value = serde_json::from_str(&std::fs::read_to_string(out.join("results.json"))?)?;
    let row = results
        .pointer("/rows/0")
        .cloned()
        .ok_or("the results hold no row")?;
    let text = |pointer: &str| {
        row.pointer(pointer)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    Ok((
        text("/status"),
        text("/skip/reason"),
        text("/explanation/whose"),
    ))
}

/// The fake server: the import route answers `import`, and the EHR reads
/// back.
fn sut(import: u16) -> FakeSut {
    let sut = FakeSut::start();
    sut.mount(
        Mock::given(method("POST"))
            .and(path(format!("/admin/import/{IMPORTED}")))
            .respond_with(ResponseTemplate::new(import)),
    );
    sut.mount(
        Mock::given(method("GET"))
            .and(path(format!("/ehr/{IMPORTED}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "ehr_id": { "value": IMPORTED } })),
            ),
    );
    sut
}

#[test]
#[cfg(unix)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn an_undeclared_extension_skips_the_case_and_a_declared_one_runs_its_setup() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let root = catalogue(dir.path(), &clean_setup())?;

    let server = sut(201);
    let out = dir.path().join("undeclared");
    let output = run(&root, &statement(dir.path(), false)?, &server, &out)?;
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let (status, reason, _) = the_row(&out)?;
    assert_eq!(
        (status.as_str(), reason.as_str()),
        ("skipped", "extension-not-offered")
    );
    let (sent, _) = server.catalogue_requests();
    assert!(sent.is_empty(), "a skipped case sent {sent:?}");

    let server = sut(201);
    let out = dir.path().join("declared");
    let output = run(&root, &statement(dir.path(), true)?, &server, &out)?;
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let (status, reason, _) = the_row(&out)?;
    assert_eq!((status.as_str(), reason.as_str()), ("passed", ""));
    let (sent, _) = server.catalogue_requests();
    let sent: Vec<String> = sent
        .iter()
        .map(|request| format!("{} {}", request.method, request.url.path()))
        .collect();
    assert_eq!(
        sent,
        [
            format!("POST /admin/import/{IMPORTED}"),
            format!("GET /ehr/{IMPORTED}")
        ]
    );

    let again = dir.path().join("rejudged");
    let rejudged = Command::new(env!("CARGO_BIN_EXE_veredictum"))
        .arg("test")
        .arg("--root")
        .arg(&root)
        .args(["--document", "imported", "--statement"])
        .arg(dir.path().join("declared.json"))
        .arg("--rejudge")
        .arg(out.join("recording.har"))
        .arg("--out")
        .arg(&again)
        .env_remove("VEREDICTUM_CATALOGUE")
        .output()?;
    assert_eq!(rejudged.status.code(), Some(0), "{rejudged:?}");
    assert_eq!(
        std::fs::read(again.join("results.json"))?,
        std::fs::read(out.join("results.json"))?
    );
    Ok(())
}

#[test]
#[cfg(unix)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book's Result-test shape: assertions panic, plumbing propagates with `?`"
)]
fn a_failed_extension_setup_errors_the_case_on_the_instrument() -> Fallible {
    let dir = assert_fs::TempDir::new()?;
    let root = catalogue(dir.path(), &clean_setup())?;
    let server = sut(500);
    let out = dir.path().join("failed");
    let output = run(&root, &statement(dir.path(), true)?, &server, &out)?;
    let (status, _, whose) = the_row(&out)?;
    assert_eq!(
        (status.as_str(), whose.as_str()),
        ("errored", "instrument"),
        "{output:?}"
    );
    Ok(())
}
