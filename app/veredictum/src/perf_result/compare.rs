// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Comparing `perf-result` records: alignment, ratios and the A/A floor.
//!
//! Alignment reads two or more records into one table and names every
//! mismatch that makes their numbers describe different things: the
//! workload, the corpus recipe, the posture, the host prescription and the
//! generator's fingerprint, and the network leg. Two records from one
//! interleaved run (`perf-compare --ab` or `--aa`) share their ladder, so
//! their plateaus line up rung by rung and every metric gets a ratio with
//! both values beside it.
//!
//! A single pass measures sampling intervals only. Between-run noise is
//! measured by an A/A run: the same target on both sides, interleaved, whose
//! largest ratio per metric is the floor. A difference counts only when its
//! ratio exceeds both the declared tolerance and the floor, and the two
//! sides' 95 % intervals do not overlap (Kalibera and Jones, "Rigorous
//! Benchmarking in Reasonable Time", ISMM 2013, on measuring the noise at
//! the level it arises). No openEHR specification governs this: our own
//! design.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::perf_emulate::posture::PostureRecord;
use crate::perf_result::PerfResult;
use crate::perf_result::derive::{DerivedPlateau, Latency};
use crate::perf_result::quantile::Quantile;
use crate::perf_result::view::{format_ms, format_rate, knee_sentence};

/// The tolerance a comparison declares unless told otherwise: a ratio
/// within 5 % either way is never a difference.
pub const DEFAULT_TOLERANCE: f64 = 0.05;

/// The side of an interleaved run (a closed vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideName {
    /// The first target named.
    A,
    /// The second target named.
    B,
}

impl SideName {
    /// The side's token, `a` or `b`.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::A => "a",
            Self::B => "b",
        }
    }
}

/// One plateau of an interleaved run: the rung of the shared ladder it
/// belongs to and the side it ran on.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    /// The rung of the shared ladder, from zero.
    pub rung: usize,
    /// The planned operation rate of the rung, per second.
    pub planned_rate: f64,
    /// The side the plateau ran on.
    pub side: SideName,
}

/// What an interleaved run compared (a closed vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterleaveMode {
    /// One target on both sides: the run measures its own noise.
    Aa,
    /// Two targets.
    Ab,
}

/// The interleaving a record took part in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interleaving {
    /// A/A or A/B.
    pub mode: InterleaveMode,
    /// The side this record is.
    pub side: SideName,
    /// The other side's base URL.
    pub partner: String,
    /// The knee estimate the shared ladder was built from.
    pub shared_estimate: f64,
    /// Every plateau of the run, both sides, in the order they ran.
    pub sequence: Vec<Slot>,
}

/// A metric a comparison takes a ratio of (a closed vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CompareMetric {
    /// The median latency.
    #[serde(rename = "p50")]
    P50,
    /// The 95th percentile latency.
    #[serde(rename = "p95")]
    P95,
    /// The 99th percentile latency.
    #[serde(rename = "p99")]
    P99,
    /// The 99.9th percentile latency.
    #[serde(rename = "p99_9")]
    P999,
    /// The achieved throughput.
    #[serde(rename = "achieved")]
    Achieved,
}

impl CompareMetric {
    /// Every metric, in table order.
    pub const ALL: &[CompareMetric] = &[
        CompareMetric::P50,
        CompareMetric::P95,
        CompareMetric::P99,
        CompareMetric::P999,
        CompareMetric::Achieved,
    ];

    /// The metric's token.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::P50 => "p50",
            Self::P95 => "p95",
            Self::P99 => "p99",
            Self::P999 => "p99_9",
            Self::Achieved => "achieved",
        }
    }

    fn quantile(self, latency: &Latency) -> Option<&Quantile> {
        match self {
            Self::P50 => Some(&latency.p50),
            Self::P95 => Some(&latency.p95),
            Self::P99 => Some(&latency.p99),
            Self::P999 => Some(&latency.p99_9),
            Self::Achieved => None,
        }
    }
}

/// The A/A floor: per metric, the largest ratio of the larger side over the
/// smaller one that an interleaved run of one target against itself showed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AaFloor {
    /// The largest A/A ratio per metric, at least 1.
    pub ratios: BTreeMap<CompareMetric, f64>,
    /// The aligned value pairs the floor was taken over.
    pub pairs: u64,
}

