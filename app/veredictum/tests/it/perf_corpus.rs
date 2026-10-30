// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The measured run's corpus and performer against a fake SUT on a real
//! socket: the closed-loop seeding writes a stored state that a later run
//! reuses only while its digest, its recipe and the server's read-back
//! counts match, and the performer sends every operation of the reference
//! workload as its released exchange, counting a refused optional query
//! apart from errors.

#![expect(
    clippy::unwrap_used,
    reason = "test-support helpers (not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them) are panic-idiomatic: a broken harness must abort the test loudly, Book ch11"
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use veredictum::perf_emulate::binding::{Binding, bind};
use veredictum::perf_emulate::client::PerfClient;
use veredictum::perf_emulate::corpus::{
    self, CorpusError, CorpusState, Needs, Payloads, SeedRequest,
};
use veredictum::perf_emulate::http::HttpClient;
use veredictum::perf_emulate::performer::{Targets, WirePerformer, read_current};
use veredictum::perf_gen::{Call, OperationId, Outcome, Performer};
use veredictum::workload::Workload;
use veredictum::workload::load::{LoadContext, load_file};
use veredictum::workload::ops::WorkloadOp;
use wiremock::matchers::{body_string_contains, method, path, path_regex};
use wiremock::{Mock, Request, ResponseTemplate};

use crate::fake_sut::{FakeSut, ixit};

fn repo() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).to_owned()
}

fn workload() -> (Workload, Payloads) {
    let ctx = LoadContext::from_roots(&repo().join("perf"), &repo().join("catalogue")).unwrap();
    let loaded = load_file(&repo().join("perf/workloads/clinical-mix.yaml"), &ctx).unwrap();
    let payloads = Payloads::load(&loaded.workload, &ctx.payloads_dir, &ctx.payloads).unwrap();
    (loaded.workload, payloads)
}

/// A fake SUT that creates everything it is sent, names each object by a
/// fresh id, answers every read with an `ETag`, and reads back the counts
/// it is told to.
fn sut(ehrs: Arc<AtomicU64>, compositions: Arc<AtomicU64>) -> FakeSut {
    let sut = FakeSut::start();
    let next = Arc::new(AtomicU64::new(1));
    let counts = move |request: &Request| {
        let text = String::from_utf8_lossy(&request.body).into_owned();
        let n = if text.contains("COMPOSITION") {
            compositions.load(Ordering::SeqCst)
        } else {
            ehrs.load(Ordering::SeqCst)
        };
        ResponseTemplate::new(200).set_body_json(serde_json::json!({ "rows": [[n]] }))
    };
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/query/aql"))
            .and(body_string_contains("COUNT("))
            .respond_with(counts),
    );
    sut.mount(
        Mock::given(method("POST"))
            .and(path("/query/aql"))
            .and(body_string_contains("TERMINOLOGY("))
            .respond_with(ResponseTemplate::new(400)),
    );
    sut.mount(
        Mock::given(method("POST"))
            .and(path_regex("^/query/"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "rows": [] })),
            ),
    );
    let created = move |request: &Request| {
        let id = next.fetch_add(1, Ordering::SeqCst);
        let uid = format!("00000000-0000-4000-8000-{id:012}");
        ResponseTemplate::new(201)
            .insert_header("Location", format!("{}/{uid}", request.url.path()))
            .insert_header("ETag", format!("\"{uid}::fake::1\""))
    };
    sut.mount(Mock::given(method("POST")).respond_with(created));
    let read = |request: &Request| {
        let at = request
            .url
            .path()
            .rsplit('/')
            .next()
            .unwrap_or("x")
            .to_owned();
        ResponseTemplate::new(200)
            .insert_header("ETag", format!("\"{at}::fake::1\""))
            .set_body_json(serde_json::json!({ "rows": [] }))
    };
    sut.mount(Mock::given(method("GET")).respond_with(read));
    sut.mount(Mock::given(method("PUT")).respond_with(read));
    sut.mount(Mock::given(method("DELETE")).respond_with(ResponseTemplate::new(204)));
    sut.mount(Mock::given(method("OPTIONS")).respond_with(ResponseTemplate::new(200)));
    sut
}

