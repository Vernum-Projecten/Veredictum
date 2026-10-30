// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The terminal and Markdown views of a `perf-result`.
//!
//! Both read the record alone, so one record always renders to the same
//! bytes. They print a header naming what was measured and what one pass
//! cannot show, one line per plateau with its qualification as a glyph as
//! well as a colour, the operations at the knee and at the lowest plateau
//! with the quantile each is judged at, the knee, peak, fit and population
//! sentences, a sparkline of p99 against the offered rate, and the
//! server-side readings as context. Every number is the record's derived
//! value, never recomputed. The terminal view fits 80 columns. Colour is
//! ANSI SGR (ECMA-48 §8.3.117), and whether to use it is the caller's
//! decision. No openEHR specification governs this: our own design.

use std::fmt::Write as _;

use crate::identity::IdentityRead;
use crate::perf_result::PerfResult;
use crate::perf_result::TargetOptions;
use crate::perf_result::capacity::{Knee, PeakSource, PopulationReading, UslReading};
use crate::perf_result::derive::{
    BetweenRunNoise, DerivedOperation, DerivedPlateau, JudgedAt, Latency, Steadiness,
};
use crate::perf_result::host::HostReading;
use crate::perf_result::quantile::{Quantile, QuantileName, u64_to_f64};
use crate::perf_result::server::{
    ContainerRole, ContainerSummary, ContainersReading, SettingsReading, StatementsReading,
};

/// The widest line the terminal view prints.
pub const WIDTH: usize = 80;

/// The cells of the p99 sparkline, lowest first.
const SPARK: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// The settings the views name, when the record carries them.
const SHOWN_SETTINGS: [&str; 5] = [
    "shared_buffers",
    "work_mem",
    "effective_cache_size",
    "max_connections",
    "jit",
];

/// How a not-resolvable quantile reads in a table cell.
const NOT_RESOLVABLE: &str = "n/r";

/// The legend line under every table that can hold [`NOT_RESOLVABLE`].
const NOT_RESOLVABLE_LEGEND: &str =
    "n/r: not resolvable, fewer than 10 samples lie above the quantile.";

/// Milliseconds as a table cell: one decimal below 100, whole above.
#[must_use]
pub fn format_ms(ms: f64) -> String {
    if ms < 100.0 {
        format!("{ms:.1}")
    } else {
        group(&format!("{ms:.0}"))
    }
}

/// An integer with thousands separators.
#[must_use]
pub fn format_count(n: u64) -> String {
    group(&n.to_string())
}

/// Puts thousands separators into the integer part of `digits`.
fn group(digits: &str) -> String {
    let (sign, rest) = digits
        .strip_prefix('-')
        .map_or(("", digits), |rest| ("-", rest));
    let (whole, fraction) = rest
        .split_once('.')
        .map_or((rest, None), |(w, f)| (w, Some(f)));
    let mut out = String::new();
    let len = whole.chars().count();
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    match fraction {
        Some(f) => format!("{sign}{out}.{f}"),
        None => format!("{sign}{out}"),
    }
}

/// A rate as a table cell, one decimal.
#[must_use]
pub fn format_rate(rate: f64) -> String {
    group(&format!("{rate:.1}"))
}

/// Bytes in decimal units, one decimal below 10.
#[must_use]
pub fn format_bytes(bytes: f64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    let negative = bytes < 0.0;
    let mut value = bytes.abs();
    let mut unit = 0;
    while value >= 1_000.0 && unit < UNITS.len() - 1 {
        value /= 1_000.0;
        unit += 1;
    }
    let text = if unit == 0 || value >= 10.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    };
    let sign = if negative { "-" } else { "" };
    format!("{sign}{text} {}", UNITS.get(unit).copied().unwrap_or("B"))
}

/// A share as a percentage, two decimals.
fn format_share(share: Option<f64>) -> String {
    share.map_or_else(|| "—".to_owned(), |s| format!("{:.2}%", s * 100.0))
}

/// A quantile as a table cell.
#[must_use]
pub fn quantile_cell(quantile: &Quantile) -> String {
    match quantile {
        Quantile::Resolved { value_ms, .. } => format_ms(*value_ms),
        Quantile::NotResolvable { .. } => NOT_RESOLVABLE.to_owned(),
    }
}

