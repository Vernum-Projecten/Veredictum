// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! `perf-compare` against in-process fakes on a paused clock: an A/A run
//! reports nothing beyond its own floor and attaches the floor to both
//! records, an A/B run against two fakes 20 % apart reports the difference
//! with both values, the plateaus run ABBA per rung and the records show
//! it, mismatched records exit `1` naming the mismatch above the table, and
//! the host prescriptions load through their gates.

#![expect(
    clippy::unwrap_used,
    reason = "test-support helpers (not `#[test]` fns, so the clippy.toml in-tests scoping does not reach them) are panic-idiomatic: a broken harness must abort the test loudly, Book ch11"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use veredictum::perf_emulate::binding::Binding;
use veredictum::perf_emulate::run::{
    InterleavedOutcome, LadderOutcome, RunHooks, RunRequest, RunShape, run_interleaved, rung_order,
};
use veredictum::perf_gen::arrivals::{ArrivalProcess, Rate};
use veredictum::perf_gen::drive::GeneratorLimits;
use veredictum::perf_gen::{Call, Interaction, Mix, OperationId, Outcome, Performer};
use veredictum::perf_result::compare::{
    CompareMetric, InterleaveMode, RowReading, SideName, floor_of,
};
use veredictum::perf_result::derive::BetweenRunNoise;
use veredictum::perf_result::host::{
    HostPrescription, HostReading, NetworkLeg, load_host, load_hosts,
};
use veredictum::perf_result::{PerfResult, Raw, RawQualification};
use veredictum::pipeline::perf_compare::{Sides, assemble};
use veredictum::workload::Stage;
use veredictum::workload::ops::WorkloadOp;

/// A server that answers every request after a fixed service time.
struct Fixed(Duration);

impl Performer for Fixed {
    async fn perform(&self, _call: &Call) -> Outcome {
        tokio::time::sleep(self.0).await;
        Outcome::Success
    }
}

struct Quiet;

impl RunHooks for Quiet {
    fn progress(&self, _message: &str) {}

    async fn settle(&self) {}

    async fn closing(&self) {}
}

fn binding() -> Binding {
    let read = OperationId::new("ehr_read").unwrap();
    Binding {
        mix: Mix::new(vec![Interaction::single("read", read, 1_000_000).unwrap()]).unwrap(),
        stages: vec![vec![Stage {
            op: WorkloadOp::EhrRead,
            payload: None,
            query: None,
        }]],
        operations_per_interaction: 1.0,
        qualification: RawQualification {
            default_budget_ms: 1_000,
            budgets_ms: BTreeMap::new(),
            max_error_ppm: 1_000,
            min_achieved_ppm: 950_000,
        },
        dropped: Vec::new(),
    }
}

/// Drives an interleaved run on a paused clock, with a short scout so the
/// ladder stays near 50 operations per second.
fn interleave<A: Performer + 'static, B: Performer + 'static>(
    a: A,
    b: Arc<B>,
) -> InterleavedOutcome {
    let binding = binding();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap();
    runtime.block_on(async {
        let request = RunRequest {
            seed: 11,
            process: ArrivalProcess::Poisson,
            binding: &binding,
            limits: GeneratorLimits::default(),
            shape: RunShape::standard(1_000, Rate::new(8.0).unwrap(), Rate::new(64.0).unwrap()),
        };
        run_interleaved(&request, (Arc::new(a), &Quiet), (b, &Quiet))
            .await
            .unwrap()
    })
}

fn raw_of(outcome: &LadderOutcome) -> Raw {
    let mut raw = crate::perf_result::raw(outcome.plateaus.clone());
    raw.scout = Some(outcome.scout.clone());
    raw.phases.clone_from(&outcome.phases);
    raw
}

fn sides(outcome: &InterleavedOutcome) -> Sides<'static> {
    Sides {
        a: (raw_of(&outcome.a), "http://a.test/openehr/v1"),
        b: (raw_of(&outcome.b), "http://b.test/openehr/v1"),
    }
}

#[test]
fn an_aa_run_reports_no_difference_beyond_its_floor_and_attaches_it_to_both_records() {
    let fake = Arc::new(Fixed(Duration::from_millis(10)));
    let outcome = interleave(Fixed(Duration::from_millis(10)), Arc::clone(&fake));
    let done = assemble(sides(&outcome), &outcome, InterleaveMode::Aa, 0.05, None).unwrap();
    assert!(
        done.comparison.differences().is_empty(),
        "{:?}",
        done.comparison.differences()
    );
    let floor = done
        .comparison
        .floor
        .clone()
        .expect("an A/A run has a floor");
    assert!(floor.pairs > 0);
    for record in [&done.a, &done.b] {
        assert_eq!(record.raw.aa_floor.as_ref(), Some(&floor));
        assert_eq!(
            record.derived.between_run_noise,
            BetweenRunNoise::AaFloor(floor.clone())
        );
        let text = record.render().unwrap();
        let judged = veredictum::perf_result::judge::rejudge(&text).unwrap();
        assert!(judged.reproduces(), "{:?}", judged.mismatches);
    }
}

