// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The charts of a `perf-result`, drawn as SVG from the record alone.
//!
//! Three charts: the saturation curve (p50, p95 and p99 against achieved
//! throughput over the scout windows and the plateaus, the knee bracket
//! marked, and the Universal Scalability Law fit as a dashed curve of mean
//! latency by Little's law); achieved against offered throughput with the
//! identity line; and each operation's latency at the knee at the quantile
//! it is judged at, beside its budget. Every number is the record's derived
//! value, coordinates are printed at fixed precision, and nothing depends on
//! the clock, so one record always draws the same bytes. A quantile that
//! does not resolve is drawn as its own mark below the plot and never
//! joined to its neighbours. Scout windows are hollow circles on thin lines
//! and plateaus filled squares on thick ones. The colours follow
//! `prefers-color-scheme` for light and dark
//! (<https://www.w3.org/TR/mediaqueries-5/#prefers-color-scheme>). No openEHR
//! specification governs this: our own design.

use std::fmt::Write as _;

use crate::perf_result::PerfResult;
use crate::perf_result::capacity::{Knee, UslReading};
use crate::perf_result::derive::{DerivedPlateau, Latency};
use crate::perf_result::quantile::{Quantile, QuantileName};
use crate::perf_result::view::{format_ms, format_rate, judged_token, knee_sentence};

/// One drawn file: its name and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chart {
    /// The file name the chart is published under.
    pub name: &'static str,
    /// The SVG text.
    pub svg: String,
}

/// The shared style: a sequential blue ramp for p50, p95 and p99, a neutral
/// for the scout and the fit, green and red kept for qualification, and a
/// dark-mode restyle of the same roles.
const STYLE: &str = "<style>\n\
  text { fill: #52514e; font: 12px -apple-system, 'Segoe UI', Helvetica, Arial, sans-serif; }\n\
  .title { fill: #0b0b0b; font-weight: 600; }\n\
  .muted { fill: #8a8880; font-size: 11px; }\n\
  .grid { stroke: #e4e2dd; stroke-width: 1; }\n\
  .axis { stroke: #8a8880; stroke-width: 1; }\n\
  .p50 { stroke: #7db4ea; fill: #7db4ea; } .p95 { stroke: #3d7fd0; fill: #3d7fd0; }\n\
  .p99 { stroke: #1b4f94; fill: #1b4f94; }\n\
  .line { fill: none; stroke-width: 2.2; }\n\
  .scoutline { fill: none; stroke-width: 1; stroke-opacity: 0.55; }\n\
  .hollow { fill: #ffffff; stroke-width: 1.4; }\n\
  .nr { fill: none; stroke-width: 1.4; }\n\
  .fit { fill: none; stroke: #6b6a64; stroke-width: 1.8; stroke-dasharray: 7 4; }\n\
  .identity { stroke: #8a8880; stroke-width: 1.2; stroke-dasharray: 3 3; }\n\
  .bracket { fill: #1baf7a; fill-opacity: 0.12; stroke: #1baf7a; stroke-width: 1; }\n\
  .ok { fill: #1baf7a; stroke: #1baf7a; } .bad { fill: #b3261e; stroke: #b3261e; }\n\
  .okt { fill: #1baf7a; } .badt { fill: #b3261e; }\n\
  .bar { fill: #3d7fd0; }\n\
  .budget { stroke: #b3261e; stroke-width: 2; }\n\
  @media (prefers-color-scheme: dark) {\n\
    text { fill: #c3c2b7; }\n\
    .title { fill: #ffffff; }\n\
    .muted { fill: #8f8e85; }\n\
    .grid { stroke: #3a3a38; }\n\
    .axis { stroke: #8f8e85; }\n\
    .p50 { stroke: #3d5f82; fill: #3d5f82; } .p95 { stroke: #3987e5; fill: #3987e5; }\n\
    .p99 { stroke: #9ec7ef; fill: #9ec7ef; }\n\
    .hollow { fill: #1d1d1b; }\n\
    .fit { stroke: #a8a79e; }\n\
    .identity { stroke: #8f8e85; }\n\
    .bracket { fill: #26c78d; stroke: #26c78d; }\n\
    .ok { fill: #26c78d; stroke: #26c78d; } .bad { fill: #e5484d; stroke: #e5484d; }\n\
    .okt { fill: #26c78d; } .badt { fill: #e5484d; }\n\
    .bar { fill: #3987e5; }\n\
    .budget { stroke: #e5484d; }\n\
  }\n\
</style>\n";

