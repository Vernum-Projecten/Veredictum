// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The Scope screen's acceptance gates: the preview equals what the engine
//! then actually processes (#65), the client-safe draft view carries no
//! secret, a pasted claim is schema-validated before it is stored (#101), and
//! a capability-composed claim selects exactly the folders the operator
//! checked (#100).

use veredictum_console::engine::{Credential, Engine, Secret};
use veredictum_console::run_api::{
    AuthChoice, CredentialForm, DeclaredPostures, DigestEncoding, PostureForm, RunDraft,
    SigningChoice, SigningPosture, SpecProfileChoice,
};

use veredictum_console::error::ConsoleError;

use crate::engine_gate;

/// The document filter the equality gate drives: one small document, so the
/// run is seconds.
const SCOPE_FILTER: &str = engine_gate::GATE_DOCUMENT;

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_scope_preview_counts_what_the_engine_processes() -> Result<(), Box<dyn std::error::Error>> {
    let binary = engine_gate::gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first",
            binary.display()
        );
        return Ok(());
    }
    let engine = Engine::verified(&binary)?;

    let state = crate::support::state(
        &crate::support::repo_root().join("out"),
        crate::support::committed_listing(),
    );
    let preview = veredictum_console::run_api::read::scope_preview(&state, SCOPE_FILTER)
        .map_err(|e| format!("preview: {e}"))?;
    let documents = veredictum_console::run_api::read::filtered_documents(&state, SCOPE_FILTER)
        .map_err(|e| format!("documents: {e}"))?;

    // The engine drives the same scope against the deterministic fixture SUT;
    // every workflow in scope lands as at least one row.
    let scratch = assert_fs::TempDir::new()?;
    let port = engine_gate::fixture_sut()?;
    let ixit = engine_gate::write_ixit(scratch.path(), port)?;
    let out = scratch.path().join("scope-run");
    std::fs::create_dir_all(&out)?;
    let spec = veredictum_console::engine::RunSpec {
        profile: veredictum::arazzo::spec_profile::SpecProfile::Development,
        documents,
        ..engine_gate::gate_spec(ixit, port, out)
    };
    let finished = engine.run(&spec, |_line| {})?;
    let processed: std::collections::BTreeSet<&str> = finished
        .results
        .rows
        .iter()
        .map(|row| row.case.as_str())
        .collect();
    assert_eq!(
        u64::try_from(processed.len())?,
        preview.total,
        "the preview promised a scope the engine did not process"
    );
    assert!(preview.total > 0, "the gate filter selected nothing at all");
    Ok(())
}