/// The token a judged quantile prints as.
#[must_use]
pub fn judged_token(judged: JudgedAt) -> &'static str {
    judged.quantile().map_or("none", QuantileName::token)
}

/// The operation a judged quantile names, with its value at that quantile.
fn judged_value(op: &DerivedOperation) -> Option<f64> {
    op.judged_at
        .quantile()
        .and_then(|q| op.latency.get(q).value_ms())
}

/// `text` cut to `width` characters, an ellipsis marking the cut.
fn clip(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Wraps `text` at word boundaries to `width`, each line after the first
/// indented by `indent` spaces.
fn wrap(text: &str, width: usize, first_prefix: &str, indent: usize) -> String {
    let mut out = String::new();
    let mut line = first_prefix.to_owned();
    let mut fresh = true;
    let pad = " ".repeat(indent);
    for word in text.split_whitespace() {
        let needed = line.chars().count() + usize::from(!fresh) + word.chars().count();
        if !fresh && needed > width {
            out.push_str(line.trim_end());
            out.push('\n');
            line.clone_from(&pad);
            fresh = true;
        }
        if !fresh {
            line.push(' ');
        }
        line.push_str(word);
        fresh = false;
    }
    out.push_str(line.trim_end());
    out.push('\n');
    out
}

/// One of the colours the terminal view uses.
#[derive(Debug, Clone, Copy)]
enum Paint {
    Green,
    Red,
    Yellow,
    Bold,
}

impl Paint {
    const fn sgr(self) -> &'static str {
        match self {
            Self::Green => "32",
            Self::Red => "31",
            Self::Yellow => "33",
            Self::Bold => "1",
        }
    }
}

fn paint(text: &str, paint: Paint, color: bool) -> String {
    if color {
        format!("\u{1b}[{}m{text}\u{1b}[0m", paint.sgr())
    } else {
        text.to_owned()
    }
}

/// What the server said about itself, as one phrase.
fn target_phrase(record: &PerfResult) -> String {
    let base = &record.raw.target.base_url;
    match &record.raw.target.options {
        TargetOptions::Answered {
            answer: IdentityRead::Offered { options, .. },
        } => {
            let name = options.solution.as_deref().unwrap_or("unnamed solution");
            match options.solution_version.as_deref() {
                Some(version) => format!("{base} ({name} {version})"),
                None => format!("{base} ({name})"),
            }
        }
        TargetOptions::Answered { .. } => format!("{base} (no Options document)"),
        TargetOptions::Unanswered { .. } => format!("{base} (System OPTIONS unanswered)"),
    }
}

fn environment_phrase(record: &PerfResult) -> String {
    let e = &record.raw.environment;
    format!(
        "{}, {} cores, {} GiB, {} storage, {}",
        e.hardware_class, e.cores, e.memory_gb, e.storage_class, e.topology
    )
}

fn host_phrase(record: &PerfResult) -> String {
    let host = &record.raw.generator.host;
    let prescription = match &record.derived.host {
        HostReading::Unclaimed => "no prescription claimed".to_owned(),
        HostReading::Matched { prescription } => format!("prescription {prescription}, matched"),
        HostReading::Unmatched { prescription, .. } => {
            format!("prescription {prescription}, UNMATCHED")
        }
        HostReading::Unverifiable { prescription, .. } => {
            format!("prescription {prescription}, unverifiable")
        }
    };
    let mut generator = format!("{} {}", host.arch, host.os);
    if let Some(n) = host.available_parallelism {
        let _ = write!(generator, ", {n} threads");
    }
    if let Some(cpu) = &host.cpu_model {
        let _ = write!(generator, ", {cpu}");
    }
    format!("{prescription}; generator on {generator}")
}

fn workload_phrase(record: &PerfResult) -> String {
    let w = &record.raw.workload;
    if w.label == "custom" {
        format!(
            "{} v{}, custom: a document the instrument does not pin",
            w.id, w.version
        )
    } else {
        format!("{} ({})", w.id, w.label)
    }
}

fn digest_phrase(digest: &str) -> String {
    clip(digest, 30)
}

fn corpus_phrase(record: &PerfResult) -> String {
    let c = &record.raw.corpus;
    let origin = match c.origin {
        crate::perf_result::CorpusOrigin::Seeded => "seeded by this run",
        crate::perf_result::CorpusOrigin::Reused => "reused",
    };
    format!(
        "scale {}: {} EHRs, {} compositions, {origin}",
        c.scale.token(),
        format_count(c.ehrs),
        format_count(c.compositions)
    )
}