impl AaFloor {
    /// The floor of `metric`, 1 when the A/A run gave no pair for it.
    #[must_use]
    pub fn of(&self, metric: CompareMetric) -> f64 {
        self.ratios.get(&metric).copied().unwrap_or(1.0)
    }
}

/// How one aligned value pair reads (a closed vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowReading {
    /// The ratio exceeds the tolerance and the floor, and the intervals do
    /// not overlap.
    Difference,
    /// The ratio stays inside the tolerance or the floor, or the intervals
    /// overlap.
    WithinNoise,
    /// One side has no value: a quantile that does not resolve, or an
    /// operation with no sample.
    NotResolvable,
    /// The two plateaus were offered different rates, so their values are
    /// shown and not judged.
    DifferentRates,
}

/// One metric of one operation on one rung, both sides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RatioRow {
    /// The plateau's position in both records.
    pub rung: usize,
    /// Side A's planned rate, per second.
    pub planned_rate: f64,
    /// The operation, or `all` for the plateau as a whole.
    pub operation: String,
    /// The metric.
    pub metric: CompareMetric,
    /// Side A's value: milliseconds, or per second for `achieved`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a: Option<f64>,
    /// Side B's value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b: Option<f64>,
    /// B over A.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ratio: Option<f64>,
    /// Whether the two 95 % intervals overlap; absent for `achieved`, which
    /// has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlap: Option<bool>,
    /// The reading.
    pub reading: RowReading,
}

/// A field on which aligned records disagree (a closed vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlignField {
    /// The workload's digest.
    Workload,
    /// The corpus recipe's digest.
    Corpus,
    /// The declared posture.
    Posture,
    /// The host prescription claimed.
    HostPrescription,
    /// The generator's fingerprint.
    Fingerprint,
    /// The network leg.
    NetworkLeg,
}

impl AlignField {
    /// Every field, in header order.
    pub const ALL: &[AlignField] = &[
        AlignField::Workload,
        AlignField::Corpus,
        AlignField::Posture,
        AlignField::HostPrescription,
        AlignField::Fingerprint,
        AlignField::NetworkLeg,
    ];

    /// The field's token.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Workload => "workload",
            Self::Corpus => "corpus",
            Self::Posture => "posture",
            Self::HostPrescription => "host_prescription",
            Self::Fingerprint => "fingerprint",
            Self::NetworkLeg => "network_leg",
        }
    }
}

/// One mismatch: the field and each record's value, in record order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mismatch {
    /// The field.
    pub field: AlignField,
    /// Each record's value.
    pub values: Vec<String>,
}

/// What a comparison read from one record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordHeader {
    /// The record's label: its side, or its file name.
    pub label: String,
    /// The target's base URL.
    pub base_url: String,
    /// Each alignment field's value, by field token.
    pub fields: BTreeMap<String, String>,
}

/// The kind of comparison (a closed vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareMode {
    /// Records read from files, aligned.
    Aligned,
    /// An interleaved run of one target against itself.
    Aa,
    /// An interleaved run of two targets.
    Ab,
}

/// The document family tag of a comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompareFamily {
    /// The only family.
    #[serde(rename = "perf-compare")]
    PerfCompare,
}

/// One row of the aligned table: a label and one cell per record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableRow {
    /// What the row shows.
    pub label: String,
    /// One cell per record.
    pub cells: Vec<String>,
}

/// A comparison of `perf-result` records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerfComparison {
    /// Always `perf-compare`.
    pub family: CompareFamily,
    /// Aligned, A/A or A/B.
    pub mode: CompareMode,
    /// The records, in the order named.
    pub records: Vec<RecordHeader>,
    /// Every field on which the records disagree.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mismatches: Vec<Mismatch>,
    /// The aligned table, one cell per record.
    pub table: Vec<TableRow>,
    /// The declared tolerance: a ratio within it either way is never a
    /// difference.
    pub tolerance: f64,
    /// The A/A floor the ratios were judged against, when one is attached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floor: Option<AaFloor>,
    /// The interleaved plateaus in the order they ran, for an A/A or A/B
    /// run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sequence: Vec<Slot>,
    /// Ratios of two records, rung by rung; empty for more than two.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<RatioRow>,
}