/// The quantiles the saturation chart draws, with their class.
const DRAWN: [(QuantileName, &str); 3] = [
    (QuantileName::P50, "p50"),
    (QuantileName::P95, "p95"),
    (QuantileName::P99, "p99"),
];

/// The plot area every chart shares: left, right, top and bottom.
const X0: f64 = 84.0;
const X1: f64 = 720.0;
const Y_TOP: f64 = 92.0;
const Y_BOTTOM: f64 = 352.0;

/// The strip below the plot where a not-resolvable quantile is marked.
const NR_STRIP: f64 = Y_BOTTOM + 14.0;

/// The points of the fitted curve.
const FIT_POINTS: u32 = 80;

/// Escapes text for SVG character data.
fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn open(out: &mut String, width: f64, height: f64, title: &str, desc: &str) {
    let _ = write!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width} {height}\" \
         width=\"{width}\" height=\"{height}\" role=\"img\">\n<title>{}</title>\n<desc>{}</desc>\n{STYLE}",
        esc(title),
        esc(desc)
    );
}

/// The smallest of 1, 1.2, 1.6, 2, 2.4, 3, 4, 5, 6 or 8 times a power of ten
/// at or above `v`: each divides into four ticks of round values.
fn nice_ceil(v: f64) -> f64 {
    if !(v.is_finite() && v > 0.0) {
        return 1.0;
    }
    let magnitude = 10.0_f64.powf(v.log10().floor());
    [1.0, 1.2, 1.6, 2.0, 2.4, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0]
        .into_iter()
        .map(|step| step * magnitude)
        .find(|candidate| *candidate >= v)
        .unwrap_or(10.0 * magnitude)
}

/// A linear axis from zero to `max`.
#[derive(Debug, Clone, Copy)]
struct Linear {
    max: f64,
    from: f64,
    to: f64,
}

impl Linear {
    fn at(self, v: f64) -> f64 {
        self.from + v.clamp(0.0, self.max) / self.max * (self.to - self.from)
    }

    /// Five ticks from zero to the maximum.
    fn ticks(self) -> Vec<f64> {
        (0..=4).map(|i| self.max * f64::from(i) / 4.0).collect()
    }
}

/// A decade-log axis from `10^low` to `10^high`.
#[derive(Debug, Clone, Copy)]
struct Log {
    low: i32,
    high: i32,
    from: f64,
    to: f64,
}

impl Log {
    /// The axis spanning every value in `values`, at least one decade.
    fn spanning(values: &[f64], from: f64, to: f64) -> Self {
        let positive = values.iter().copied().filter(|v| *v > 0.0);
        let low = positive.clone().reduce(f64::min).unwrap_or(1.0);
        let high = positive.reduce(f64::max).unwrap_or(10.0);
        let low = decade(low.log10().floor());
        let high = decade(high.log10().ceil()).max(low + 1);
        Self {
            low,
            high,
            from,
            to,
        }
    }

    fn at(self, v: f64) -> f64 {
        let low = f64::from(self.low);
        let high = f64::from(self.high);
        let at = v.max(f64::MIN_POSITIVE).log10().clamp(low, high);
        self.from + (at - low) / (high - low) * (self.to - self.from)
    }

    fn holds(self, v: f64) -> bool {
        v > 0.0 && (f64::from(self.low)..=f64::from(self.high)).contains(&v.log10())
    }