/// A filter no document id contains is refused before a run is built, so a
/// run never drives nothing while the screen says it drove a scope.
#[test]
fn a_filter_that_selects_no_document_is_refused() {
    let state = drafted_state();
    let refused = veredictum_console::run_api::read::filtered_documents(&state, "no-such-document");
    assert!(
        refused.is_err_and(|e| matches!(e, ConsoleError::Invalid { .. })
            && e.to_string().contains("no-such-document")),
        "an empty selection is refused by name"
    );
    assert_eq!(
        veredictum_console::run_api::read::filtered_documents(&state, "").ok(),
        Some(Vec::new()),
        "an empty filter runs every document"
    );
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_draft_view_carries_no_secret() -> Result<(), Box<dyn std::error::Error>> {
    let state = crate::support::state(
        &crate::support::repo_root().join("out"),
        crate::support::absent("unused"),
    );
    veredictum_console::run_api::read::save_connection(
        &state,
        engine_gate::gate_submitter(),
        RunDraft {
            profile: veredictum::arazzo::spec_profile::SpecProfile::Development,
            base_url: String::from("http://cdr.example"),
            sut_name: String::from("x"),
            sut_version: String::from("y"),
            identity: None,
            auth: AuthChoice::Basic,
            admin_auth: AuthChoice::None,
            credentials: vec![Credential {
                name: String::from(veredictum::identity::PASSWORD_ENV),
                value: Secret::new(String::from("hunter2-super-secret")),
            }],
            probed_ok: true,
            probe: None,
            statement_json: None,
            statement_product: None,
            filter: None,
            postures: DeclaredPostures::default(),
        },
    )
    .map_err(|e| format!("save: {e}"))?;
    let view = veredictum_console::run_api::read::draft_view(&state, engine_gate::gate_submitter())
        .ok_or("the draft must read back")?;
    let serialized = serde_json::to_string(&view)?;
    assert!(
        !serialized.contains("hunter2"),
        "a secret value reached the client-safe view: {serialized}"
    );
    // The debug rendering of the whole draft redacts too.
    let debugged = format!("{:?}", state.draft.lock().map_err(|e| e.to_string())?);
    assert!(
        !debugged.contains("hunter2"),
        "a secret value reached a Debug rendering: {debugged}"
    );
    Ok(())
}

/// A credential the connect form posts never prints its secret values.
#[test]
fn a_credential_form_debug_rendering_carries_no_secret() {
    let basic = CredentialForm {
        auth: AuthChoice::Basic,
        user: String::from("operator-user"),
        password: String::from("hunter2"),
        token: String::new(),
    };
    let bearer = CredentialForm {
        auth: AuthChoice::Bearer,
        user: String::new(),
        password: String::new(),
        token: String::from("bearer-secret"),
    };
    for (form, secrets) in [
        (basic, ["operator-user", "hunter2"]),
        (bearer, ["bearer-secret", "bearer-secret"]),
    ] {
        let debugged = format!("{form:?}");
        for secret in secrets {
            assert!(
                !debugged.contains(secret),
                "a credential value reached a Debug rendering: {debugged}"
            );
        }
    }
}

/// A fresh state over the repository mounts with a connected draft, for the
/// claim gates below.
fn drafted_state() -> veredictum_console::state::ConsoleState {
    veredictum_console::state::ConsoleState {
        draft: std::sync::Arc::new(std::sync::Mutex::new(engine_gate::drafts_of(RunDraft {
            profile: veredictum::arazzo::spec_profile::SpecProfile::Development,
            base_url: String::from("http://unused"),
            sut_name: String::from("claim-gate"),
            sut_version: String::from("0.0.0-gate"),
            identity: None,
            auth: AuthChoice::None,
            admin_auth: AuthChoice::None,
            credentials: vec![],
            probed_ok: true,
            probe: None,
            statement_json: None,
            statement_product: None,
            filter: None,
            postures: DeclaredPostures::default(),
        }))),
        ..crate::support::state(
            &crate::support::repo_root().join("out"),
            crate::support::committed_listing(),
        )
    }
}

/// #101: a pasted claim is held to the PUBLISHED statement schema — the named
/// ICS fixture passes with its summary, a shape serde would tolerate but the
/// schema forbids is refused, and non-JSON is refused.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_pasted_claim_is_schema_validated_before_it_is_stored() -> Result<(), Box<dyn std::error::Error>>
{
    let state = drafted_state();
    let body = std::fs::read_to_string(engine_gate::declaration_fixture())?;
    let summary = veredictum_console::run_api::read::save_scope(
        &state,
        engine_gate::gate_submitter(),
        Some(body),
        None,
        &PostureForm::default(),
    )
    .map_err(|e| format!("the ICS fixture must pass: {e}"))?
    .ok_or("a pasted claim must yield a summary")?;
    assert_eq!(summary.product, "Fixture CDR 0.0.0-fixture");
    assert!(
        summary.capabilities > 0,
        "the ICS fixture claims at least one capability"
    );

    // additionalProperties: false — serde would ignore the stray key, the
    // published schema refuses it.
    let stray = veredictum_console::run_api::read::save_scope(
        &state,
        engine_gate::gate_submitter(),
        Some(String::from(
            r#"{"product":{"name":"x","version":"1","vendor":"v","identifier":"urn:x"},"schedule_release":"cnf-2.0-w2","claims":{},"stray_key":true}"#,
        )),
        None,
        &PostureForm::default(),
    );
    assert!(
        stray.is_err(),
        "an undeclared key must be refused by the schema"
    );

    let not_json = veredictum_console::run_api::read::save_scope(
        &state,
        engine_gate::gate_submitter(),
        Some(String::from("not json")),
        None,
        &PostureForm::default(),
    );
    assert!(not_json.is_err(), "non-JSON must be refused");

    // The honest no-claim run stays legal: nothing pasted, no summary.
    let none = veredictum_console::run_api::read::save_scope(
        &state,
        engine_gate::gate_submitter(),
        None,
        None,
        &PostureForm::default(),
    )
    .map_err(|e| format!("no-claim save: {e}"))?;
    assert_eq!(none, None);
    Ok(())
}

/// #100: the capability rows ARE the listing's own folders, each counted
/// over the documents it holds.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_capability_rows_are_the_listings_own_folders() -> Result<(), Box<dyn std::error::Error>> {
    let state = drafted_state();
    let listing = state.catalogue.as_ref().as_ref().map_err(Clone::clone)?;
    let rows =
        veredictum_console::run_api::read::capability_rows(&state, engine_gate::gate_submitter())
            .map_err(|e| format!("capability rows: {e}"))?;
    let folders = listing.capabilities();
    assert_eq!(rows.len(), folders.len(), "one row per capability folder");
    for row in &rows {
        let documents = folders
            .get(row.name.as_str())
            .ok_or_else(|| format!("{} is not a folder of the listing", row.name))?;
        assert_eq!(row.documents, u64::try_from(documents.len())?);
        assert_eq!(
            row.cases,
            u64::try_from(documents.iter().map(|d| d.workflows.len()).sum::<usize>())?,
            "{} counted cases its documents do not carry",
            row.name
        );
    }
    Ok(())
}