impl PerfComparison {
    /// Whether any mismatch applies, which makes `perf-compare` exit `1`.
    #[must_use]
    pub fn mismatched(&self) -> bool {
        !self.mismatches.is_empty()
    }

    /// The rows read as a difference.
    #[must_use]
    pub fn differences(&self) -> Vec<&RatioRow> {
        self.rows
            .iter()
            .filter(|r| r.reading == RowReading::Difference)
            .collect()
    }

    /// The comparison's canonical text: pretty JSON and a trailing newline.
    ///
    /// # Errors
    /// The serializer's message when serialization fails.
    pub fn render(&self) -> Result<String, String> {
        let mut text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        text.push('\n');
        Ok(text)
    }
}

/// Returns each alignment field's value for `record`.
fn header_fields(record: &PerfResult) -> BTreeMap<AlignField, String> {
    let raw = &record.raw;
    BTreeMap::from([
        (
            AlignField::Workload,
            format!("{} {}", raw.workload.label, raw.workload.sha256),
        ),
        (
            AlignField::Corpus,
            format!("{} {}", raw.corpus.scale.token(), raw.corpus.recipe_sha256),
        ),
        (
            AlignField::Posture,
            raw.posture
                .as_ref()
                .map_or_else(|| "undeclared".to_owned(), PostureRecord::signature),
        ),
        (
            AlignField::HostPrescription,
            raw.host_prescription
                .as_ref()
                .map_or_else(|| "none".to_owned(), |h| format!("{}@{}", h.id, h.version)),
        ),
        (
            AlignField::Fingerprint,
            raw.generator
                .host
                .labels()
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(" "),
        ),
        (
            AlignField::NetworkLeg,
            raw.network_leg.unwrap_or_default().token().to_owned(),
        ),
    ])
}

/// Returns the fields on which `records` disagree.
#[must_use]
pub fn mismatches(records: &[&PerfResult]) -> Vec<Mismatch> {
    let all: Vec<BTreeMap<AlignField, String>> = records.iter().map(|r| header_fields(r)).collect();
    AlignField::ALL
        .iter()
        .filter_map(|field| {
            let values: Vec<String> = all
                .iter()
                .map(|f| f.get(field).cloned().unwrap_or_default())
                .collect();
            let first = values.first()?;
            let differ = values.iter().any(|v| v != first);
            differ.then_some(Mismatch {
                field: *field,
                values,
            })
        })
        .collect()
}

fn cell_quantile(q: &Quantile) -> String {
    q.value_ms().map_or_else(|| "n/r".to_owned(), format_ms)
}

/// Builds the aligned table: the knee, the peak and the scout estimate,
/// then per plateau position the offered and achieved rates and the overall
/// p50 to p99.9.
fn table(records: &[&PerfResult]) -> Vec<TableRow> {
    let row = |label: &str, cell: &dyn Fn(&PerfResult) -> String| TableRow {
        label: label.to_owned(),
        cells: records.iter().map(|r| cell(r)).collect(),
    };
    let mut rows = vec![
        row("workload", &|r| r.raw.workload.label.clone()),
        row("knee", &|r| knee_sentence(&r.derived.knee)),
        row("peak achieved/s", &|r| {
            r.derived
                .peak
                .as_ref()
                .map_or_else(|| "—".to_owned(), |p| format_rate(p.achieved_per_s))
        }),
        row("scout estimate/s", &|r| {
            r.derived
                .scout_estimate
                .as_ref()
                .map_or_else(|| "—".to_owned(), |e| format_rate(e.rate))
        }),
    ];
    let longest = records
        .iter()
        .map(|r| r.derived.plateaus.len())
        .max()
        .unwrap_or(0);
    for index in 0..longest {
        let at = |r: &PerfResult| r.derived.plateaus.get(index).cloned();
        let cell = |f: &dyn Fn(&DerivedPlateau) -> String| -> Vec<String> {
            records
                .iter()
                .map(|r| at(r).map_or_else(|| "—".to_owned(), |p| f(&p)))
                .collect()
        };
        rows.push(TableRow {
            label: format!("plateau {index} offered/s"),
            cells: cell(&|p| format_rate(p.planned_rate)),
        });
        rows.push(TableRow {
            label: format!("plateau {index} achieved/s"),
            cells: cell(&|p| format_rate(p.throughput.achieved_per_s)),
        });
        for metric in [
            CompareMetric::P50,
            CompareMetric::P95,
            CompareMetric::P99,
            CompareMetric::P999,
        ] {
            rows.push(TableRow {
                label: format!("plateau {index} {} ms", metric.token()),
                cells: cell(&|p| {
                    metric
                        .quantile(&p.latency)
                        .map_or_else(|| "—".to_owned(), cell_quantile)
                }),
            });
        }
        rows.push(TableRow {
            label: format!("plateau {index} qualified"),
            cells: cell(&|p| {
                if p.qualification.qualified {
                    "yes".to_owned()
                } else {
                    "no".to_owned()
                }
            }),
        });
    }
    rows
}