    fn decades(self) -> impl Iterator<Item = f64> {
        (self.low..=self.high).map(|e| 10.0_f64.powi(e))
    }
}

/// A whole decade exponent, clamped to the range latencies live in.
fn decade(exponent: f64) -> i32 {
    (-3_i32..=6)
        .find(|e| f64::from(*e) >= exponent)
        .unwrap_or(6)
}

/// A milliseconds axis label.
fn ms_label(ms: f64) -> String {
    if ms >= 1_000.0 {
        format!("{} s", format_rate(ms / 1_000.0).trim_end_matches(".0"))
    } else if ms >= 1.0 {
        format!("{} ms", format_rate(ms).trim_end_matches(".0"))
    } else {
        format!("{ms} ms")
    }
}

/// One point of a series: its x and the quantile at it.
struct Point<'a> {
    x: f64,
    latency: &'a Latency,
}

/// Draws one quantile's series over `points`: resolved values joined in
/// runs, a not-resolvable one marked in the strip below the plot.
fn series(
    out: &mut String,
    points: &[Point<'_>],
    quantile: QuantileName,
    class: &str,
    x: Linear,
    y: Log,
    scout: bool,
) {
    let mut runs: Vec<Vec<(f64, f64)>> = vec![Vec::new()];
    let mut unresolved: Vec<f64> = Vec::new();
    for p in points {
        match p.latency.get(quantile) {
            Quantile::Resolved { value_ms, .. } => {
                if let Some(run) = runs.last_mut() {
                    run.push((x.at(p.x), y.at(*value_ms)));
                }
            }
            Quantile::NotResolvable { .. } => {
                unresolved.push(x.at(p.x));
                runs.push(Vec::new());
            }
        }
    }
    let line = if scout { "scoutline" } else { "line" };
    for run in runs.iter().filter(|r| r.len() > 1) {
        let d: Vec<String> = run
            .iter()
            .enumerate()
            .map(|(i, (px, py))| format!("{}{px:.1},{py:.1}", if i == 0 { "M" } else { "L" }))
            .collect();
        let _ = writeln!(
            out,
            "<path d=\"{}\" class=\"{class} {line}\"/>",
            d.join(" ")
        );
    }
    for (px, py) in runs.iter().flatten() {
        if scout {
            let _ = writeln!(
                out,
                "<circle cx=\"{px:.1}\" cy=\"{py:.1}\" r=\"2.6\" class=\"{class} hollow\"/>"
            );
        } else {
            let _ = writeln!(
                out,
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"7\" height=\"7\" class=\"{class}\"/>",
                px - 3.5,
                py - 3.5
            );
        }
    }
    let offset = match quantile {
        QuantileName::P50 => -4.0,
        QuantileName::P99 => 4.0,
        _ => 0.0,
    };
    for px in unresolved {
        let (cx, cy) = (px, NR_STRIP + offset);
        let _ = writeln!(
            out,
            "<path d=\"M{:.1},{cy:.1} L{cx:.1},{:.1} L{:.1},{cy:.1} L{cx:.1},{:.1} Z\" class=\"{class} nr\"/>",
            cx - 3.0,
            cy - 3.0,
            cx + 3.0,
            cy + 3.0
        );
    }
}

/// The plateau the knee was read at, and the failing plateau above it.
fn bracket_plateaus(record: &PerfResult) -> (Option<&DerivedPlateau>, Option<&DerivedPlateau>) {
    let at = |rate: f64| {
        record
            .derived
            .plateaus
            .iter()
            .find(|p| p.planned_rate.total_cmp(&rate).is_eq())
    };
    match &record.derived.knee {
        Knee::Bracket {
            qualified_rate,
            failing_rate,
            ..
        } => (at(*qualified_rate), at(*failing_rate)),
        Knee::LowerBound { qualified_rate, .. } => (at(*qualified_rate), None),
        Knee::BelowLadder { failing_rate } => (None, at(*failing_rate)),
        Knee::NoPlateau => (None, None),
    }
}

/// Shades the knee between `from` and `to` on the x axis, or draws a line
/// where only one side is known.
fn bracket(out: &mut String, from: Option<f64>, to: Option<f64>, label: &str) {
    match (from, to) {
        (Some(a), Some(b)) => {
            let (left, right) = if a <= b { (a, b) } else { (b, a) };
            let _ = writeln!(
                out,
                "<rect x=\"{left:.1}\" y=\"{Y_TOP}\" width=\"{:.1}\" height=\"{:.1}\" class=\"bracket\"/>",
                (right - left).max(1.0),
                Y_BOTTOM - Y_TOP
            );
            let _ = writeln!(
                out,
                "<text x=\"{:.1}\" y=\"{:.1}\" class=\"okt\" text-anchor=\"middle\">{}</text>",
                f64::midpoint(left, right),
                Y_TOP - 6.0,
                esc(label)
            );
        }
        (Some(edge), None) | (None, Some(edge)) => {
            let _ = writeln!(
                out,
                "<line x1=\"{edge:.1}\" y1=\"{Y_TOP}\" x2=\"{edge:.1}\" y2=\"{Y_BOTTOM}\" class=\"bracket\"/>\
                 <text x=\"{edge:.1}\" y=\"{:.1}\" class=\"okt\" text-anchor=\"middle\">{}</text>",
                Y_TOP - 6.0,
                esc(label)
            );
        }
        (None, None) => {}
    }
}

fn knee_label(knee: &Knee) -> &'static str {
    match knee {
        Knee::Bracket { .. } => "knee bracket",
        Knee::LowerBound { .. } => "knee at or above",
        Knee::BelowLadder { .. } => "knee below",
        Knee::NoPlateau => "",
    }
}