/// The capability rows of a drafted state whose server answered System
/// OPTIONS with `endpoints`, or never stated the member when `None`.
fn rows_with_endpoints(
    endpoints: Option<&[&str]>,
) -> Result<Vec<veredictum_console::run_api::CapabilityRow>, Box<dyn std::error::Error>> {
    use veredictum_console::identity::{IdentityAnswer, StatedIdentity};

    let state = drafted_state();
    {
        let mut guard = state.draft.lock().map_err(|e| e.to_string())?;
        let draft = guard
            .get_mut(engine_gate::gate_submitter())
            .ok_or("the drafted state holds the gate submitter's draft")?;
        draft.identity = Some(IdentityAnswer::Stated(StatedIdentity {
            endpoints: endpoints.map(|list| list.iter().map(|e| (*e).to_owned()).collect()),
            ..StatedIdentity::default()
        }));
    }
    Ok(veredictum_console::run_api::read::capability_rows(
        &state,
        engine_gate::gate_submitter(),
    )?)
}

/// The names of the rows that start checked.
fn started(rows: &[veredictum_console::run_api::CapabilityRow]) -> Vec<&str> {
    rows.iter()
        .filter(|row| row.starts_checked)
        .map(|row| row.name.as_str())
        .collect()
}

/// #669: a server listing every top-level resource of the released
/// `Options.yaml` example starts every capability checked.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_full_endpoint_list_starts_every_capability_checked() -> Result<(), Box<dyn std::error::Error>>
{
    let rows = rows_with_endpoints(Some(&[
        "/ehr",
        "/demographic",
        "/definition",
        "/query",
        "/admin",
    ]))?;
    assert!(!rows.is_empty());
    assert_eq!(started(&rows).len(), rows.len(), "{rows:?}");
    Ok(())
}

