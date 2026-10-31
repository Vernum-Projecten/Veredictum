// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The workload loader's gates, each proven by a seeded defect.
//!
//! Every test starts from the committed reference workload, seeds exactly one
//! defect into its text, and requires the loader to refuse the document with
//! an error naming the file and the field. The committed document itself must
//! load clean, inside the read band, within the ten-minute plan, and match
//! its pinned digest.

#![expect(
    clippy::expect_used,
    reason = "test-support helpers (not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them) are panic-idiomatic: a broken harness must abort the test loudly, Book ch11"
)]

use std::path::PathBuf;

use veredictum::workload::load::{
    LoadContext, PINNED, WorkloadIdentity, identify, load_file, sha256_hex,
};
use veredictum::workload::manifest::{describe_pinned, render_json, render_page, summary};
use veredictum::workload::{Claimed, MixStatus};

fn repo() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn committed() -> PathBuf {
    repo().join("perf/workloads/clinical-mix.yaml")
}

fn context() -> LoadContext {
    LoadContext::from_roots(&repo().join("perf"), &repo().join("catalogue"))
        .expect("the payload manifest and the catalogue vocabulary load")
}

/// Writes `text` as `clinical-mix.yaml` in a fresh directory and loads it.
fn load_text(text: &str) -> Result<veredictum::workload::load::LoadedWorkload, Vec<String>> {
    let dir = assert_fs::TempDir::new().expect("temp dir");
    let path = dir.path().join("clinical-mix.yaml");
    std::fs::write(&path, text).expect("writing the seeded document");
    load_file(&path, &context()).map_err(|refused| {
        refused
            .errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    })
}

/// Seeds `to` in place of the first `from` and requires a refusal naming
/// `field` and `words`.
fn refused_with(from: &str, to: &str, field: &str, words: &str) {
    let text = std::fs::read_to_string(committed()).expect("the reference workload");
    assert!(
        text.contains(from),
        "the seed {from:?} is not in the document"
    );
    let seeded = text.replacen(from, to, 1);
    let errors = load_text(&seeded).expect_err("the seeded defect is refused");
    assert!(
        errors
            .iter()
            .any(|e| e.contains("clinical-mix.yaml") && e.contains(field) && e.contains(words)),
        "no error names {field:?} with {words:?}: {errors:#?}"
    );
}