fn x_axis(out: &mut String, x: Linear, label: &str) {
    for tick in x.ticks() {
        let px = x.at(tick);
        let _ = writeln!(
            out,
            "<line x1=\"{px:.1}\" y1=\"{Y_TOP}\" x2=\"{px:.1}\" y2=\"{Y_BOTTOM}\" class=\"grid\"/>\
             <text x=\"{px:.1}\" y=\"{:.1}\" class=\"muted\" text-anchor=\"middle\">{}</text>",
            Y_BOTTOM + 34.0,
            format_rate(tick).trim_end_matches(".0")
        );
    }
    let _ = writeln!(
        out,
        "<text x=\"{:.1}\" y=\"{:.1}\" class=\"muted\" text-anchor=\"middle\">{}</text>",
        f64::midpoint(X0, X1),
        Y_BOTTOM + 52.0,
        esc(label)
    );
}

/// The fitted curve in latency-throughput space: throughput `X(N)` and
/// mean latency `N / X(N)` by Little's law, for `N` up to `n_max`.
fn fit_curve(usl: &UslReading, n_max: f64) -> Vec<(f64, f64)> {
    let UslReading::Fitted {
        alpha, beta, gamma, ..
    } = usl
    else {
        return Vec::new();
    };
    (1..=FIT_POINTS)
        .filter_map(|i| {
            let n = 1.0 + (n_max - 1.0).max(0.0) * f64::from(i) / f64::from(FIT_POINTS);
            let x = gamma * n / (1.0 + alpha * (n - 1.0) + beta * n * (n - 1.0));
            (x.is_finite() && x > 0.0).then(|| (x, n / x * 1_000.0))
        })
        .collect()
}