fn noise_phrase(record: &PerfResult) -> String {
    match &record.derived.between_run_noise {
        BetweenRunNoise::Unmeasured => {
            "1 pass; between-run noise unmeasured, no A/A floor attached".to_owned()
        }
        BetweenRunNoise::AaFloor(floor) => format!(
            "1 pass; A/A floor attached, p99 within {:.3}x",
            floor.of(crate::perf_result::compare::CompareMetric::P99)
        ),
    }
}

fn estimate_phrase(record: &PerfResult) -> String {
    match &record.derived.scout_estimate {
        Some(e) => match e.breached_window {
            Some(w) => format!(
                "knee estimate {}/s, from scout window {w}",
                format_rate(e.rate)
            ),
            None => format!(
                "knee estimate {}/s, a lower bound: no scout window breached",
                format_rate(e.rate)
            ),
        },
        None => "no scout".to_owned(),
    }
}

/// The header lines as label and value, in print order.
fn header(record: &PerfResult) -> Vec<(&'static str, String)> {
    vec![
        ("target", target_phrase(record)),
        ("environment", environment_phrase(record)),
        ("host", host_phrase(record)),
        ("workload", workload_phrase(record)),
        ("digest", digest_phrase(&record.raw.workload.sha256)),
        ("corpus", corpus_phrase(record)),
        ("scout", estimate_phrase(record)),
        ("noise", noise_phrase(record)),
        ("scope", record.derived.scope.clone()),
    ]
}

/// The plateau the knee was read at, and the lowest one.
fn knee_and_lowest(record: &PerfResult) -> (Option<&DerivedPlateau>, Option<&DerivedPlateau>) {
    let plateaus = &record.derived.plateaus;
    let at = |rate: f64| {
        plateaus
            .iter()
            .find(|p| p.planned_rate.total_cmp(&rate).is_eq())
    };
    let knee = match &record.derived.knee {
        Knee::Bracket { qualified_rate, .. } | Knee::LowerBound { qualified_rate, .. } => {
            at(*qualified_rate)
        }
        Knee::BelowLadder { .. } | Knee::NoPlateau => None,
    };
    let lowest = plateaus
        .iter()
        .min_by(|a, b| a.planned_rate.total_cmp(&b.planned_rate));
    (knee, lowest)
}

/// The knee sentence.
#[must_use]
pub fn knee_sentence(knee: &Knee) -> String {
    match knee {
        Knee::Bracket {
            qualified_rate,
            achieved_per_s,
            failing_rate,
        } => format!(
            "Knee: between {}/s offered, the highest plateau that qualifies with every one \
             below it (achieved {}/s), and {}/s, the first plateau that fails.",
            format_rate(*qualified_rate),
            format_rate(*achieved_per_s),
            format_rate(*failing_rate)
        ),
        Knee::LowerBound {
            qualified_rate,
            achieved_per_s,
        } => format!(
            "Knee: at least {}/s offered (achieved {}/s); every plateau qualifies, so the \
             knee lies above the ladder.",
            format_rate(*qualified_rate),
            format_rate(*achieved_per_s)
        ),
        Knee::BelowLadder { failing_rate } => format!(
            "Knee: below {}/s offered; the lowest plateau fails.",
            format_rate(*failing_rate)
        ),
        Knee::NoPlateau => "Knee: none; the run has no plateau.".to_owned(),
    }
}

fn peak_sentence(record: &PerfResult) -> String {
    match &record.derived.peak {
        Some(p) => {
            let place = match p.source {
                PeakSource::Plateau => format!("plateau {}", p.index),
                PeakSource::Scout => format!("scout window {}", p.index),
            };
            format!(
                "Peak: {}/s achieved, in {place}, whatever its latency.",
                format_rate(p.achieved_per_s)
            )
        }
        None => "Peak: none; no exchange succeeded.".to_owned(),
    }
}