/// A 95 % interval, low and high, in milliseconds.
type Interval = (f64, f64);

fn interval(q: &Quantile) -> Option<Interval> {
    match q {
        Quantile::Resolved {
            low_ms, high_ms, ..
        } => Some((*low_ms, *high_ms)),
        Quantile::NotResolvable { .. } => None,
    }
}

/// The larger of a ratio and its inverse: how far apart two values are,
/// whichever is larger.
fn spread(ratio: f64) -> f64 {
    if ratio >= 1.0 { ratio } else { 1.0 / ratio }
}

/// Builds the rows of one metric of one operation on one rung.
#[expect(
    clippy::too_many_arguments,
    reason = "one row's inputs: its place, both sides' values and intervals, and its judgement"
)]
fn ratio_row(
    rung: usize,
    planned_rate: f64,
    same_rate: bool,
    operation: &str,
    metric: CompareMetric,
    a: Option<f64>,
    b: Option<f64>,
    intervals: Option<[Option<Interval>; 2]>,
    limit: f64,
) -> RatioRow {
    let ratio = match (a, b) {
        (Some(a), Some(b)) if a > 0.0 && b > 0.0 => Some(b / a),
        _ => None,
    };
    let overlap = intervals.and_then(|[ia, ib]| {
        let ((al, ah), (bl, bh)) = (ia?, ib?);
        Some(al <= bh && bl <= ah)
    });
    let reading = match ratio {
        None => RowReading::NotResolvable,
        Some(_) if !same_rate => RowReading::DifferentRates,
        Some(r) if spread(r) > limit && overlap != Some(true) => RowReading::Difference,
        Some(_) => RowReading::WithinNoise,
    };
    RatioRow {
        rung,
        planned_rate,
        operation: operation.to_owned(),
        metric,
        a,
        b,
        ratio,
        overlap,
        reading,
    }
}

/// Returns the ratio rows of two records, rung by rung: the plateau as a
/// whole and every operation both sides measured, each latency metric and
/// the achieved throughput.
#[must_use]
pub fn ratio_rows(
    a: &PerfResult,
    b: &PerfResult,
    tolerance: f64,
    floor: Option<&AaFloor>,
) -> Vec<RatioRow> {
    let mut rows = Vec::new();
    for (rung, (pa, pb)) in a
        .derived
        .plateaus
        .iter()
        .zip(&b.derived.plateaus)
        .enumerate()
    {
        let same_rate = (pa.planned_rate - pb.planned_rate).abs() <= 1e-9 * pa.planned_rate.abs();
        let mut latencies = vec![("all".to_owned(), &pa.latency, &pb.latency)];
        for (id, op) in &pa.operations {
            if let Some(other) = pb.operations.get(id) {
                latencies.push((id.to_string(), &op.latency, &other.latency));
            }
        }
        for metric in CompareMetric::ALL.iter().copied() {
            let limit = (1.0 + tolerance).max(floor.map_or(1.0, |f| f.of(metric)));
            if metric == CompareMetric::Achieved {
                rows.push(ratio_row(
                    rung,
                    pa.planned_rate,
                    same_rate,
                    "all",
                    metric,
                    Some(pa.throughput.achieved_per_s),
                    Some(pb.throughput.achieved_per_s),
                    None,
                    limit,
                ));
                continue;
            }
            for (operation, la, lb) in &latencies {
                let (Some(qa), Some(qb)) = (metric.quantile(la), metric.quantile(lb)) else {
                    continue;
                };
                rows.push(ratio_row(
                    rung,
                    pa.planned_rate,
                    same_rate,
                    operation,
                    metric,
                    qa.value_ms(),
                    qb.value_ms(),
                    Some([interval(qa), interval(qb)]),
                    limit,
                ));
            }
        }
    }
    rows.sort_by(|x, y| (x.rung, &x.operation, x.metric).cmp(&(y.rung, &y.operation, y.metric)));
    rows
}