/// The saturation curve: latency against achieved throughput.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "one linear chart emitter: axes, marks and labels in drawing order"
)]
pub fn saturation(record: &PerfResult) -> String {
    let derived = &record.derived;
    let scout: Vec<Point<'_>> = derived
        .scout
        .iter()
        .map(|w| Point {
            x: w.throughput.achieved_per_s,
            latency: &w.latency,
        })
        .collect();
    let mut plateaus: Vec<Point<'_>> = derived
        .plateaus
        .iter()
        .map(|p| Point {
            x: p.throughput.achieved_per_s,
            latency: &p.latency,
        })
        .collect();
    plateaus.sort_by(|a, b| a.x.total_cmp(&b.x));
    let n_max = derived
        .scout
        .iter()
        .map(|w| w.throughput.mean_in_flight)
        .chain(derived.plateaus.iter().map(|p| p.throughput.mean_in_flight))
        .fold(1.0, f64::max)
        * 1.1;
    let curve = fit_curve(&derived.usl, n_max);
    let max_x = scout
        .iter()
        .chain(&plateaus)
        .map(|p| p.x)
        .fold(1.0, f64::max);
    let x = Linear {
        max: nice_ceil(max_x * 1.05),
        from: X0,
        to: X1,
    };
    let values: Vec<f64> = scout
        .iter()
        .chain(&plateaus)
        .flat_map(|p| {
            DRAWN
                .iter()
                .filter_map(|(q, _)| p.latency.get(*q).value_ms())
        })
        .collect();
    let y = Log::spanning(&values, Y_BOTTOM, Y_TOP);

    let mut out = String::new();
    open(
        &mut out,
        760.0,
        430.0,
        "Latency against achieved throughput",
        &format!(
            "p50, p95 and p99 over {} scout windows and {} plateaus of {}. {}",
            scout.len(),
            plateaus.len(),
            record.raw.workload.id,
            knee_sentence(&derived.knee)
        ),
    );
    let _ = writeln!(
        out,
        "<text x=\"24\" y=\"28\" class=\"title\">Latency against achieved throughput: {}</text>",
        esc(&record.raw.workload.id)
    );
    let _ = writeln!(
        out,
        "<text x=\"24\" y=\"46\" class=\"muted\">Scout windows: hollow circles on thin lines. Plateaus: squares on thick lines. \
         ◇ below the plot: the quantile does not resolve.</text>"
    );
    let _ = writeln!(
        out,
        "<text x=\"24\" y=\"62\" class=\"muted\">Dashed: the Universal Scalability Law fit, mean latency by Little's law; an explanatory model, never a verdict.</text>"
    );
    for (i, (_, class)) in DRAWN.iter().enumerate() {
        let lx = 24.0 + 64.0 * f64::from(u8::try_from(i).unwrap_or(0));
        let _ = writeln!(
            out,
            "<rect x=\"{lx:.1}\" y=\"70\" width=\"10\" height=\"10\" class=\"{class}\"/>\
             <text x=\"{:.1}\" y=\"79\">{class}</text>",
            lx + 14.0
        );
    }
    for ms in y.decades() {
        let py = y.at(ms);
        let _ = writeln!(
            out,
            "<line x1=\"{X0}\" y1=\"{py:.1}\" x2=\"{X1}\" y2=\"{py:.1}\" class=\"grid\"/>\
             <text x=\"{:.1}\" y=\"{:.1}\" class=\"muted\" text-anchor=\"end\">{}</text>",
            X0 - 8.0,
            py + 4.0,
            ms_label(ms)
        );
    }
    x_axis(&mut out, x, "achieved throughput, operations per second");
    let _ = writeln!(
        out,
        "<text x=\"{:.1}\" y=\"{:.1}\" class=\"muted\" text-anchor=\"end\">n/r</text>",
        X0 - 8.0,
        NR_STRIP + 4.0
    );
    let (qualified, failing) = bracket_plateaus(record);
    bracket(
        &mut out,
        qualified.map(|p| x.at(p.throughput.achieved_per_s)),
        failing.map(|p| x.at(p.throughput.achieved_per_s)),
        knee_label(&derived.knee),
    );
    let fit: Vec<String> = curve
        .iter()
        .filter(|(cx, ms)| *cx <= x.max && y.holds(*ms))
        .enumerate()
        .map(|(i, (cx, ms))| {
            format!(
                "{}{:.1},{:.1}",
                if i == 0 { "M" } else { "L" },
                x.at(*cx),
                y.at(*ms)
            )
        })
        .collect();
    if fit.len() > 1 {
        let _ = writeln!(out, "<path d=\"{}\" class=\"fit\"/>", fit.join(" "));
    }
    for (quantile, class) in DRAWN {
        series(&mut out, &scout, quantile, class, x, y, true);
        series(&mut out, &plateaus, quantile, class, x, y, false);
    }
    out.push_str("</svg>\n");
    out
}