fn usl_sentence(record: &PerfResult) -> String {
    match &record.derived.usl {
        UslReading::Fitted {
            alpha,
            beta,
            r_squared,
            n_star,
            predicted_max_per_s,
            points,
            ..
        } => {
            let maximum = match (n_star, predicted_max_per_s) {
                (Some(n), Some(x)) => format!(
                    "a predicted maximum of {}/s at {:.1} in flight",
                    format_rate(*x),
                    n
                ),
                _ => "no finite maximum".to_owned(),
            };
            format!(
                "Scalability fit (an explanatory model, never a verdict): \u{3b1} {alpha:.4}, \
                 \u{3b2} {beta:.6}, R\u{b2} {r_squared:.3} over {points} points; {maximum}."
            )
        }
        UslReading::NotFitted { points, reason } => {
            format!("Scalability fit: none over {points} points; {reason}.")
        }
    }
}

fn population_sentence(record: &PerfResult) -> String {
    match &record.derived.population {
        PopulationReading::Read {
            lower_bound,
            arrivals_per_100k,
            population,
            corpus_scale,
            corpus_ehrs,
            environment,
            ..
        } => {
            let [low, high] = population;
            let [a_low, a_high] = arrivals_per_100k;
            let bound = if *lower_bound { "at least " } else { "" };
            format!(
                "Population: {bound}{} to {} people at their peak, at {a_low:.0} to \
                 {a_high:.0} peak arrivals/s per 100,000, read only at corpus scale {} \
                 ({} EHRs) on {environment}.",
                format_count(round(*low)),
                format_count(round(*high)),
                corpus_scale.token(),
                format_count(*corpus_ehrs)
            )
        }
        PopulationReading::NotRead { reason } => format!("Population: not read; {reason}."),
    }
}

/// `value` rounded to a whole count, clamped to `[0, 2^53]`, where the
/// conversion is exact.
fn round(value: f64) -> u64 {
    let clamped = if value.is_nan() {
        0.0
    } else {
        value.round().clamp(0.0, 9_007_199_254_740_992.0)
    };
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to [0, 2^53] and rounded, so the integral value converts exactly"
    )]
    let whole = clamped as u64;
    whole
}

/// The p99 sparkline over the scout windows and the plateaus, by offered
/// rate: one cell per point on a log scale, `·` for a p99 that does not
/// resolve.
#[must_use]
pub fn sparkline(record: &PerfResult) -> (String, f64, f64) {
    let mut points: Vec<(f64, Option<f64>)> = record
        .derived
        .scout
        .iter()
        .map(|w| (w.planned_rate, w.latency.p99.value_ms()))
        .chain(
            record
                .derived
                .plateaus
                .iter()
                .map(|p| (p.planned_rate, p.latency.p99.value_ms())),
        )
        .collect();
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    let values: Vec<f64> = points
        .iter()
        .filter_map(|(_, v)| *v)
        .filter(|v| *v > 0.0)
        .collect();
    let low = values.iter().copied().reduce(f64::min).unwrap_or(1.0);
    let high = values.iter().copied().reduce(f64::max).unwrap_or(1.0);
    let span = (high.ln() - low.ln()).max(f64::EPSILON);
    let cells = points
        .iter()
        .map(|(_, v)| match v {
            Some(v) if *v > 0.0 => {
                let at = round((v.ln() - low.ln()) / span * 7.0);
                SPARK
                    .get(usize::try_from(at).unwrap_or(7))
                    .copied()
                    .unwrap_or('█')
            }
            _ => '·',
        })
        .collect();
    let first = points.first().map_or(0.0, |p| p.0);
    let last = points.last().map_or(0.0, |p| p.0);
    (cells, first, last)
}

/// A container summary as one phrase: CPU, memory and I/O.
fn container_phrase(c: &ContainerSummary) -> String {
    let cpu = match (c.cpu_mean_pct, c.cpu_max_pct) {
        (Some(mean), Some(max)) => format!("cpu {mean:.0}% (max {max:.0}%)"),
        _ => "cpu —".to_owned(),
    };
    let rss = c.rss_max_bytes.map_or_else(
        || "rss —".to_owned(),
        |b| format!("rss {}", format_bytes(u64_to_f64(b))),
    );
    let cut = if c.truncated { ", truncated" } else { "" };
    format!("{cpu}, {rss}{cut}")
}

fn role_name(role: ContainerRole) -> &'static str {
    match role {
        ContainerRole::Sut => "sut",
        ContainerRole::Db => "db",
    }
}

