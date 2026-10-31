// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Comparing performance records: aligning committed records, and the
//! interleaved A/B and A/A runs on one host.
//!
//! Alignment reads two or more `perf-result` records and names every
//! mismatch above one table. An interleaved run drives two targets (A/B) or
//! one target on both sides (A/A) with the same workload, corpus recipe and
//! seed: each side settles, warms up and scouts, the shared ladder is built
//! from the larger knee estimate, and the plateaus alternate per rung in
//! ABBA order. An A/A run attaches its floor to both of its records; an A/B
//! run attaches a floor it is given. No openEHR specification governs
//! this: our own design.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;

use crate::perf_emulate::baselines::{DockerCli, ReferenceCdr, compose};
use crate::perf_emulate::posture::Bracket;
use crate::perf_emulate::run::{InterleavedOutcome, RunRequest, run_interleaved};
use crate::perf_result::compare::{
    AaFloor, CompareMode, InterleaveMode, Interleaving, PerfComparison, SideName, compare,
    floor_of, markdown,
};
use crate::perf_result::{PerfResult, Raw};
use crate::pipeline::Error;
use crate::pipeline::perf_workload::{
    Declared, OpenTarget, PostureCheck, Prepared, ReadyTarget, WorkloadSettings, instrument,
    live_hooks, open_target, prepare, raw_record, ready_target, refuse_rate_limited, runtime,
    store_read_back, write_record,
};

/// The file an interleaved run writes side A's record to.
pub const A_RECORD: &str = "a.perf-result.json";
/// The file an interleaved run writes side B's record to.
pub const B_RECORD: &str = "b.perf-result.json";
/// The comparison document.
pub const COMPARISON_FILE: &str = "perf-compare.json";
/// The comparison's Markdown view.
pub const COMPARISON_MARKDOWN: &str = "perf-compare.md";

fn read_record(path: &Path) -> Result<PerfResult, Error> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_owned(),
        source,
    })?;
    PerfResult::read(&text).map_err(|e| Error::Party(format!("{}: {e}", path.display())))
}

#[derive(Deserialize)]
struct FloorCarrier {
    #[serde(default)]
    family: Option<String>,
    #[serde(default)]
    floor: Option<AaFloor>,
}

/// Reads the A/A floor from a `perf-result` that carries one or from a
/// `perf-compare` document of an A/A run.
///
/// # Errors
/// [`Error::Read`] or [`Error::Parse`] for the file, and [`Error::Party`]
/// when it carries no floor.
pub fn read_floor(path: &Path) -> Result<AaFloor, Error> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_owned(),
        source,
    })?;
    let carrier: FloorCarrier = serde_json::from_str(&text).map_err(|e| Error::Parse {
        context: path.display().to_string(),
        message: e.to_string(),
    })?;
    let floor = match carrier.family.as_deref() {
        Some("perf-result") => read_record(path)?.raw.aa_floor,
        Some("perf-compare") => carrier.floor,
        _ => None,
    };
    floor.ok_or_else(|| {
        Error::Party(format!(
            "{}: carries no A/A floor; an A/A run of perf-compare writes one",
            path.display()
        ))
    })
}