/// Returns the A/A floor two records of one target give: per metric, the
/// largest spread of any aligned pair offered the same rate.
#[must_use]
pub fn floor_of(a: &PerfResult, b: &PerfResult) -> AaFloor {
    let mut ratios: BTreeMap<CompareMetric, f64> =
        CompareMetric::ALL.iter().map(|m| (*m, 1.0)).collect();
    let mut pairs = 0_u64;
    for row in ratio_rows(a, b, 0.0, None) {
        let Some(ratio) = row.ratio else { continue };
        if row.reading == RowReading::DifferentRates {
            continue;
        }
        pairs = pairs.saturating_add(1);
        let slot = ratios.entry(row.metric).or_insert(1.0);
        *slot = slot.max(spread(ratio));
    }
    AaFloor { ratios, pairs }
}

/// Compares `records`, labelled, in the order given.
///
/// Every mismatch is named; the table holds one cell per record; and for
/// exactly two records the ratio rows are judged against `tolerance` and
/// the attached `floor`.
#[must_use]
pub fn compare(
    records: &[(String, &PerfResult)],
    mode: CompareMode,
    tolerance: f64,
    floor: Option<AaFloor>,
    sequence: Vec<Slot>,
) -> PerfComparison {
    let plain: Vec<&PerfResult> = records.iter().map(|(_, r)| *r).collect();
    let headers = records
        .iter()
        .map(|(label, record)| RecordHeader {
            label: label.clone(),
            base_url: record.raw.target.base_url.clone(),
            fields: header_fields(record)
                .into_iter()
                .map(|(k, v)| (k.token().to_owned(), v))
                .collect(),
        })
        .collect();
    let rows = match plain.as_slice() {
        [a, b] => ratio_rows(a, b, tolerance, floor.as_ref()),
        _ => Vec::new(),
    };
    PerfComparison {
        family: CompareFamily::PerfCompare,
        mode,
        records: headers,
        mismatches: mismatches(&plain),
        table: table(&plain),
        tolerance,
        floor,
        sequence,
        rows,
    }
}

fn value_cell(metric: CompareMetric, value: Option<f64>) -> String {
    match (metric, value) {
        (_, None) => "n/r".to_owned(),
        (CompareMetric::Achieved, Some(v)) => format_rate(v),
        (_, Some(v)) => format_ms(v),
    }
}