fn settings_phrase(record: &PerfResult) -> String {
    match &record.raw.server_settings {
        SettingsReading::Present {
            server_version,
            settings,
        } => {
            let named: Vec<String> = SHOWN_SETTINGS
                .iter()
                .filter_map(|name| {
                    settings.get(*name).map(|s| match &s.unit {
                        Some(unit) => format!("{name} {} {unit}", s.setting),
                        None => format!("{name} {}", s.setting),
                    })
                })
                .collect();
            if named.is_empty() {
                format!("PostgreSQL {server_version}")
            } else {
                format!("PostgreSQL {server_version}; {}", named.join(", "))
            }
        }
        SettingsReading::Absent { reason } => format!("absent: {reason}"),
    }
}

/// What a plateau's container reading is, when it is not a series.
fn containers_absence(record: &PerfResult, index: usize) -> Option<&str> {
    match record.raw.plateaus.get(index).map(|p| &p.server.containers) {
        Some(ContainersReading::Absent { reason }) => Some(reason.as_str()),
        _ => None,
    }
}

fn statements_of(record: &PerfResult, plateau: &DerivedPlateau) -> Result<Vec<String>, String> {
    let raw = record.raw.plateaus.iter().find(|p| {
        p.planned_rate
            .per_second()
            .total_cmp(&plateau.planned_rate)
            .is_eq()
    });
    match raw.map(|p| &p.server.statements) {
        Some(StatementsReading::Present { statements }) => Ok(statements
            .iter()
            .take(3)
            .map(|s| {
                format!(
                    "{} ms, {} calls: {}",
                    format_ms(s.total_ms()),
                    format_count(s.calls),
                    s.query
                )
            })
            .collect()),
        Some(StatementsReading::Absent { reason }) => Err(reason.clone()),
        None => Err("no raw plateau at this rate".to_owned()),
    }
}

/// The plateau mark: a glyph that reads without colour.
fn mark(p: &DerivedPlateau) -> (&'static str, Paint) {
    if p.qualification.qualified {
        ("✓", Paint::Green)
    } else {
        ("✕", Paint::Red)
    }
}

fn latency_cells(l: &Latency) -> [String; 5] {
    [
        quantile_cell(&l.p50),
        quantile_cell(&l.p95),
        quantile_cell(&l.p99),
        quantile_cell(&l.p99_9),
        l.max_ms.map_or_else(|| "—".to_owned(), format_ms),
    ]
}

/// The operations judged below p99, as one sentence each.
fn below_p99(plateau: &DerivedPlateau) -> Vec<String> {
    plateau
        .operations
        .iter()
        .filter(|(_, op)| op.judged_at != JudgedAt::P99)
        .map(|(id, op)| match op.judged_at.quantile() {
            Some(q) => format!(
                "{id} is judged at {}: its {} samples resolve no higher quantile.",
                q.token(),
                format_count(op.succeeded)
            ),
            None => format!(
                "{id} is judged at no quantile: its {} samples resolve none.",
                format_count(op.succeeded)
            ),
        })
        .collect()
}

/// The operation tables a view prints: at the knee, and at the lowest
/// plateau when that is another one.
fn tables(record: &PerfResult) -> Vec<(&'static str, &DerivedPlateau)> {
    let (knee, lowest) = knee_and_lowest(record);
    let mut out: Vec<(&'static str, &DerivedPlateau)> = Vec::new();
    if let Some(k) = knee {
        out.push(("at the knee", k));
    }
    if let Some(l) = lowest
        && knee.is_none_or(|k| !std::ptr::eq(k, l))
    {
        out.push(("at the lowest plateau", l));
    }
    out
}

/// The capacity sentences, in print order.
fn sentences(record: &PerfResult) -> [String; 4] {
    [
        knee_sentence(&record.derived.knee),
        peak_sentence(record),
        population_sentence(record),
        usl_sentence(record),
    ]
}

/// The server-side lines of one plateau: a line per container, then the
/// database growth, or the reason there is no reading.
fn server_lines(record: &PerfResult, index: usize, p: &DerivedPlateau) -> Vec<String> {
    let mut lines: Vec<String> = p
        .server
        .containers
        .iter()
        .map(|c| format!("{} {}", role_name(c.role), container_phrase(c)))
        .collect();
    if let Some(g) = p.server.database_growth_bytes {
        lines.push(format!("database {}", Growth(g)));
    }
    if lines.is_empty() {
        let reason = containers_absence(record, index).unwrap_or("no reading");
        lines.push(format!("absent: {reason}"));
    }
    lines
}

/// The terminal view under construction.
struct Terminal<'r> {
    record: &'r PerfResult,
    color: bool,
    out: String,
}