fn client(sut: &FakeSut) -> HttpClient {
    let ixit = ixit(&sut.base_url());
    let principal = PerfClient::from_instance(ixit.default_instance().unwrap(), &ixit).unwrap();
    HttpClient::new(principal).unwrap()
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn seeded(
    sut: &FakeSut,
    workload: &Workload,
    payloads: Payloads,
    binding: &Binding,
) -> CorpusState {
    let request = SeedRequest {
        recipe: &workload.corpus,
        ehrs: 3,
        workers: 2,
        seed: workload.seed,
        token: "test".to_owned(),
        needs: Needs::of(&binding.operations()),
    };
    let (state, report) = runtime()
        .block_on(corpus::seed(
            &client(sut),
            Arc::new(payloads),
            workload,
            &request,
        ))
        .unwrap();
    assert!(report.requests > 30, "{report:?}");
    state
}

#[test]
fn a_seeded_corpus_is_reused_only_while_it_matches() {
    let (workload, payloads) = workload();
    let binding = bind(&workload, None).unwrap();
    let ehrs = Arc::new(AtomicU64::new(3));
    let compositions = Arc::new(AtomicU64::new(30));
    let sut = sut(Arc::clone(&ehrs), Arc::clone(&compositions));
    let state = seeded(&sut, &workload, payloads, &binding);
    assert_eq!(state.listing.ehrs.len(), 3);
    assert!(
        state
            .listing
            .ehrs
            .iter()
            .all(|e| e.compositions.len() == 10)
    );
    assert!(
        state
            .listing
            .ehrs
            .iter()
            .all(|e| e.subject_id.starts_with("test-"))
    );
    assert_eq!(
        (state.ehrs_read_back, state.compositions_read_back),
        (3, 30)
    );

    let dir = assert_fs::TempDir::new().unwrap();
    let file = dir.path().join("corpus.json");
    state.write(&file).unwrap();
    let stored = CorpusState::read(&file).unwrap();
    assert_eq!(stored, state);
    let client = client(&sut);
    runtime()
        .block_on(corpus::reuse(&client, &workload, &stored))
        .unwrap();

    compositions.store(31, Ordering::SeqCst);
    let changed = runtime().block_on(corpus::reuse(&client, &workload, &stored));
    assert!(
        matches!(&changed, Err(CorpusError::Refused(m)) if m.contains("31 compositions")),
        "{changed:?}"
    );
    compositions.store(30, Ordering::SeqCst);

    let mut other = workload.clone();
    other.corpus.compositions_per_ehr = 11;
    let recipe = runtime().block_on(corpus::reuse(&client, &other, &stored));
    assert!(matches!(recipe, Err(CorpusError::Refused(_))), "{recipe:?}");

    let text = std::fs::read_to_string(&file).unwrap();
    let first = &state.listing.ehrs[0].ehr_id;
    std::fs::write(&file, text.replacen(first.as_str(), "tampered", 1)).unwrap();
    let tampered = CorpusState::read(&file);
    assert!(
        matches!(&tampered, Err(CorpusError::Refused(m)) if m.contains("hashes to")),
        "{tampered:?}"
    );
}

#[test]
fn every_reference_operation_is_sent_and_an_optional_refusal_counted_apart() {
    let (workload, payloads) = workload();
    let binding = Arc::new(bind(&workload, None).unwrap());
    let sut = sut(Arc::new(AtomicU64::new(3)), Arc::new(AtomicU64::new(30)));
    let payloads = Arc::new(payloads);
    let state = seeded(&sut, &workload, (*payloads).clone(), &binding);
    let client = client(&sut);
    let runtime = runtime();
    let mut requests = 0;
    let current = runtime
        .block_on(read_current(&client, &state, &binding, &mut requests))
        .unwrap();
    let performer = WirePerformer::new(
        client,
        Arc::clone(&binding),
        &workload,
        payloads,
        Targets::new(state, &current),
    );
    let mut refused = Vec::new();
    for (interaction, stages) in binding.stages.iter().enumerate() {
        for (stage, workload_stage) in stages.iter().enumerate() {
            let call = Call {
                arrival: u64::try_from(interaction * 10 + stage).unwrap(),
                stage: u32::try_from(stage).unwrap(),
                interaction,
                operation: OperationId::new(workload_stage.key().as_str()).unwrap(),
            };
            match runtime.block_on(performer.perform(&call)) {
                Outcome::Success => {}
                Outcome::Refused => refused.push(workload_stage.key().to_string()),
                failure @ Outcome::Failure { .. } => {
                    panic!("{} failed: {failure:?}", workload_stage.key())
                }
            }
        }
    }
    assert_eq!(refused, vec!["aql:problem_list_terminology".to_owned()]);
    let sent = sut.requests();
    let by_subject = sent
        .iter()
        .find(|r| r.url.path() == "/ehr" && r.method == wiremock::http::Method::GET)
        .expect("the subject lookup was sent");
    let query = by_subject.url.query().unwrap_or_default();
    assert!(
        query.contains("subject_namespace=org.veredictum.perf"),
        "{query}"
    );
    let update = sent
        .iter()
        .find(|r| r.method == wiremock::http::Method::PUT && r.url.path().ends_with("/ehr_status"))
        .expect("the status update was sent");
    assert!(update.headers.get("if-match").is_some());
    assert!(
        binding
            .operations()
            .contains(&WorkloadOp::CompositionDelete)
    );
}