/// Achieved against offered throughput, with the identity line.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "one linear chart emitter: axes, marks and labels in drawing order"
)]
pub fn throughput(record: &PerfResult) -> String {
    let derived = &record.derived;
    let max = derived
        .scout
        .iter()
        .map(|w| w.planned_rate.max(w.throughput.achieved_per_s))
        .chain(
            derived
                .plateaus
                .iter()
                .map(|p| p.planned_rate.max(p.throughput.achieved_per_s)),
        )
        .fold(1.0, f64::max);
    let x = Linear {
        max: nice_ceil(max * 1.05),
        from: X0,
        to: X1,
    };
    let y = Linear {
        max: x.max,
        from: Y_BOTTOM,
        to: Y_TOP,
    };
    let mut out = String::new();
    open(
        &mut out,
        760.0,
        430.0,
        "Achieved against offered throughput",
        &format!(
            "{} scout windows and {} plateaus of {}; the dotted line is achieved equal to offered.",
            derived.scout.len(),
            derived.plateaus.len(),
            record.raw.workload.id
        ),
    );
    let _ = writeln!(
        out,
        "<text x=\"24\" y=\"28\" class=\"title\">Achieved against offered throughput: {}</text>",
        esc(&record.raw.workload.id)
    );
    let _ = writeln!(
        out,
        "<text x=\"24\" y=\"46\" class=\"muted\">Scout windows: hollow circles. Plateaus: a filled square qualifies, a cross fails. \
         Dotted: achieved equals offered.</text>"
    );
    for tick in y.ticks() {
        let py = y.at(tick);
        let _ = writeln!(
            out,
            "<line x1=\"{X0}\" y1=\"{py:.1}\" x2=\"{X1}\" y2=\"{py:.1}\" class=\"grid\"/>\
             <text x=\"{:.1}\" y=\"{:.1}\" class=\"muted\" text-anchor=\"end\">{}</text>",
            X0 - 8.0,
            py + 4.0,
            format_rate(tick).trim_end_matches(".0")
        );
    }
    x_axis(&mut out, x, "offered throughput, operations per second");
    let _ = writeln!(
        out,
        "<text x=\"20\" y=\"{:.1}\" class=\"muted\" transform=\"rotate(-90 20 {:.1})\" text-anchor=\"middle\">achieved, operations per second</text>",
        f64::midpoint(Y_TOP, Y_BOTTOM),
        f64::midpoint(Y_TOP, Y_BOTTOM)
    );
    let (qualified, failing) = bracket_plateaus(record);
    bracket(
        &mut out,
        qualified.map(|p| x.at(p.planned_rate)),
        failing.map(|p| x.at(p.planned_rate)),
        knee_label(&derived.knee),
    );
    let _ = writeln!(
        out,
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" class=\"identity\"/>",
        x.at(0.0),
        y.at(0.0),
        x.at(x.max),
        y.at(x.max)
    );
    for w in &derived.scout {
        let _ = writeln!(
            out,
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"3\" class=\"p95 hollow\"/>",
            x.at(w.planned_rate),
            y.at(w.throughput.achieved_per_s)
        );
    }
    for p in &derived.plateaus {
        let (px, py) = (x.at(p.planned_rate), y.at(p.throughput.achieved_per_s));
        if p.qualification.qualified {
            let _ = writeln!(
                out,
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"8\" height=\"8\" class=\"ok\"/>",
                px - 4.0,
                py - 4.0
            );
        } else {
            let _ = writeln!(
                out,
                "<path d=\"M{:.1},{:.1} L{:.1},{:.1} M{:.1},{:.1} L{:.1},{:.1}\" class=\"bad\" stroke-width=\"2.2\"/>",
                px - 5.0,
                py - 5.0,
                px + 5.0,
                py + 5.0,
                px - 5.0,
                py + 5.0,
                px + 5.0,
                py - 5.0
            );
        }
    }
    out.push_str("</svg>\n");
    out
}