/// The comparison as Markdown: the mismatches above the table, the aligned
/// table, the interleaving order, and the ratio rows read as a difference
/// or not.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "one page: the mode, the mismatches, the table, the order and the ratios"
)]
pub fn markdown(comparison: &PerfComparison) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Performance comparison\n");
    let mode = match comparison.mode {
        CompareMode::Aligned => "records aligned from files",
        CompareMode::Aa => "an interleaved A/A run: one target on both sides",
        CompareMode::Ab => "an interleaved A/B run on one host",
    };
    let _ = writeln!(out, "Mode: {mode}.\n");
    if comparison.mismatches.is_empty() {
        let _ = writeln!(
            out,
            "No mismatch: the records share the workload, the corpus recipe, the posture, the \
             host prescription, the fingerprint and the network leg.\n"
        );
    } else {
        let _ = writeln!(
            out,
            "**Mismatches.** These records describe different things, so their numbers do not \
             compare:\n"
        );
        for mismatch in &comparison.mismatches {
            let _ = writeln!(
                out,
                "- `{}`: {}",
                mismatch.field.token(),
                mismatch
                    .values
                    .iter()
                    .zip(&comparison.records)
                    .map(|(v, r)| format!("{} = {v}", r.label))
                    .collect::<Vec<_>>()
                    .join("; ")
            );
        }
        out.push('\n');
    }
    let _ = write!(out, "| |");
    for record in &comparison.records {
        let _ = write!(out, " {} |", record.label);
    }
    let _ = write!(out, "\n|---|");
    for _ in &comparison.records {
        let _ = write!(out, "---|");
    }
    out.push('\n');
    for row in &comparison.table {
        let _ = writeln!(out, "| {} | {} |", row.label, row.cells.join(" | "));
    }
    if !comparison.sequence.is_empty() {
        let order: Vec<&str> = comparison
            .sequence
            .iter()
            .map(|s| match s.side {
                SideName::A => "A",
                SideName::B => "B",
            })
            .collect();
        let _ = writeln!(
            out,
            "\nPlateau order, two per rung: {} (ABBA per rung).",
            order.join(" ")
        );
    }
    if !comparison.rows.is_empty() {
        let floor = comparison.floor.as_ref().map_or_else(
            || "no A/A floor is attached".to_owned(),
            |f| {
                format!(
                    "the attached A/A floor ({})",
                    f.ratios
                        .iter()
                        .map(|(m, r)| format!("{} {r:.3}", m.token()))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            },
        );
        let _ = writeln!(
            out,
            "\nA difference counts when B/A or A/B exceeds {:.2} and {floor}, and the two 95 % \
             intervals do not overlap.\n",
            1.0 + comparison.tolerance
        );
        let _ = writeln!(
            out,
            "| rung | offered/s | operation | metric | A | B | B/A | reading |"
        );
        let _ = writeln!(out, "|---|---|---|---|---|---|---|---|");
        for row in &comparison.rows {
            let reading = match row.reading {
                RowReading::Difference => "difference",
                RowReading::WithinNoise => "within noise",
                RowReading::NotResolvable => "n/r",
                RowReading::DifferentRates => "different rates",
            };
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} | {} | {} | {reading} |",
                row.rung,
                format_rate(row.planned_rate),
                row.operation,
                row.metric.token(),
                value_cell(row.metric, row.a),
                value_cell(row.metric, row.b),
                row.ratio
                    .map_or_else(|| "—".to_owned(), |r| format!("{r:.3}")),
            );
        }
    }
    out
}

/// The comparison for a terminal: the mismatches, the aligned table, and
/// the differences only, each with both values.
#[must_use]
pub fn terminal(comparison: &PerfComparison) -> String {
    let mut out = String::new();
    for mismatch in &comparison.mismatches {
        let _ = writeln!(
            out,
            "MISMATCH {}: {}",
            mismatch.field.token(),
            mismatch.values.join(" | ")
        );
    }
    let width = comparison
        .table
        .iter()
        .map(|r| r.label.chars().count())
        .max()
        .unwrap_or(0);
    let _ = writeln!(
        out,
        "{:width$}  {}",
        "",
        comparison
            .records
            .iter()
            .map(|r| r.label.clone())
            .collect::<Vec<_>>()
            .join("  ")
    );
    for row in &comparison.table {
        if row.label == "knee" {
            continue;
        }
        let _ = writeln!(out, "{:width$}  {}", row.label, row.cells.join("  "));
    }
    if !comparison.sequence.is_empty() {
        let _ = writeln!(
            out,
            "order: {}",
            comparison
                .sequence
                .iter()
                .map(|s| s.side.token().to_uppercase())
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
    if !comparison.rows.is_empty() {
        let differences = comparison.differences();
        if differences.is_empty() {
            let _ = writeln!(out, "no difference beyond the tolerance and the floor");
        }
        for row in differences {
            let _ = writeln!(
                out,
                "difference: rung {} {} {}: A {} B {} (B/A {})",
                row.rung,
                row.operation,
                row.metric.token(),
                value_cell(row.metric, row.a),
                value_cell(row.metric, row.b),
                row.ratio
                    .map_or_else(|| "—".to_owned(), |r| format!("{r:.3}")),
            );
        }
    }
    out
}