#[test]
fn an_ab_run_against_fakes_20_percent_apart_reports_the_difference_with_both_values() {
    let outcome = interleave(
        Fixed(Duration::from_millis(10)),
        Arc::new(Fixed(Duration::from_millis(12))),
    );
    let aa = interleave(
        Fixed(Duration::from_millis(10)),
        Arc::new(Fixed(Duration::from_millis(10))),
    );
    let aa_records = assemble(sides(&aa), &aa, InterleaveMode::Aa, 0.05, None).unwrap();
    let floor = floor_of(&aa_records.a, &aa_records.b);
    let done = assemble(
        sides(&outcome),
        &outcome,
        InterleaveMode::Ab,
        0.05,
        Some(floor.clone()),
    )
    .unwrap();
    let p50 = done
        .comparison
        .rows
        .iter()
        .find(|r| r.rung == 0 && r.operation == "all" && r.metric == CompareMetric::P50)
        .expect("a p50 row on the first rung");
    assert_eq!(p50.reading, RowReading::Difference, "{p50:?}");
    assert!((10.0..10.6).contains(&p50.a.unwrap()), "{p50:?}");
    assert!((12.0..12.6).contains(&p50.b.unwrap()), "{p50:?}");
    assert!((1.15..1.25).contains(&p50.ratio.unwrap()), "{p50:?}");
    assert_eq!(done.a.raw.aa_floor.as_ref(), Some(&floor));
    let markdown = veredictum::perf_result::compare::markdown(&done.comparison);
    assert!(markdown.contains("| 0 |"), "{markdown}");
    assert!(markdown.contains("| difference |"), "{markdown}");
}

#[test]
fn the_plateaus_run_abba_per_rung_and_both_records_show_it() {
    let outcome = interleave(
        Fixed(Duration::from_millis(5)),
        Arc::new(Fixed(Duration::from_millis(5))),
    );
    let (a, b) = (SideName::A, SideName::B);
    let sides_run: Vec<SideName> = outcome.sequence.iter().map(|s| s.side).collect();
    assert_eq!(sides_run, [a, b, b, a, a, b, b, a]);
    assert_eq!(rung_order(2), [a, b]);
    for pair in outcome.sequence.chunks(2) {
        assert_eq!(pair[0].rung, pair[1].rung);
        assert!((pair[0].planned_rate - pair[1].planned_rate).abs() < f64::EPSILON);
    }
    let done = assemble(sides(&outcome), &outcome, InterleaveMode::Ab, 0.05, None).unwrap();
    for (record, side) in [(&done.a, a), (&done.b, b)] {
        let interleaving = record.raw.interleaving.as_ref().unwrap();
        assert_eq!(interleaving.side, side);
        assert_eq!(interleaving.sequence, outcome.sequence);
        assert_eq!(record.derived.plateaus.len(), 4);
    }
    for (pa, pb) in done.a.derived.plateaus.iter().zip(&done.b.derived.plateaus) {
        assert!((pa.planned_rate - pb.planned_rate).abs() < f64::EPSILON);
        assert!((pa.hold_s - pb.hold_s).abs() < f64::EPSILON);
    }
}

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_veredictum")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "veredictum-perf-compare-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(record: &PerfResult, path: &Path) {
    std::fs::write(path, record.render().unwrap()).unwrap();
}