/// Each operation's latency at the knee, at the quantile it is judged at,
/// beside its budget; the lowest plateau stands in when no plateau
/// qualifies.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "one linear chart emitter: axes, marks and labels in drawing order"
)]
pub fn operations(record: &PerfResult) -> String {
    let (qualified, _) = bracket_plateaus(record);
    let lowest = record
        .derived
        .plateaus
        .iter()
        .min_by(|a, b| a.planned_rate.total_cmp(&b.planned_rate));
    let (place, plateau) = match (qualified, lowest) {
        (Some(p), _) => ("at the knee", Some(p)),
        (None, Some(p)) => ("at the lowest plateau, no plateau qualifying", Some(p)),
        (None, None) => ("with no plateau", None),
    };
    let rows: Vec<_> = plateau
        .map(|p| p.operations.iter().collect())
        .unwrap_or_default();
    let row_height = 34.0;
    let top = 84.0;
    let rows_f = f64::from(u32::try_from(rows.len()).unwrap_or(u32::MAX));
    let height = top + row_height * rows_f.max(1.0) + 56.0;
    let values: Vec<f64> = rows
        .iter()
        .flat_map(|(_, op)| {
            let judged = op
                .judged_at
                .quantile()
                .and_then(|q| op.latency.get(q).value_ms());
            judged.into_iter().chain([f64::from(op.budget_ms)])
        })
        .collect();
    let (left, right) = (230.0, 600.0);
    let x = Log::spanning(&values, left, right);
    let bottom = top + row_height * rows_f.max(1.0);

    let mut out = String::new();
    let rate = plateau.map_or_else(String::new, |p| {
        format!(", {}/s offered", format_rate(p.planned_rate))
    });
    open(
        &mut out,
        760.0,
        height,
        "Latency per operation at the knee",
        &format!(
            "Each operation of {} {place}{rate}, at the quantile it is judged at, beside its budget.",
            record.raw.workload.id
        ),
    );
    let _ = writeln!(
        out,
        "<text x=\"24\" y=\"28\" class=\"title\">Latency per operation {}{}</text>",
        esc(place),
        esc(&rate)
    );
    let _ = writeln!(
        out,
        "<text x=\"24\" y=\"46\" class=\"muted\">The bar reaches each operation's latency at the quantile it is judged at, the highest its samples resolve; \
         the red tick is its budget.</text>"
    );
    for ms in x.decades() {
        let px = x.at(ms);
        let _ = writeln!(
            out,
            "<line x1=\"{px:.1}\" y1=\"{top}\" x2=\"{px:.1}\" y2=\"{bottom:.1}\" class=\"grid\"/>\
             <text x=\"{px:.1}\" y=\"{:.1}\" class=\"muted\" text-anchor=\"middle\">{}</text>",
            bottom + 18.0,
            ms_label(ms)
        );
    }
    for (i, (id, op)) in rows.iter().enumerate() {
        let row_top = top + row_height * f64::from(u32::try_from(i).unwrap_or(0));
        let mid = row_top + row_height / 2.0;
        let _ = writeln!(
            out,
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\">{}</text>",
            left - 10.0,
            mid + 4.0,
            esc(id.as_str())
        );
        let budget = x.at(f64::from(op.budget_ms));
        let judged = op
            .judged_at
            .quantile()
            .and_then(|q| op.latency.get(q).value_ms());
        match judged {
            Some(ms) => {
                let end = x.at(ms);
                let within = ms <= f64::from(op.budget_ms);
                let _ = writeln!(
                    out,
                    "<rect x=\"{left}\" y=\"{:.1}\" width=\"{:.1}\" height=\"14\" class=\"bar\"/>\
                     <text x=\"{:.1}\" y=\"{:.1}\" class=\"{}\">{} {} ms of {} {}</text>",
                    mid - 7.0,
                    (end - left).max(1.0),
                    right + 12.0,
                    mid + 4.0,
                    if within { "okt" } else { "badt" },
                    judged_token(op.judged_at),
                    format_ms(ms),
                    format_ms(f64::from(op.budget_ms)),
                    if within { "✓" } else { "✕" }
                );
            }
            None => {
                let _ = writeln!(
                    out,
                    "<text x=\"{:.1}\" y=\"{:.1}\" class=\"muted\">not resolvable at any quantile</text>",
                    left + 6.0,
                    mid + 4.0
                );
            }
        }
        let _ = writeln!(
            out,
            "<line x1=\"{budget:.1}\" y1=\"{:.1}\" x2=\"{budget:.1}\" y2=\"{:.1}\" class=\"budget\"/>",
            mid - 11.0,
            mid + 11.0
        );
    }
    out.push_str("</svg>\n");
    out
}