/// #669: a server listing only `/ehr` starts the capabilities addressing only
/// `/ehr` (or the API root) checked, and every other one unchecked.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_partial_endpoint_list_starts_only_the_listed_capabilities_checked()
-> Result<(), Box<dyn std::error::Error>> {
    let rows = rows_with_endpoints(Some(&["/ehr/"]))?;
    for row in &rows {
        let only_ehr = row.resources.iter().all(|resource| resource == "/ehr");
        assert_eq!(row.starts_checked, only_ehr, "{row:?}");
    }
    let checked = started(&rows);
    assert!(checked.contains(&"ehr"), "{checked:?}");
    assert!(!checked.contains(&"query"), "{checked:?}");
    assert!(!checked.contains(&"demographic"), "{checked:?}");
    Ok(())
}

/// #669: a server that lists no endpoints, by an empty list or by leaving the
/// member out, starts every capability that addresses a resource unchecked,
/// and one that never stated the member starts every capability unchecked.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_server_listing_no_endpoints_starts_nothing_checked() -> Result<(), Box<dyn std::error::Error>>
{
    let rows = rows_with_endpoints(Some(&[]))?;
    for row in &rows {
        assert_eq!(row.starts_checked, row.resources.is_empty(), "{row:?}");
    }
    let unstated = rows_with_endpoints(None)?;
    assert_eq!(started(&unstated), Vec::<&str>::new());
    let state = drafted_state();
    let never_read =
        veredictum_console::run_api::read::capability_rows(&state, engine_gate::gate_submitter())?;
    assert_eq!(started(&never_read), Vec::<&str>::new());
    Ok(())
}

/// #100: a composed claim selects exactly the checked capability folders: a
/// document in a claimed folder runs, and one in an unclaimed folder is
/// skipped `not-claimed` with the statement schema's citation.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_composed_claim_selects_the_checked_capabilities() -> Result<(), Box<dyn std::error::Error>> {
    let binary = engine_gate::gate_binary();
    if !binary.exists() {
        eprintln!(
            "SKIPPED(no engine binary at {}): build the workspace first",
            binary.display()
        );
        return Ok(());
    }
    let engine = Engine::verified(&binary)?;
    let state = drafted_state();
    let scratch = assert_fs::TempDir::new()?;
    let port = engine_gate::fixture_sut()?;
    let ixit = engine_gate::write_ixit(scratch.path(), port)?;

    let run_claiming = |capability: &str| -> Result<_, Box<dyn std::error::Error>> {
        let document = veredictum_console::run_api::read::compose_claim(
            &state,
            engine_gate::gate_submitter(),
            &[String::from(capability)],
        )
        .map_err(|e| format!("compose: {e}"))?;
        let out = scratch.path().join(format!("claim-{capability}"));
        std::fs::create_dir_all(&out)?;
        // The claim travels with the run exactly as `start_run` writes it.
        let statement_path = out.join("statement.json");
        std::fs::write(&statement_path, &document)?;
        let spec = veredictum_console::engine::RunSpec {
            profile: veredictum::arazzo::spec_profile::SpecProfile::Development,
            statement: Some(statement_path),
            ..engine_gate::gate_spec(ixit.clone(), port, out)
        };
        Ok(engine.run(&spec, |_line| {})?.results)
    };

    let claimed = run_claiming("system")?;
    assert!(!claimed.rows.is_empty(), "the claimed document drove rows");
    assert!(
        claimed
            .rows
            .iter()
            .all(|row| row.status != veredictum::arazzo::results::Status::Skipped),
        "a document in a claimed folder runs"
    );

    let unclaimed = run_claiming("ehr")?;
    assert!(!unclaimed.rows.is_empty());
    assert!(
        unclaimed.rows.iter().all(|row| {
            row.skip.as_ref().is_some_and(|skip| {
                skip.reason == veredictum::arazzo::results::SkipReason::NotClaimed
            })
        }),
        "a document outside the claim is skipped not-claimed"
    );
    Ok(())
}