impl Terminal<'_> {
    fn line(&mut self, text: &str) {
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn heading(&mut self, text: &str) {
        let painted = paint(text, Paint::Bold, self.color);
        self.out.push('\n');
        self.line(&painted);
    }

    fn wrapped(&mut self, text: &str, prefix: &str, indent: usize) {
        self.out.push_str(&wrap(text, WIDTH, prefix, indent));
    }

    fn header(&mut self) {
        let title = format!(
            "perf-result: {} against {}",
            self.record.raw.workload.id, self.record.raw.target.base_url
        );
        let painted = paint(&clip(&title, WIDTH), Paint::Bold, self.color);
        self.line(&painted);
        self.out.push('\n');
        for (label, value) in header(self.record) {
            self.wrapped(&value, &format!("{label:<12} "), 13);
        }
    }

    fn plateaus(&mut self) {
        self.heading("Plateaus (operations/s; latency ms)");
        self.line(&format!(
            "  {:>9} {:>9} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7}",
            "offered", "achieved", "p50", "p95", "p99", "p99.9", "max", "errors"
        ));
        for p in &self.record.derived.plateaus {
            let (glyph, tint) = mark(p);
            let [p50, p95, p99, p999, max] = latency_cells(&p.latency);
            let mut line = format!(
                "{} {:>9} {:>9} {p50:>7} {p95:>7} {p99:>7} {p999:>7} {max:>7} {:>7}",
                paint(glyph, tint, self.color),
                format_rate(p.planned_rate),
                format_rate(p.throughput.achieved_per_s),
                format_share(p.throughput.error_share),
            );
            if p.halves.steadiness == Steadiness::Unsteady {
                line.push(' ');
                line.push_str(&paint("unsteady", Paint::Yellow, self.color));
            }
            self.line(&line);
        }
        self.wrapped(
            "✓ qualifies, ✕ fails; unsteady: the second half's p50 or p95 over the first's \
             lies outside 0.8 to 1.25.",
            "",
            0,
        );
        self.wrapped(NOT_RESOLVABLE_LEGEND, "", 0);
    }

    fn operations(&mut self) {
        for (place, p) in tables(self.record) {
            self.heading(&format!(
                "Operations {place}, {}/s offered (ms)",
                format_rate(p.planned_rate)
            ));
            self.line(&format!(
                "  {:<22} {:>6} {:>7} {:>7} {:>7} {:>6} {:>7} {:>7}",
                "operation", "share", "p50", "p95", "p99", "judged", "value", "budget"
            ));
            for (id, op) in &p.operations {
                let value = judged_value(op);
                let glyph = if value.is_some_and(|v| v <= f64::from(op.budget_ms)) {
                    paint("✓", Paint::Green, self.color)
                } else {
                    paint("✕", Paint::Red, self.color)
                };
                self.line(&format!(
                    "{glyph} {:<22} {:>5.1}% {:>7} {:>7} {:>7} {:>6} {:>7} {:>7}",
                    clip(id.as_str(), 22),
                    op.share * 100.0,
                    quantile_cell(&op.latency.p50),
                    quantile_cell(&op.latency.p95),
                    quantile_cell(&op.latency.p99),
                    judged_token(op.judged_at),
                    value.map_or_else(|| "—".to_owned(), format_ms),
                    format_count(u64::from(op.budget_ms)),
                ));
            }
            for sentence in below_p99(p) {
                self.wrapped(&sentence, "  ", 2);
            }
        }
    }

    fn capacity(&mut self) {
        self.out.push('\n');
        for sentence in sentences(self.record) {
            self.wrapped(&sentence, "", 0);
        }
        let (cells, first, last) = sparkline(self.record);
        if !cells.is_empty() {
            self.heading("p99 by offered rate");
            let line = format!("{}/s {cells} {}/s", format_rate(first), format_rate(last));
            self.line(&format!("  {}", clip(&line, WIDTH - 2)));
            self.line("  scout windows and plateaus, log scale; · where p99 does not resolve");
        }
    }

    fn server(&mut self) {
        self.heading("Server readings (context, never part of qualification)");
        let settings = settings_phrase(self.record);
        self.wrapped(&settings, "  settings ", 11);
        for (index, p) in self.record.derived.plateaus.iter().enumerate() {
            for (i, line) in server_lines(self.record, index, p).iter().enumerate() {
                let lead = if i == 0 {
                    format!("  {:>9}/s  ", format_rate(p.planned_rate))
                } else {
                    " ".repeat(15)
                };
                self.wrapped(line, &lead, 15);
            }
        }
        let (knee, _) = knee_and_lowest(self.record);
        match knee.map(|k| statements_of(self.record, k)) {
            Some(Ok(lines)) if !lines.is_empty() => {
                self.line("  costliest statements at the knee:");
                for line in lines {
                    self.line(&format!("    {}", clip(&line, WIDTH - 4)));
                }
            }
            Some(Err(reason)) => {
                self.wrapped(
                    &format!("statements at the knee: absent: {reason}"),
                    "  ",
                    4,
                );
            }
            Some(Ok(_)) | None => {}
        }
    }
}