/// Every chart of `record`, in publication order.
#[must_use]
pub fn charts(record: &PerfResult) -> Vec<Chart> {
    vec![
        Chart {
            name: "perf-saturation.svg",
            svg: saturation(record),
        },
        Chart {
            name: "perf-throughput.svg",
            svg: throughput(record),
        },
        Chart {
            name: "perf-operations.svg",
            svg: operations(record),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn latency(p50: Option<f64>) -> Latency {
        let q = |v: Option<f64>| match v {
            Some(value_ms) => Quantile::Resolved {
                value_ms,
                rank: 1,
                low_ms: value_ms,
                low_rank: 1,
                high_ms: value_ms,
                high_rank: 1,
            },
            None => Quantile::NotResolvable { samples_above: 3 },
        };
        Latency {
            p50: q(p50),
            p90: q(None),
            p95: q(None),
            p99: q(None),
            p99_9: q(None),
            max_ms: None,
        }
    }

    #[test]
    fn a_quantile_that_does_not_resolve_breaks_the_line_and_is_marked_apart() {
        let latencies = [
            latency(Some(5.0)),
            latency(Some(6.0)),
            latency(None),
            latency(Some(9.0)),
            latency(Some(12.0)),
        ];
        let points: Vec<Point<'_>> = latencies
            .iter()
            .zip([0.0, 10.0, 20.0, 30.0, 40.0])
            .map(|(l, x)| Point { x, latency: l })
            .collect();
        let x = Linear {
            max: 50.0,
            from: X0,
            to: X1,
        };
        let y = Log::spanning(&[5.0, 12.0], Y_BOTTOM, Y_TOP);
        let mut out = String::new();
        series(&mut out, &points, QuantileName::P50, "p50", x, y, false);
        assert_eq!(out.matches(r#"class="p50 line""#).count(), 2, "{out}");
        assert_eq!(out.matches(r#"class="p50 nr""#).count(), 1, "{out}");
        assert!(
            out.lines()
                .filter(|l| l.contains("p50 line"))
                .all(|l| l.matches('L').count() == 1),
            "a line joins across the gap: {out}"
        );
        let mut scout = String::new();
        series(&mut scout, &points, QuantileName::P50, "p50", x, y, true);
        assert_eq!(scout.matches("<circle").count(), 4);
        assert!(!scout.contains("<rect"));
    }

    #[test]
    fn a_log_axis_spans_whole_decades() {
        let axis = Log::spanning(&[3.0, 4_200.0], 0.0, 100.0);
        assert_eq!((axis.low, axis.high), (0, 4));
        assert!((axis.at(10.0) - 25.0).abs() < 1e-9);
        assert!(axis.holds(4_200.0));
        assert!(!axis.holds(0.0));
        assert!((nice_ceil(212.0) - 240.0).abs() < 1e-9);
        assert_eq!(ms_label(1_000.0), "1 s");
    }
}