/// #100: the composed claim is a real statement — it passes the PUBLISHED
/// schema through the same save path a pasted one takes, and it claims
/// exactly the checked capabilities.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_composed_claim_saves_through_the_pasted_claim_path() -> Result<(), Box<dyn std::error::Error>>
{
    let state = drafted_state();
    let document = veredictum_console::run_api::read::compose_claim(
        &state,
        engine_gate::gate_submitter(),
        &[
            String::from("ehr"),
            String::from("system"),
            String::from("ehr"),
        ],
    )
    .map_err(|e| format!("compose: {e}"))?;
    let summary = veredictum_console::run_api::read::save_scope(
        &state,
        engine_gate::gate_submitter(),
        Some(document.clone()),
        None,
        &PostureForm::default(),
    )
    .map_err(|e| format!("the composed claim must pass its own schema: {e}"))?
    .ok_or("a composed claim must yield a summary")?;
    assert_eq!(summary.product, "claim-gate 0.0.0-gate");
    assert_eq!(
        summary.capabilities, 2,
        "a repeated name composes one claim"
    );

    // The empty selection claims nothing, so it is refused rather than
    // composed into a claim the engine would refuse anyway.
    assert!(
        veredictum_console::run_api::read::compose_claim(
            &state,
            engine_gate::gate_submitter(),
            &[]
        )
        .is_err()
    );
    // A name from a public endpoint that is no folder is refused by name.
    let unknown = veredictum_console::run_api::read::compose_claim(
        &state,
        engine_gate::gate_submitter(),
        &[String::from("NoSuchCapability")],
    );
    assert!(
        unknown.is_err_and(|e| matches!(e, ConsoleError::Invalid { .. })
            && e.to_string().contains("NoSuchCapability"))
    );
    Ok(())
}

/// #465: a declaration reaches the console only as PASTED BYTES.
///
/// The example loader this test used to fence read a client-supplied path
/// under a mounted party tree; the tree and the loader are both gone, so the
/// file-read primitive no longer exists. A path in the paste box is therefore
/// a document that is not a declaration, and it is refused as one, while the
/// fixture's own bytes are accepted.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn a_declaration_reaches_the_console_only_as_pasted_bytes() -> Result<(), Box<dyn std::error::Error>>
{
    let state = drafted_state();
    let fixture = engine_gate::declaration_fixture();
    for pasted in [
        String::from("../../etc/passwd"),
        String::from("party/ehrbase/statement.json"),
        fixture.display().to_string(),
    ] {
        let answer = veredictum_console::run_api::read::save_scope(
            &state,
            engine_gate::gate_submitter(),
            Some(pasted.clone()),
            None,
            &PostureForm::default(),
        );
        assert!(
            answer.is_err(),
            "{pasted} must be refused: a path is not a declaration"
        );
    }

    let summary = veredictum_console::run_api::read::save_scope(
        &state,
        engine_gate::gate_submitter(),
        Some(std::fs::read_to_string(&fixture)?),
        None,
        &PostureForm::default(),
    )
    .map_err(|e| format!("the fixture's own bytes must pass: {e}"))?
    .ok_or("pasted bytes must yield a summary")?;
    assert_eq!(summary.product, "Fixture CDR 0.0.0-fixture");
    Ok(())
}
/// The posture form is UNTRUSTED input on a public endpoint, and an unusable
/// declaration is REFUSED by name rather than dropped: an operator who
/// believes they declared a signing posture must never get a run that judged
/// without one (#456).
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn an_unusable_posture_declaration_is_refused_by_name() -> Result<(), Box<dyn std::error::Error>> {
    // Undeclared is a first-class answer: nothing declared, nothing refused.
    let none = veredictum_console::run_api::declared_postures(&PostureForm::default())
        .map_err(|e| format!("an empty form declares nothing: {e}"))?;
    assert_eq!(none, DeclaredPostures::default());

    // The pgp mode with no key: the operator picked a posture and supplied
    // nothing, which is exactly the confusion this refusal exists to name.
    let keyless = veredictum_console::run_api::declared_postures(&PostureForm {
        signing: SigningChoice::Pgp,
        ..PostureForm::default()
    })
    .expect_err("a pgp posture with no key cannot verify anything");
    assert!(
        matches!(keyless, ConsoleError::Invalid { .. })
            && keyless.to_string().contains("no public key"),
        "{keyless}"
    );

    let junk = veredictum_console::run_api::declared_postures(&PostureForm {
        signing: SigningChoice::Pgp,
        pgp_public_key: String::from("not a key"),
        ..PostureForm::default()
    })
    .expect_err("a key the verifier cannot read is refused");
    assert!(
        matches!(junk, ConsoleError::Invalid { .. })
            && junk.to_string().contains("PGP PUBLIC KEY BLOCK"),
        "{junk}"
    );

    let oversized = veredictum_console::run_api::declared_postures(&PostureForm {
        system_id: "s".repeat(4096),
        ..PostureForm::default()
    })
    .expect_err("an abusive system id is capped");
    assert_eq!(
        oversized,
        ConsoleError::TooLong {
            field: String::from("system id"),
            bytes: 4096,
            cap: 256,
        }
    );
    assert!(oversized.to_string().contains("the cap is"), "{oversized}");

    // A declared digest posture narrows to the typed carrier, parameters and
    // all, and an empty prefix stays empty rather than becoming a stand-in.
    let digest = veredictum_console::run_api::declared_postures(&PostureForm {
        signing: SigningChoice::Digest,
        digest_encoding: DigestEncoding::Base64Url,
        system_id: String::from("  cdr.example.org  "),
        spec_profile: SpecProfileChoice::Stable,
        ..PostureForm::default()
    })
    .map_err(|e| format!("a digest posture is declarable: {e}"))?;
    assert_eq!(
        digest,
        DeclaredPostures {
            system_id: Some(String::from("cdr.example.org")),
            dump_location: None,
            signing: Some(SigningPosture::Digest {
                encoding: DigestEncoding::Base64Url,
                prefix: String::new(),
            }),
            spec_profile: Some(veredictum::ixit::SpecProfile::Stable),
        }
    );
    Ok(())
}