/// Renders the terminal view of `record`.
#[must_use]
pub fn terminal(record: &PerfResult, color: bool) -> String {
    let mut view = Terminal {
        record,
        color,
        out: String::new(),
    };
    view.header();
    view.plateaus();
    view.operations();
    view.capacity();
    view.server();
    view.out
}

/// A signed byte growth, printed with its sign.
struct Growth(i64);

impl std::fmt::Display for Growth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bytes = format_bytes(i64_to_f64(self.0));
        if self.0 >= 0 {
            write!(f, "+{bytes}")
        } else {
            write!(f, "{bytes}")
        }
    }
}

fn i64_to_f64(value: i64) -> f64 {
    let magnitude = u64_to_f64(value.unsigned_abs());
    if value < 0 { -magnitude } else { magnitude }
}

/// Escapes the characters Markdown table cells give meaning to.
fn md(text: &str) -> String {
    text.replace('|', "\\|").replace('`', "'")
}

fn md_header(record: &PerfResult, out: &mut String) {
    let _ = writeln!(
        out,
        "## Performance: {} against {}\n",
        md(&record.raw.workload.id),
        md(&record.raw.target.base_url)
    );
    let _ = writeln!(out, "| | |\n|---|---|");
    for (label, value) in header(record) {
        let value = if label == "digest" {
            format!("`{}`", record.raw.workload.sha256)
        } else {
            md(&value)
        };
        let _ = writeln!(out, "| {label} | {value} |");
    }
}

fn md_plateaus(record: &PerfResult, out: &mut String) {
    let _ = writeln!(
        out,
        "\n### Plateaus\n\n| | offered/s | achieved/s | p50 ms | p95 ms | p99 ms | p99.9 ms | \
         max ms | errors | steadiness |\n|---|--:|--:|--:|--:|--:|--:|--:|--:|---|"
    );
    for p in &record.derived.plateaus {
        let (glyph, _) = mark(p);
        let [p50, p95, p99, p999, max] = latency_cells(&p.latency);
        let steadiness = match p.halves.steadiness {
            Steadiness::Steady => "steady",
            Steadiness::Unsteady => "**unsteady**",
            Steadiness::NotMeasurable => "not measurable",
        };
        let _ = writeln!(
            out,
            "| {glyph} | {} | {} | {p50} | {p95} | {p99} | {p999} | {max} | {} | {steadiness} |",
            format_rate(p.planned_rate),
            format_rate(p.throughput.achieved_per_s),
            format_share(p.throughput.error_share),
        );
    }
    let _ = writeln!(
        out,
        "\n✓ qualifies, ✕ fails. {NOT_RESOLVABLE_LEGEND} Unsteady: the second half's p50 or p95 \
         over the first's lies outside 0.8 to 1.25."
    );
}

fn md_operations(record: &PerfResult, out: &mut String) {
    for (place, p) in tables(record) {
        let _ = writeln!(
            out,
            "\n### Operations {place}, {}/s offered\n\n| | operation | share | requests | p50 ms \
             | p95 ms | p99 ms | p99.9 ms | max ms | judged at | value ms | budget ms |\n\
             |---|---|--:|--:|--:|--:|--:|--:|--:|---|--:|--:|",
            format_rate(p.planned_rate)
        );
        for (id, op) in &p.operations {
            let [p50, p95, p99, p999, max] = latency_cells(&op.latency);
            let value = judged_value(op);
            let glyph = if value.is_some_and(|v| v <= f64::from(op.budget_ms)) {
                "✓"
            } else {
                "✕"
            };
            let _ = writeln!(
                out,
                "| {glyph} | `{id}` | {:.1}% | {} | {p50} | {p95} | {p99} | {p999} | {max} | {} \
                 | {} | {} |",
                op.share * 100.0,
                format_count(op.requests),
                judged_token(op.judged_at),
                value.map_or_else(|| "—".to_owned(), format_ms),
                format_count(u64::from(op.budget_ms)),
            );
        }
        let below = below_p99(p);
        if !below.is_empty() {
            out.push('\n');
            for sentence in below {
                let _ = writeln!(out, "- {}", md(&sentence));
            }
        }
    }
}