fn reference_host() -> HostPrescription {
    load_host(&repo().join("perf/hosts/reference-8cpu-8gb.yaml")).unwrap()
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn records_of_different_workloads_and_hosts_exit_1_naming_each_mismatch_above_the_table() {
    let dir = scratch("mismatch");
    let a = PerfResult::from_raw(crate::perf_result::raw(Vec::new())).unwrap();
    let mut other = crate::perf_result::raw(Vec::new());
    other.workload.sha256 = format!("sha256:{}", "9".repeat(64));
    other.host_prescription = Some(reference_host());
    other.network_leg = Some(NetworkLeg::SameZone);
    let b = PerfResult::from_raw(other).unwrap();
    let (pa, pb) = (dir.join("a.json"), dir.join("b.json"));
    write(&a, &pa);
    write(&b, &pb);
    let output = Command::new(binary())
        .args(["perf-compare", "--result"])
        .arg(&pa)
        .arg("--result")
        .arg(&pb)
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(output.status.code(), Some(1), "{stdout}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines[0].starts_with("MISMATCH workload"), "{stdout}");
    assert!(
        lines
            .iter()
            .take(3)
            .any(|l| l.starts_with("MISMATCH host_prescription")),
        "{stdout}"
    );
    assert!(
        lines
            .iter()
            .take(3)
            .any(|l| l.starts_with("MISMATCH network_leg")),
        "{stdout}"
    );
    let same = Command::new(binary())
        .args(["perf-compare", "--result"])
        .arg(&pa)
        .arg("--result")
        .arg(&pa)
        .output()
        .unwrap();
    assert_eq!(same.status.code(), Some(0));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn every_committed_host_prescription_loads_and_the_reference_host_is_first() {
    let hosts = load_hosts(&repo().join("perf")).unwrap();
    assert_eq!(hosts[0].id, "reference-8cpu-8gb");
    assert_eq!((hosts[0].vcpu, hosts[0].memory_gib), (8, 8));
    assert!(hosts[0].idle);
}

/// A prescription with one seeded defect is refused, naming the field.
#[test]
fn a_defective_host_prescription_is_refused_naming_its_field() {
    let dir = scratch("hosts");
    let base = std::fs::read_to_string(repo().join("perf/hosts/reference-8cpu-8gb.yaml")).unwrap();
    let cases = [
        ("stem", base.clone(), "id"),
        (
            "reference-8cpu-8gb",
            base.replace("machine_class: unstated", "machine_class: mainframe"),
            "machine_class",
        ),
        (
            "reference-8cpu-8gb",
            format!("{base}container_ceilings: {{ cpus: 16, memory_gib: 4 }}\n"),
            "container_ceilings.cpus",
        ),
        ("reference-8cpu-8gb", format!("{base}gpu: 1\n"), "gpu"),
    ];
    for (stem, text, field) in cases {
        let path = dir.join(format!("{stem}.yaml"));
        std::fs::write(&path, text).unwrap();
        let error = load_host(&path)
            .expect_err("the defect is refused")
            .to_string();
        assert!(error.contains(field), "{field}: {error}");
        std::fs::remove_file(&path).unwrap();
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_record_on_an_unmatched_prescription_is_labelled_unmatched() {
    let mut raw = crate::perf_result::raw(Vec::new());
    raw.host_prescription = Some(reference_host());
    raw.generator.host.available_parallelism = Some(4);
    raw.generator.host.total_memory_bytes = Some(8_000_000_000);
    let record = PerfResult::from_raw(raw).unwrap();
    assert!(
        matches!(&record.derived.host, HostReading::Unmatched { prescription, mismatches }
            if prescription == "reference-8cpu-8gb" && mismatches.len() == 1),
        "{:?}",
        record.derived.host
    );
    let view = veredictum::perf_result::view::terminal(&record, false);
    assert!(view.contains("UNMATCHED"), "{view}");
    let markdown = veredictum::perf_result::view::markdown(&record);
    assert!(markdown.contains("UNMATCHED"), "{markdown}");
}

/// `--with-baselines` proves the container runtime answers before it loads,
/// seeds or composes anything, and names the binary it could not invoke.
#[test]
fn a_baseline_run_without_a_container_runtime_is_refused_first() {
    use veredictum::perf_emulate::baselines::DockerCli;
    use veredictum::perf_emulate::posture::MINIMAL;
    use veredictum::pipeline::perf_compare::{InterleavedRequest, run_with_baselines};
    use veredictum::pipeline::perf_workload::{CorpusMode, WorkloadSettings};
    let root = repo().join("perf");
    let catalogue = repo().join("catalogue");
    let workload = root.join("workloads/clinical-mix.yaml");
    let out = scratch("baselines");
    let request = InterleavedRequest {
        settings: WorkloadSettings {
            root: &root,
            catalogue: &catalogue,
            workload: &workload,
            statement: None,
            corpus: CorpusMode::Seed,
            seed_workers: 1,
            start_rate: 8.0,
            scout_end_rate: 64.0,
            host_prescription: None,
            network_leg: NetworkLeg::SameHost,
            posture: &MINIMAL,
        },
        ixit_a: Path::new("/nonexistent/ixit.json"),
        ixit_b: None,
        out: &out,
        tolerance: 0.05,
        floor: None,
    };
    let docker = DockerCli::at("/nonexistent/veredictum/docker");
    let error = run_with_baselines(&request, &docker, &|_| {})
        .expect_err("no runtime, no baseline")
        .to_string();
    assert!(error.contains("/nonexistent/veredictum/docker"), "{error}");
    std::fs::remove_dir_all(&out).unwrap();
}