/// The Scope save carries the postures onto the draft, and a REFUSED
/// declaration leaves the stored scope exactly as it was — the refusal is not
/// a half-applied save.
#[expect(
    clippy::panic_in_result_fn,
    reason = "the Book ch11 Result-returning test shape: assertions panic, plumbing propagates with ? (https://doc.rust-lang.org/book/ch11-01-writing-tests.html)"
)]
#[test]
fn the_scope_save_stores_the_postures_and_a_refusal_changes_nothing()
-> Result<(), Box<dyn std::error::Error>> {
    let state = drafted_state();
    let who = engine_gate::gate_submitter();
    veredictum_console::run_api::read::save_scope(
        &state,
        who,
        None,
        None,
        &PostureForm {
            signing: SigningChoice::Digest,
            digest_prefix: String::from("sha256:"),
            system_id: String::from("cdr.example.org"),
            ..PostureForm::default()
        },
    )
    .map_err(|e| format!("the posture save must pass: {e}"))?;
    let view =
        veredictum_console::run_api::read::draft_view(&state, who).ok_or("the draft reads back")?;
    assert_eq!(
        view.postures,
        vec![
            String::from("system_id cdr.example.org"),
            String::from("signing digest (sha256, base64, prefix \"sha256:\")"),
        ],
        "the interface states back exactly what was declared"
    );

    let refused = veredictum_console::run_api::read::save_scope(
        &state,
        who,
        None,
        None,
        &PostureForm {
            signing: SigningChoice::Pgp,
            ..PostureForm::default()
        },
    );
    assert!(refused.is_err(), "a keyless pgp posture is refused");
    let after = veredictum_console::run_api::read::draft_view(&state, who)
        .ok_or("the draft survives a refusal")?;
    assert_eq!(
        after.postures, view.postures,
        "a refused declaration overwrote the stored one"
    );
    Ok(())
}