fn md_capacity(record: &PerfResult, out: &mut String) {
    let _ = writeln!(out, "\n### Capacity\n");
    for sentence in sentences(record) {
        let _ = writeln!(out, "- {}", md(&sentence));
    }
    if let PopulationReading::Read { derivation, .. } = &record.derived.population {
        let _ = writeln!(out, "\nThe population band rests on: {}", md(derivation));
    }
    let (cells, first, last) = sparkline(record);
    if !cells.is_empty() {
        let _ = writeln!(
            out,
            "\np99 by offered rate over the scout windows and the plateaus, {}/s to {}/s, on a \
             log scale, `·` where p99 does not resolve:\n\n```text\n{cells}\n```",
            format_rate(first),
            format_rate(last)
        );
    }
}

fn md_server(record: &PerfResult, out: &mut String) {
    let _ = writeln!(
        out,
        "\n### Server readings\n\nContext only: no reading bears on a plateau's \
         qualification.\n\n- Database settings: {}",
        md(&settings_phrase(record))
    );
    let _ = writeln!(
        out,
        "\n| offered/s | containers | database growth |\n|--:|---|--:|"
    );
    for (index, p) in record.derived.plateaus.iter().enumerate() {
        let lines = server_lines(record, index, p);
        let (containers, growth): (Vec<&String>, Vec<&String>) =
            lines.iter().partition(|l| !l.starts_with("database "));
        let growth = growth
            .first()
            .and_then(|g| g.strip_prefix("database "))
            .unwrap_or("—");
        let containers = containers
            .iter()
            .map(|c| md(c))
            .collect::<Vec<_>>()
            .join("; ");
        let _ = writeln!(
            out,
            "| {} | {containers} | {growth} |",
            format_rate(p.planned_rate)
        );
    }
    let (knee, _) = knee_and_lowest(record);
    match knee.map(|k| statements_of(record, k)) {
        Some(Ok(lines)) if !lines.is_empty() => {
            let _ = writeln!(out, "\nCostliest statements at the knee:\n");
            for line in lines {
                let _ = writeln!(out, "- {}", md(&line));
            }
        }
        Some(Err(reason)) => {
            let _ = writeln!(out, "\nStatements at the knee: absent: {}", md(&reason));
        }
        Some(Ok(_)) | None => {}
    }
}

/// Renders the Markdown view of `record`.
#[must_use]
pub fn markdown(record: &PerfResult) -> String {
    let mut out = String::new();
    md_header(record, &mut out);
    md_plateaus(record, &mut out);
    md_operations(record, &mut out);
    md_capacity(record, &mut out);
    md_server(record, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_group_their_thousands_and_keep_their_sign() {
        assert_eq!(format_count(1_234_567), "1,234,567");
        assert_eq!(format_ms(1_234.4), "1,234");
        assert_eq!(format_ms(12.34), "12.3");
        assert_eq!(format_rate(1_000.05), "1,000.0");
        assert_eq!(format_bytes(95_232_000.0), "95 MB");
        assert_eq!(format_bytes(-1_500.0), "-1.5 kB");
        assert_eq!(Growth(-1_500).to_string(), "-1.5 kB");
        assert_eq!(Growth(2_000_000).to_string(), "+2.0 MB");
    }

    #[test]
    fn wrapping_keeps_every_line_inside_the_width() {
        let text = "a ".repeat(100);
        let wrapped = wrap(&text, 20, "label ", 6);
        assert!(
            wrapped.lines().all(|l| l.chars().count() <= 20),
            "{wrapped}"
        );
        assert!(wrapped.starts_with("label a"));
        assert_eq!(clip("abcdef", 4), "abc…");
    }
}