/// Aligns committed records, labelled by their file names.
///
/// # Errors
/// [`Error::Party`] when fewer than two records are named or a record is
/// refused, and the errors of [`read_floor`].
pub fn align_records(
    paths: &[PathBuf],
    tolerance: f64,
    floor: Option<&Path>,
) -> Result<PerfComparison, Error> {
    if paths.len() < 2 {
        return Err(Error::Party(
            "perf-compare aligns two or more records".to_owned(),
        ));
    }
    let records = paths
        .iter()
        .map(|p| {
            let label = p.file_name().map_or_else(
                || p.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            Ok((label, read_record(p)?))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let floor = match floor {
        Some(path) => Some(read_floor(path)?),
        None => None,
    };
    let labelled: Vec<(String, &PerfResult)> =
        records.iter().map(|(l, r)| (l.clone(), r)).collect();
    Ok(compare(
        &labelled,
        CompareMode::Aligned,
        tolerance,
        floor,
        Vec::new(),
    ))
}

/// An interleaved comparison on one host.
#[derive(Debug, Clone)]
pub struct InterleavedRequest<'a> {
    /// The workload and the run's settings, shared by both sides.
    pub settings: WorkloadSettings<'a>,
    /// Side A's IXIT.
    pub ixit_a: &'a Path,
    /// Side B's IXIT; `None` for an A/A run, which puts A on both sides.
    pub ixit_b: Option<&'a Path>,
    /// The directory the two records, their corpora and the comparison are
    /// written to.
    pub out: &'a Path,
    /// The declared tolerance.
    pub tolerance: f64,
    /// A file carrying an A/A floor to attach to an A/B run.
    pub floor: Option<&'a Path>,
}

/// What an interleaved comparison wrote.
#[derive(Debug, Clone)]
pub struct InterleavedComparison {
    /// Side A's record.
    pub a: PerfResult,
    /// Side B's record.
    pub b: PerfResult,
    /// The comparison.
    pub comparison: PerfComparison,
}

fn attach(mut raw: Raw, interleaving: Interleaving, floor: Option<&AaFloor>) -> Raw {
    raw.interleaving = Some(interleaving);
    raw.aa_floor = floor.cloned();
    raw
}

/// The two sides' raw records, each with its target's base URL.
#[derive(Debug, Clone)]
pub struct Sides<'a> {
    /// Side A's raw record and base URL.
    pub a: (Raw, &'a str),
    /// Side B's.
    pub b: (Raw, &'a str),
}

/// Builds both records of an interleaved run and their comparison.
///
/// Each record carries the interleaving with the plateau order. An A/A
/// run's floor is taken from its own two sides and attached to both
/// records; an A/B run attaches `supplied_floor` when one is given.
///
/// # Errors
/// [`Error::Instrument`] when a record does not derive.
pub fn assemble(
    sides: Sides<'_>,
    outcome: &InterleavedOutcome,
    mode: InterleaveMode,
    tolerance: f64,
    supplied_floor: Option<AaFloor>,
) -> Result<InterleavedComparison, Error> {
    let Sides {
        a: (raw_a, url_a),
        b: (raw_b, url_b),
    } = sides;
    let interleaving = |side: SideName, partner: &str| Interleaving {
        mode,
        side,
        partner: partner.to_owned(),
        shared_estimate: outcome.shared_estimate,
        sequence: outcome.sequence.clone(),
    };
    let floor = match mode {
        InterleaveMode::Aa => {
            let a = PerfResult::from_raw(raw_a.clone()).map_err(instrument)?;
            let b = PerfResult::from_raw(raw_b.clone()).map_err(instrument)?;
            Some(floor_of(&a, &b))
        }
        InterleaveMode::Ab => supplied_floor,
    };
    let a = PerfResult::from_raw(attach(
        raw_a,
        interleaving(SideName::A, url_b),
        floor.as_ref(),
    ))
    .map_err(instrument)?;
    let b = PerfResult::from_raw(attach(
        raw_b,
        interleaving(SideName::B, url_a),
        floor.as_ref(),
    ))
    .map_err(instrument)?;
    let comparison = compare(
        &[("A".to_owned(), &a), ("B".to_owned(), &b)],
        match mode {
            InterleaveMode::Aa => CompareMode::Aa,
            InterleaveMode::Ab => CompareMode::Ab,
        },
        tolerance,
        floor,
        outcome.sequence.clone(),
    );
    Ok(InterleavedComparison { a, b, comparison })
}

fn ensure_dir(dir: &Path) -> Result<(), Error> {
    std::fs::create_dir_all(dir).map_err(|source| Error::Write {
        path: dir.to_owned(),
        source,
    })
}

/// Runs two targets, or one target against itself, interleaved, and
/// writes both records and the comparison.
///
/// # Errors
/// Every error of a single run on either side, and the errors of
/// [`read_floor`].
pub fn run_compare(
    request: &InterleavedRequest<'_>,
    progress: &(dyn Fn(String) + Sync),
) -> Result<InterleavedComparison, Error> {
    let supplied_floor = match request.floor {
        Some(path) => Some(read_floor(path)?),
        None => None,
    };
    ensure_dir(request.out)?;
    let settings = &request.settings;
    let prepared = prepare(settings, progress)?;
    let target_a = open_target(request.ixit_a)?;
    let target_b = match request.ixit_b {
        Some(path) => Some(open_target(path)?),
        None => None,
    };
    let sides = TargetSides {
        a: &target_a,
        b: target_b
            .as_ref()
            .map(|t| (t, Declared::plain(settings.posture))),
    };
    let InterleavedComparison { a, b, comparison } = compare_targets(
        &prepared,
        settings,
        &sides,
        request.out,
        request.tolerance,
        supplied_floor,
        progress,
    )?;
    write_record(&a, &request.out.join(A_RECORD))?;
    write_record(&b, &request.out.join(B_RECORD))?;
    write_comparison(&comparison, request.out)?;
    Ok(InterleavedComparison { a, b, comparison })
}

/// The two targets of an interleaved run: side A, and side B with the
/// posture profile it declares, or `None` for A on both sides.
pub(crate) struct TargetSides<'t> {
    pub(crate) a: &'t OpenTarget,
    pub(crate) b: Option<(&'t OpenTarget, Declared)>,
}

/// One side's canaries, built once its corpus is ready.
fn side_posture(
    prepared: &Prepared,
    declared: Declared,
    target: &OpenTarget,
    ready: &ReadyTarget,
) -> Result<Arc<PostureCheck>, Error> {
    Ok(Arc::new(PostureCheck::new(
        declared,
        target,
        prepared.twin.clone(),
        &ready.state,
    )?))
}

/// Drives two opened targets interleaved and builds both records and their
/// comparison; the corpora are stored under `out`.
#[expect(
    clippy::too_many_lines,
    reason = "one sequence: ready both sides, bracket, drive, bracket, record, compare"
)]
pub(crate) fn compare_targets(
    prepared: &Prepared,
    settings: &WorkloadSettings<'_>,
    sides: &TargetSides<'_>,
    out: &Path,
    tolerance: f64,
    supplied_floor: Option<AaFloor>,
    progress: &(dyn Fn(String) + Sync),
) -> Result<InterleavedComparison, Error> {
    let mode = if sides.b.is_some() {
        InterleaveMode::Ab
    } else {
        InterleaveMode::Aa
    };
    let target_a = sides.a;
    let state_a = out.join("a.corpus.json");
    let state_b = out.join("b.corpus.json");
    let runtime = runtime(prepared.workers)?;
    let (ready_a, ready_b, outcome, settings_a, settings_b, posture_a, posture_b) = runtime
        .block_on(async {
            let ready_a = ready_target(prepared, target_a, settings, &state_a, progress).await?;
            let ready_b = match &sides.b {
                Some((target, _)) => {
                    Some(ready_target(prepared, target, settings, &state_b, progress).await?)
                }
                None => None,
            };
            let check_a = side_posture(
                prepared,
                Declared::plain(settings.posture),
                target_a,
                &ready_a,
            )?;
            let check_b = match (&sides.b, &ready_b) {
                (Some((target, declared)), Some(ready)) => {
                    side_posture(prepared, declared.clone(), target, ready)?
                }
                _ => Arc::clone(&check_a),
            };
            let before_a = check_a.bracket(Bracket::Before).await?;
            let before_b = check_b.bracket(Bracket::Before).await?;
            let target_b = sides.b.as_ref().map_or(target_a, |(t, _)| *t);
            let (hooks_a, settings_a) = live_hooks(target_a, check_a, "A: ", progress).await;
            let (hooks_b, settings_b) = live_hooks(target_b, check_b, "B: ", progress).await;
            let performer_b = ready_b.as_ref().map_or_else(
                || Arc::clone(&ready_a.performer),
                |r| Arc::clone(&r.performer),
            );
            let run = RunRequest {
                seed: prepared.loaded.workload.seed,
                process: prepared.loaded.workload.arrival,
                binding: &prepared.binding,
                limits: prepared.limits.clone(),
                shape: prepared.shape.clone(),
            };
            progress(format!(
                "interleaved {}: each side settles, warms up and scouts, then four rungs run ABBA",
                match mode {
                    InterleaveMode::Aa => "A/A",
                    InterleaveMode::Ab => "A/B",
                }
            ));
            let outcome = run_interleaved(
                &run,
                (Arc::clone(&ready_a.performer), &hooks_a),
                (performer_b, &hooks_b),
            )
            .await
            .map_err(instrument)?;
            let posture_a = hooks_a.posture_record(&before_a)?;
            let posture_b = hooks_b.posture_record(&before_b)?;
            store_read_back(target_a, &ready_a.state, &state_a, progress).await?;
            if let (Some((target, _)), Some(ready)) = (&sides.b, &ready_b) {
                store_read_back(target, &ready.state, &state_b, progress).await?;
            }
            Ok::<_, Error>((
                ready_a, ready_b, outcome, settings_a, settings_b, posture_a, posture_b,
            ))
        })?;
    let target_b = sides.b.as_ref().map_or(target_a, |(t, _)| *t);
    let ready_b = ready_b.as_ref().unwrap_or(&ready_a);
    let raw_a = raw_record(
        prepared,
        settings,
        target_a,
        &ready_a,
        settings_a,
        outcome.a.clone(),
        posture_a,
    );
    let raw_b = raw_record(
        prepared,
        settings,
        target_b,
        ready_b,
        settings_b,
        outcome.b.clone(),
        posture_b,
    );
    refuse_rate_limited(&raw_a)?;
    refuse_rate_limited(&raw_b)?;
    assemble(
        Sides {
            a: (raw_a, &target_a.base_url),
            b: (raw_b, &target_b.base_url),
        },
        &outcome,
        mode,
        tolerance,
        supplied_floor,
    )
}

/// Runs the target against every pinned reference CDR, each composed on
/// this host from its own digest-pinned images, interleaved the same way.
///
/// For each reference the stack is composed, the target is side A and the
/// reference side B, the two records and their comparison are written under
/// `<out>/<reference>/`, and the stack is torn down with its volumes before
/// the next reference is composed. The ratios of each comparison are the
/// target's index relative to that reference: dimensionless, so they travel
/// between machines where milliseconds do not. A reference whose pinned
/// recipe configures a posture item differently declares the recipe's value,
/// and its record names each departure.
///
/// # Errors
/// Every error of [`run_compare`], and [`Error::Instrument`] when a
/// reference stack cannot be composed or torn down.
pub fn run_with_baselines(
    request: &InterleavedRequest<'_>,
    docker: &DockerCli,
    progress: &(dyn Fn(String) + Sync),
) -> Result<Vec<(ReferenceCdr, InterleavedComparison)>, Error> {
    let supplied_floor = match request.floor {
        Some(path) => Some(read_floor(path)?),
        None => None,
    };
    let version = docker.probe().map_err(instrument)?;
    progress(format!(
        "container runtime answers, server version {version}"
    ));
    let settings = &request.settings;
    let prepared = prepare(settings, progress)?;
    let target = open_target(request.ixit_a)?;
    let mut done = Vec::new();
    for cdr in ReferenceCdr::ALL.iter().copied() {
        let composed = compose(cdr, docker, progress).map_err(instrument)?;
        let (profile, divergences) = composed.pin.declaration(settings.posture);
        for divergence in &divergences {
            progress(format!(
                "baseline {cdr}: `{}` is declared `{}` by profile `{}` and configured `{}` by the \
                 pinned recipe, so this baseline declares the recipe's value",
                divergence.item,
                divergence.profile_declares,
                settings.posture.name,
                divergence.deployment_configures
            ));
        }
        let out = request.out.join(cdr.as_str());
        let measured = ensure_dir(&out)
            .and_then(|()| OpenTarget::baseline(composed.pin, target.environment.clone()))
            .and_then(|baseline| {
                let sides = TargetSides {
                    a: &target,
                    b: Some((
                        &baseline,
                        Declared {
                            profile,
                            divergences,
                        },
                    )),
                };
                compare_targets(
                    &prepared,
                    settings,
                    &sides,
                    &out,
                    request.tolerance,
                    supplied_floor.clone(),
                    progress,
                )
            });
        let torn_down = composed.tear_down(progress).map_err(instrument);
        let comparison = measured?;
        torn_down?;
        write_record(&comparison.a, &out.join(A_RECORD))?;
        write_record(&comparison.b, &out.join(B_RECORD))?;
        write_comparison(&comparison.comparison, &out)?;
        done.push((cdr, comparison));
    }
    Ok(done)
}

/// Writes the comparison and its Markdown view into `dir`.
///
/// # Errors
/// [`Error::Write`] when a file cannot be written, and
/// [`Error::Instrument`] when the comparison does not serialize.
pub fn write_comparison(comparison: &PerfComparison, dir: &Path) -> Result<(), Error> {
    ensure_dir(dir)?;
    let text = comparison.render().map_err(instrument)?;
    for (name, body) in [
        (COMPARISON_FILE, text),
        (COMPARISON_MARKDOWN, markdown(comparison)),
    ] {
        let path = dir.join(name);
        std::fs::write(&path, body).map_err(|source| Error::Write { path, source })?;
    }
    Ok(())
}