#[test]
fn the_reference_workload_loads_clean_and_matches_its_pin() {
    let loaded = load_file(&committed(), &context()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(loaded.workload.status, MixStatus::Provisional);
    assert!(
        loaded.mix.inside_read_band(),
        "{:?}",
        loaded.mix.reads_writes()
    );
    assert!(loaded.plan.within_limit(), "{:?}", loaded.plan);
    let bytes = std::fs::read(committed()).expect("the reference workload");
    let pin = PINNED
        .iter()
        .find(|p| p.file == "clinical-mix.yaml")
        .expect("clinical-mix is pinned");
    assert_eq!(
        pin.sha256,
        sha256_hex(&bytes),
        "perf/workloads/clinical-mix.yaml changed: raise its version and its pin"
    );
    assert_eq!(loaded.identity, WorkloadIdentity::Pinned(pin));
    assert_eq!(loaded.identity.label(), "clinical-mix@2");
}

/// Whether `text` reads as whole sentences: it ends with a full stop and
/// every parenthesis it opens is closed.
fn is_whole(text: &str) -> bool {
    let opened = text.matches('(').count();
    let closed = text.matches(')').count();
    text.trim_end().ends_with('.') && opened == closed
}

/// A plain YAML scalar ends at ` #` (YAML 1.2.2 §6.6,
/// <https://yaml.org/spec/1.2.2/#66-comments>), so a derivation written as
/// `… (removed in #892): …` without a block scalar loads cut mid-sentence;
/// every pinned workload's description and derivations load whole.
#[test]
fn every_pinned_derivation_loads_whole() {
    let ctx = context();
    for pin in PINNED {
        let path = repo().join("perf/workloads").join(pin.file);
        let loaded = load_file(&path, &ctx).unwrap_or_else(|e| panic!("{e}"));
        assert!(
            is_whole(&loaded.workload.description),
            "{}: the description is cut: {:?}",
            pin.file,
            loaded.workload.description
        );
        for (name, interaction) in &loaded.workload.interactions {
            assert!(
                is_whole(&interaction.derivation),
                "{}: {name}: the derivation is cut: {:?}",
                pin.file,
                interaction.derivation
            );
        }
    }
}

/// The wholeness check refuses the cut a ` #` comment makes in a plain
/// scalar, so the test above cannot pass on a truncated document.
#[test]
fn a_derivation_cut_by_a_comment_is_not_whole() {
    let text = std::fs::read_to_string(committed()).expect("the reference workload");
    let whole = "derivation: >-\n      From the class-run journeys of perf/journeys.yaml at commit 20864091 (removed in #892): audit_review (4%), its EHR read.";
    assert!(text.contains(whole), "the seed is not in the document");
    let seeded = text.replacen(
        whole,
        "derivation: From the class-run journeys of perf/journeys.yaml at commit 20864091 (removed in #892): audit_review (4%), its EHR read.",
        1,
    );
    let loaded = load_text(&seeded).expect("a cut derivation still loads");
    let cut = loaded
        .workload
        .interactions
        .values()
        .map(|interaction| interaction.derivation.as_str())
        .find(|derivation| derivation.ends_with("(removed in"))
        .expect("the plain scalar is cut at the comment");
    assert!(!is_whole(cut), "{cut:?}");
}

#[test]
fn every_interaction_names_its_derivation() {
    let loaded = load_file(&committed(), &context()).unwrap_or_else(|e| panic!("{e}"));
    for (name, interaction) in &loaded.workload.interactions {
        assert!(
            interaction.derivation.split_whitespace().count() >= 4,
            "{name} carries no derivation"
        );
    }
    assert!(loaded.workload.description.contains("Provisional"));
}

#[test]
fn a_document_matching_no_pin_is_custom() {
    let text = std::fs::read_to_string(committed()).expect("the reference workload");
    let changed = text.replacen("seed: 20261030", "seed: 20261031", 1);
    assert_eq!(identify(changed.as_bytes()), WorkloadIdentity::Custom);
    let loaded = load_text(&changed).expect("a reseeded document still loads");
    assert_eq!(loaded.identity.label(), "custom");
}

#[test]
fn shares_that_do_not_sum_to_100_are_refused() {
    refused_with(
        "share: 48.8%",
        "share: 48.7%",
        "interactions:",
        "sum to 99.9%",
    );
}

#[test]
fn a_zero_share_is_refused() {
    refused_with(
        "share: 0.1%\n    description: A registered identity",
        "share: 0%\n    description: A registered identity",
        "interactions.party_update",
        "0%",
    );
}

#[test]
fn a_template_mix_that_does_not_sum_is_refused() {
    refused_with(
        "cnf.ckm.problem_list: 5%",
        "cnf.ckm.problem_list: 6%",
        "corpus.templates",
        "sum to 101%",
    );
}

#[test]
fn a_transport_mix_that_does_not_sum_is_refused() {
    refused_with(
        "stored_post: 25%\n    fetch: 20",
        "stored_post: 20%\n    fetch: 20",
        "queries.patient_summary.transport",
        "sum to 95%",
    );
}

#[test]
fn a_session_over_the_think_time_limit_is_refused() {
    refused_with(
        "think_time_mean_ms: 2000",
        "think_time_mean_ms: 10001",
        "interactions.chart_review.session.think_time_mean_ms",
        "10000 ms limit",
    );
}

#[test]
fn an_operation_outside_the_vocabulary_is_refused() {
    refused_with(
        "op: system_options",
        "op: ehr_extract_export",
        "/interactions/system_options/op",
        "schema",
    );
}

#[test]
fn an_undeclared_query_is_refused() {
    refused_with(
        "    op: aql\n    query: cohort_count",
        "    op: aql\n    query: cohort_census",
        "interactions.cohort_count.query",
        "not a declared query class",
    );
}

#[test]
fn a_payload_missing_from_the_manifest_is_refused() {
    refused_with(
        "payload: cnf.ckm.vital_signs",
        "payload: cnf.ckm.vital_signs_missing",
        "interactions.vitals_round.payload",
        "not in payloads.yaml",
    );
}

#[test]
fn a_payload_on_an_operation_that_sends_none_is_refused() {
    refused_with(
        "    op: ehr_read\n",
        "    op: ehr_read\n    payload: cnf.ckm.vital_signs\n",
        "interactions.ehr_read.payload",
        "sends no payload",
    );
}

#[test]
fn a_query_that_does_not_parse_is_refused() {
    refused_with(
        "SELECT COUNT(DISTINCT e/ehr_id/value) AS patients",
        "SELECT COUNT(DISTINCT e/ehr_id/value) AS patients WHERE",
        "queries.cohort_count.text",
        "does not parse",
    );
}

#[test]
fn a_parameter_without_a_source_is_refused() {
    refused_with(
        "      systolic:\n        int_range: [130, 180]",
        "      threshold:\n        int_range: [130, 180]",
        "queries.cohort_count.parameters",
        "$systolic has no declared source",
    );
}

#[test]
fn a_population_query_binding_the_ehr_is_refused() {
    refused_with(
        "    scope: single_subject\n    tier: metadata\n    parameters:\n      ehr_uid: ehr\n    transport:\n      adhoc_post: 50%",
        "    scope: population\n    tier: metadata\n    parameters:\n      ehr_uid: ehr\n    transport:\n      adhoc_post: 50%",
        "queries.patient_summary.parameters",
        "population query binds no EHR scope",
    );
}

#[test]
fn a_reserved_parameter_name_is_refused() {
    refused_with(
        "SELECT c/uid/value FROM EHR e[ehr_id/value=$ehr_uid] CONTAINS COMPOSITION c\n    scope: single_subject\n    tier: metadata\n    parameters:\n      ehr_uid: ehr",
        "SELECT c/uid/value FROM EHR e[ehr_id/value=$ehr_id] CONTAINS COMPOSITION c\n    scope: single_subject\n    tier: metadata\n    parameters:\n      ehr_id: ehr",
        "queries.aql_ehr_scan.parameters.ehr_id",
        "use this name for themselves",
    );
}

#[test]
fn a_stored_post_without_fetch_is_refused() {
    refused_with(
        "      stored_post: 25%\n    fetch: 20\n",
        "      stored_post: 25%\n",
        "queries.patient_summary.fetch",
        "Query body requires",
    );
}

#[test]
fn a_requires_token_naming_nothing_is_refused() {
    refused_with(
        "requires: [aql-terminology-supported]",
        "requires: [aql-terminology-maybe]",
        "queries.problem_list_terminology.requires",
        "neither a capability folder",
    );
}

#[test]
fn a_mix_outside_the_read_band_is_refused() {
    let text = std::fs::read_to_string(committed()).expect("the reference workload");
    let seeded = text.replacen("share: 48.8%", "share: 28.8%", 1).replacen(
        "vitals_round:\n    share: 5%",
        "vitals_round:\n    share: 25%",
        1,
    );
    let errors = load_text(&seeded).expect_err("a write-heavy mix is refused");
    assert!(
        errors
            .iter()
            .any(|e| e.contains("interactions:") && e.contains("read band")),
        "{errors:#?}"
    );
}

#[test]
fn a_looser_latency_limit_is_refused() {
    refused_with(
        "sample_target: 5000\n",
        "sample_target: 5000\nqualification:\n  operation_p99_ms: 2000\n",
        "qualification.operation_p99_ms",
        "looser than the ruled 1000 ms",
    );
}

#[test]
fn a_looser_error_share_is_refused() {
    refused_with(
        "sample_target: 5000\n",
        "sample_target: 5000\nqualification:\n  max_error_share: 1%\n",
        "qualification.max_error_share",
        "looser than the ruled 0.1%",
    );
}

#[test]
fn a_looser_achieved_share_is_refused() {
    refused_with(
        "sample_target: 5000\n",
        "sample_target: 5000\nqualification:\n  min_achieved_share: 90%\n",
        "qualification.min_achieved_share",
        "looser than the ruled 95%",
    );
}

#[test]
fn a_looser_population_budget_is_refused() {
    refused_with(
        "sample_target: 5000\n",
        "sample_target: 5000\nqualification:\n  population_aql_p99_ms: 6000\n",
        "qualification.population_aql_p99_ms",
        "looser than the ruled 5000 ms",
    );
}

#[test]
fn a_stricter_limit_is_accepted() {
    let text = std::fs::read_to_string(committed()).expect("the reference workload");
    let seeded = text.replacen(
        "sample_target: 5000\n",
        "sample_target: 5000\nqualification:\n  operation_p99_ms: 500\n",
        1,
    );
    let loaded = load_text(&seeded).expect("a stricter limit loads");
    assert_eq!(loaded.workload.qualification.operation_p99_ms, 500);
}

#[test]
fn an_over_long_plan_is_refused() {
    refused_with(
        "think_time_mean_ms: 2000",
        "think_time_mean_ms: 4000",
        "interactions:",
        "over the 600 s limit",
    );
}

#[test]
fn a_sample_target_below_the_floor_is_refused() {
    refused_with(
        "sample_target: 5000",
        "sample_target: 500",
        "sample_target",
        "could resolve its p99",
    );
}

#[test]
fn an_id_that_is_not_the_file_stem_is_refused() {
    refused_with("id: clinical-mix", "id: clinical-max", "id:", "file stem");
}

/// A statement that claims neither Simplified Formats nor the terminology arm
/// drops the two interactions that need them, names what each misses, and
/// renormalizes what remains.
#[test]
fn a_statement_drops_what_it_does_not_claim() {
    let loaded = load_file(&committed(), &context()).unwrap_or_else(|e| panic!("{e}"));
    let ctx = context();
    let mut claimed = Claimed {
        folders: ctx.vocabulary.folders.clone(),
        arms: ctx.vocabulary.arms.clone(),
    };
    claimed.folders.remove("simplified_formats");
    claimed.arms.remove("aql-terminology-supported");
    let applied = loaded.workload.apply_claims(&claimed);
    let dropped: Vec<(String, Vec<String>)> = applied
        .dropped
        .iter()
        .map(|d| (d.interaction.to_string(), d.missing.clone()))
        .collect();
    assert_eq!(
        dropped,
        vec![
            (
                "flat_commit".to_owned(),
                vec!["simplified_formats".to_owned()]
            ),
            (
                "problem_list_terminology".to_owned(),
                vec!["aql-terminology-supported".to_owned()]
            ),
        ]
    );
    let total: f64 = applied.kept.values().sum();
    assert!((total - 1.0).abs() < 1e-9, "renormalized to {total}");
}

/// The manifest is byte-deterministic, and its page lists every operation,
/// query and payload with a share and a source.
#[test]
fn the_manifest_is_deterministic_and_its_page_lists_everything() {
    let ctx = context();
    let perf = repo().join("perf");
    let first = render_json(&describe_pinned(&perf, &ctx).expect("describes")).expect("renders");
    let second = render_json(&describe_pinned(&perf, &ctx).expect("describes")).expect("renders");
    assert_eq!(first, second);
    let manifest = describe_pinned(&perf, &ctx).expect("describes");
    let page = render_page(&manifest);
    let workload = manifest
        .workloads
        .iter()
        .find(|w| w.id == "clinical-mix")
        .expect("the reference workload is pinned");
    assert_eq!(
        workload.sha256,
        sha256_hex(&std::fs::read(committed()).expect("reads"))
    );
    for op in &workload.operations {
        assert!(
            page.contains(&format!("| {} | {} |", op.key, op.share_of_operations)),
            "{}",
            op.key
        );
        assert!(
            !op.released.is_empty(),
            "{} names no released operation",
            op.key
        );
    }
    for query in &workload.queries {
        assert!(
            page.contains(&format!(
                "| {} | {} |",
                query.name, query.share_of_operations
            )),
            "{}",
            query.name
        );
    }
    for payload in &workload.payloads {
        assert!(
            page.contains(&format!("| {} | {} |", payload.key, payload.bytes)),
            "{}",
            payload.key
        );
        assert!(payload.bytes > 0, "{}", payload.key);
    }
    let loaded = load_file(&committed(), &ctx).unwrap_or_else(|e| panic!("{e}"));
    let text = summary(&loaded);
    assert!(
        text.contains("planned run") && text.contains("reads:writes"),
        "{text}"
    );
}

/// The committed manifest validates against its published schema.
#[test]
fn the_manifest_validates_against_its_schema() {
    let ctx = context();
    let manifest = describe_pinned(&repo().join("perf"), &ctx).expect("describes");
    let text = render_json(&manifest).expect("renders");
    let document: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    let schema = veredictum::schema::perf_workloads_schema();
    let validator = jsonschema::validator_for(&schema).expect("compiles");
    let violations: Vec<String> = validator
        .iter_errors(&document)
        .map(|e| format!("{}: {e}", e.instance_path()))
        .collect();
    assert!(violations.is_empty(), "{violations:#?}");
}

/// The book's workload page and the manifest beside it are what
/// `perf-workloads --out website/book/src` writes from the pinned workloads,
/// byte for byte, so the published methodology cannot drift from the
/// documents a run offers.
#[test]
fn the_published_workload_page_is_what_the_manifest_renders() {
    let perf = repo().join("perf");
    let manifest = describe_pinned(&perf, &context()).expect("describes");
    let book = repo().join("website/book/src");
    let committed_json =
        std::fs::read_to_string(book.join("perf-workloads.json")).expect("the committed manifest");
    let committed_page =
        std::fs::read_to_string(book.join("perf-workloads.md")).expect("the committed page");
    assert_eq!(
        committed_json,
        render_json(&manifest).expect("renders"),
        "regenerate with `veredictum perf-workloads --root perf --catalogue catalogue --out website/book/src`"
    );
    assert_eq!(
        committed_page,
        render_page(&manifest),
        "regenerate with `veredictum perf-workloads --root perf --catalogue catalogue --out website/book/src`"
    );
}

/// A closed-loop walk issues reads of one seeded composition only; a write
/// in it is refused, naming the field.
#[test]
fn a_walk_that_writes_is_refused() {
    refused_with(
        "  compositions_per_ehr: 10\n",
        "  compositions_per_ehr: 10\n  walk:\n    reads: [composition_read, composition_delete]\n",
        "corpus.walk.reads",
        "composition_delete is not a read",
    );
}

/// A mix that leaves the read band is refused unless the document names the
/// published harness it reproduces, and the same mix loads once it does.
#[test]
fn only_a_reproduction_is_exempt_from_the_read_band() {
    let text = std::fs::read_to_string(repo().join("perf/workloads/aql-mix.yaml"))
        .expect("the aql-mix workload");
    let (head, rest) = text
        .split_once("reproduces: >-")
        .expect("aql-mix names its harness");
    let (_, tail) = rest
        .split_once("\ndescription:")
        .expect("a description follows");
    let unmarked = format!("{head}description:{tail}");
    let dir = assert_fs::TempDir::new().expect("temp dir");
    let path = dir.path().join("aql-mix.yaml");
    std::fs::write(&path, &unmarked).expect("writing the document");
    let refused = load_file(&path, &context()).expect_err("a reads-only mix is refused");
    assert!(
        refused
            .errors
            .iter()
            .any(|e| e.to_string().contains("outside the 10:1 to 50:1 read band")),
        "{refused}"
    );
    std::fs::write(&path, &text).expect("writing the document");
    assert!(load_file(&path, &context()).is_ok());
}
