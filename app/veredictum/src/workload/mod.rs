// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The workload document: the traffic a measured run offers.
//!
//! A workload is a versioned YAML document under `perf/workloads/<id>.yaml`,
//! published as `schemas/perf-workload.schema.json`. It names a seed, a
//! corpus recipe, an arrival process, the qualification limits, the
//! per-plateau sample target, the interactions with their shares, and the AQL
//! query classes. [`load`] reads one through its gates; [`plan`] holds the
//! run shape the planned duration is computed from; [`manifest`] describes
//! the pinned workloads; [`wire`] realizes the query family and the subject
//! lookup on the wire.
//!
//! The shares are exact: a [`Share`] is a count of millionths, so "the shares
//! sum to 100 %" is an integer comparison and never a float tolerance.
//!
//! NOTE: no openEHR spec governs measured performance (CNF guide
//! `master03-overview.adoc`: "Non-functional conformance (performance, etc) is
//! not addressed by this guide") — our own design.

pub mod load;
pub mod manifest;
pub mod ops;
pub mod plan;
pub mod wire;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ids::{CorpusKey, InteractionName, QueryName, WorkloadId};
use crate::workload::ops::{QueryTransport, WorkloadOp};

/// The directory under the performance root that holds the workloads.
pub const WORKLOADS_DIR: &str = "workloads";

/// Millionths in a whole [`Share`].
const WHOLE_PPM: u32 = 1_000_000;

/// A share of one whole, exact to a millionth (`52.6%`, `0.0125%`).
///
/// Written as a percentage with at most four decimals, so every value a
/// document can spell is represented exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Share(u32);

impl Share {
    /// The whole, `100%`.
    pub const WHOLE: Share = Share(WHOLE_PPM);

    /// Returns the share in millionths of the whole.
    #[must_use]
    pub fn ppm(self) -> u32 {
        self.0
    }

    /// Returns the share from a count of millionths, or `None` above the
    /// whole.
    #[must_use]
    pub fn from_ppm(ppm: u32) -> Option<Self> {
        (ppm <= WHOLE_PPM).then_some(Self(ppm))
    }

    /// Returns the share as a fraction of one, for weighting a draw.
    #[must_use]
    pub fn fraction(self) -> f64 {
        f64::from(self.0) / f64::from(WHOLE_PPM)
    }

    /// Parses a percentage with at most four decimals, `0%` to `100%`.
    ///
    /// # Errors
    /// The text, named, when it is not such a percentage.
    pub fn parse(text: &str) -> Result<Self, String> {
        let bad =
            || format!("{text:?} is not a percentage from 0% to 100% with at most four decimals");
        let number = text.strip_suffix('%').ok_or_else(bad)?.trim();
        let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
        if whole.is_empty()
            || !whole.chars().all(|c| c.is_ascii_digit())
            || fraction.len() > 4
            || !fraction.chars().all(|c| c.is_ascii_digit())
            || (number.contains('.') && fraction.is_empty())
        {
            return Err(bad());
        }
        let whole: u32 = whole.parse().map_err(|_digits| bad())?;
        let mut padded = fraction.to_owned();
        while padded.len() < 4 {
            padded.push('0');
        }
        let fraction: u32 = padded.parse().map_err(|_digits| bad())?;
        whole
            .checked_mul(10_000)
            .and_then(|w| w.checked_add(fraction))
            .and_then(Self::from_ppm)
            .ok_or_else(bad)
    }
}

/// Returns a count of millionths as a percentage with at most four decimals,
/// including counts above the whole (a sum that overshoots).
#[must_use]
#[expect(
    clippy::integer_division,
    reason = "the whole percent of a count of millionths"
)]
pub fn ppm_percent(ppm: u64) -> String {
    let whole = ppm / 10_000;
    let fraction = ppm % 10_000;
    if fraction == 0 {
        format!("{whole}%")
    } else {
        let digits = format!("{fraction:04}");
        format!("{whole}.{}%", digits.trim_end_matches('0'))
    }
}

impl fmt::Display for Share {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&ppm_percent(u64::from(self.0)))
    }
}

impl Serialize for Share {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Share {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// Whether a workload's mix is provisional or ratified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MixStatus {
    /// Derived and awaiting SEC ratification.
    Provisional,
    /// Ratified by the SEC.
    Ratified,
}

/// The corpus scales, by name: no longer classes, only sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CorpusScale {
    /// 10,000 EHRs.
    #[serde(rename = "POC")]
    Poc,
    /// 100,000 EHRs.
    S,
    /// 1,000,000 EHRs.
    L,
    /// 10,000,000 EHRs.
    R,
}

impl CorpusScale {
    /// Returns the number of EHRs the scale seeds.
    #[must_use]
    pub fn ehrs(self) -> u64 {
        match self {
            CorpusScale::Poc => 10_000,
            CorpusScale::S => 100_000,
            CorpusScale::L => 1_000_000,
            CorpusScale::R => 10_000_000,
        }
    }

    /// Returns the scale's name.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            CorpusScale::Poc => "POC",
            CorpusScale::S => "S",
            CorpusScale::L => "L",
            CorpusScale::R => "R",
        }
    }
}

/// The corpus a run seeds before the clock starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusRecipe {
    /// The scale, by name.
    pub scale: CorpusScale,
    /// Compositions committed into each EHR.
    pub compositions_per_ehr: u32,
    /// The templates those compositions are drawn from, by payload key, with
    /// their shares.
    pub templates: BTreeMap<CorpusKey, Share>,
}

/// How interaction arrivals are spaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArrivalProcess {
    /// Exponential gaps: a Poisson process.
    #[default]
    Poisson,
    /// Even gaps, kept only as a labelled comparison.
    Uniform,
}

/// Samples per plateau a hold is sized for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SampleTarget(pub u32);

impl Default for SampleTarget {
    fn default() -> Self {
        Self(5_000)
    }
}

/// The plateau qualification limits.
///
/// The default is the owner's ruling of 2026-10-30; a workload may tighten a
/// limit and never loosen one, which the loader enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Qualification {
    /// The per-operation latency budget at the judged quantile, in
    /// milliseconds.
    pub operation_p99_ms: u32,
    /// The budget for a population AQL class, in milliseconds.
    pub population_aql_p99_ms: u32,
    /// The highest error share a plateau may carry.
    pub max_error_share: Share,
    /// The lowest achieved throughput, as a share of the offered rate.
    pub min_achieved_share: Share,
}

impl Default for Qualification {
    fn default() -> Self {
        Self {
            operation_p99_ms: 1_000,
            population_aql_p99_ms: 5_000,
            max_error_share: Share(1_000),
            min_achieved_share: Share(950_000),
        }
    }
}

/// One stage: an operation and what it carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    /// The operation.
    pub op: WorkloadOp,
    /// The payload key, for an operation that sends one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<CorpusKey>,
    /// The query class, for `aql`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<QueryName>,
}

impl Stage {
    /// Returns the key this stage is measured under: the operation token, or
    /// `aql:<class>` for a query.
    #[must_use]
    pub fn key(&self) -> OperationKey {
        match (&self.op, &self.query) {
            (WorkloadOp::Aql, Some(query)) => OperationKey(format!("aql:{query}")),
            (op, _) => OperationKey(op.token().to_owned()),
        }
    }
}

/// A partly-open session: stages on one EHR, each after the previous answer
/// and an exponential think time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    /// The mean of the exponential think time between stages, in
    /// milliseconds.
    pub think_time_mean_ms: u32,
    /// The stages, in order.
    pub stages: Vec<Stage>,
}

impl Session {
    /// Returns the expected span of the session: stages less one, times the
    /// mean think time, in milliseconds.
    #[must_use]
    pub fn expected_span_ms(&self) -> u64 {
        let gaps = u64::try_from(self.stages.len().saturating_sub(1)).unwrap_or(u64::MAX);
        gaps.saturating_mul(u64::from(self.think_time_mean_ms))
    }
}

/// What one interaction sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InteractionShape {
    /// One operation.
    Single(Stage),
    /// A session of stages.
    Session(Session),
}

/// One interaction of the mix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawInteraction", into = "RawInteraction")]
pub struct Interaction {
    /// The share of interaction arrivals.
    pub share: Share,
    /// What the interaction is, in plain words.
    pub description: String,
    /// Where the share comes from, and where it rests on derivation.
    pub derivation: String,
    /// What it sends.
    pub shape: InteractionShape,
}

impl Interaction {
    /// Returns the stages one arrival of the interaction sends.
    #[must_use]
    pub fn stages(&self) -> &[Stage] {
        match &self.shape {
            InteractionShape::Single(stage) => std::slice::from_ref(stage),
            InteractionShape::Session(session) => &session.stages,
        }
    }
}

/// The document form of an [`Interaction`]: `op` at the top for a single
/// operation, `session` for a session, never both.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawInteraction {
    share: Share,
    description: String,
    derivation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    op: Option<WorkloadOp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    payload: Option<CorpusKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    query: Option<QueryName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    session: Option<Session>,
}

impl TryFrom<RawInteraction> for Interaction {
    type Error = String;

    fn try_from(raw: RawInteraction) -> Result<Self, Self::Error> {
        let shape = match (raw.op, raw.session) {
            (Some(op), None) => InteractionShape::Single(Stage {
                op,
                payload: raw.payload,
                query: raw.query,
            }),
            (None, Some(session)) if raw.payload.is_none() && raw.query.is_none() => {
                InteractionShape::Session(session)
            }
            (None, Some(_)) => {
                return Err("a session carries its payloads and queries on its stages".to_owned());
            }
            (Some(_), Some(_)) => {
                return Err("an interaction is one operation or a session, not both".to_owned());
            }
            (None, None) => return Err("an interaction names an op or a session".to_owned()),
        };
        Ok(Self {
            share: raw.share,
            description: raw.description,
            derivation: raw.derivation,
            shape,
        })
    }
}

impl From<Interaction> for RawInteraction {
    fn from(interaction: Interaction) -> Self {
        let (op, payload, query, session) = match interaction.shape {
            InteractionShape::Single(stage) => (Some(stage.op), stage.payload, stage.query, None),
            InteractionShape::Session(session) => (None, None, None, Some(session)),
        };
        Self {
            share: interaction.share,
            description: interaction.description,
            derivation: interaction.derivation,
            op,
            payload,
            query,
            session,
        }
    }
}

/// Whether a query class reads one EHR or many.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryScope {
    /// One EHR: the class binds the interaction's EHR.
    SingleSubject,
    /// Many EHRs: the class binds no EHR scope (ITS-REST
    /// `docs/query/Request.md` §About the `ehr_id` parameter), and its budget
    /// is the population budget.
    Population,
}

/// Which part of the record a query class reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryTier {
    /// Reference-model attributes only: identifiers, times, names.
    Metadata,
    /// Archetyped content paths.
    Content,
}

/// What a refusal of a query class counts as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnRefusal {
    /// An error, like any other failed answer.
    Error,
    /// A refusal, counted in its own column and never as an error: for a
    /// class the release leaves optional.
    CountApart,
}

/// Where a query parameter's value comes from on each arrival.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamSource {
    /// The interaction's EHR id.
    Ehr,
    /// A further EHR id of the corpus, distinct from the interaction's and
    /// from every other `other_ehr` parameter of the arrival.
    OtherEhr,
    /// The uid of a composition the interaction's EHR holds.
    Composition,
    /// An integer drawn uniformly from the inclusive range.
    IntRange([i64; 2]),
}

/// One AQL query class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryClass {
    /// What a front end asks with it, in plain words.
    pub description: String,
    /// The AQL text, with `$name` parameters.
    pub text: String,
    /// One EHR or many.
    pub scope: QueryScope,
    /// Metadata or content.
    pub tier: QueryTier,
    /// Every `$name` of the text, with its source.
    #[serde(default)]
    pub parameters: BTreeMap<String, ParamSource>,
    /// The released executions that carry its arrivals, with shares.
    pub transport: BTreeMap<QueryTransport, Share>,
    /// The rows a page fetches, sent on every transport; required when the
    /// class runs as a stored `POST`, whose `Query` body requires it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetch: Option<u32>,
    /// Capability folders and option arms the class needs beyond `query`.
    #[serde(default)]
    pub requires: Vec<String>,
    /// What a refusal counts as.
    pub on_refusal: OnRefusal,
    /// Where the class's shape comes from.
    pub derivation: String,
}

/// The key an operation is measured under: an operation token, or
/// `aql:<class>`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct OperationKey(String);

impl OperationKey {
    /// Returns the key's text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OperationKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A workload document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workload {
    /// The workload id, also its file stem.
    pub id: WorkloadId,
    /// The version, raised on every change to the offered work.
    pub version: u32,
    /// Provisional or ratified.
    pub status: MixStatus,
    /// What the workload emulates, in plain words.
    pub description: String,
    /// The root of every seeded draw.
    pub seed: u64,
    /// What is seeded before the clock starts.
    pub corpus: CorpusRecipe,
    /// How interaction arrivals are spaced.
    #[serde(default)]
    pub arrival: ArrivalProcess,
    /// Samples per plateau a hold is sized for.
    #[serde(default)]
    pub sample_target: SampleTarget,
    /// The plateau qualification limits.
    #[serde(default)]
    pub qualification: Qualification,
    /// The interactions, by name.
    pub interactions: BTreeMap<InteractionName, Interaction>,
    /// The query classes, by name.
    #[serde(default)]
    pub queries: BTreeMap<QueryName, QueryClass>,
}

/// One operation of the expanded mix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MixEntry {
    /// Operation arrivals per million interaction arrivals.
    pub weight: u64,
    /// Whether the operation changes the server's state.
    pub write: bool,
}

/// The per-operation mix a workload expands to: every stage of every
/// interaction weighted by the interaction's share.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExpandedMix {
    /// The operations, by measurement key.
    pub operations: BTreeMap<OperationKey, MixEntry>,
}

impl ExpandedMix {
    /// Returns the total weight: operation arrivals per million interaction
    /// arrivals.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.operations.values().map(|e| e.weight).sum()
    }

    /// Returns the read and the write weight.
    #[must_use]
    pub fn reads_writes(&self) -> (u64, u64) {
        self.operations
            .values()
            .fold((0, 0), |(reads, writes), entry| {
                if entry.write {
                    (reads, writes + entry.weight)
                } else {
                    (reads + entry.weight, writes)
                }
            })
    }

    /// Returns whether reads to writes sit inside the 10:1 to 50:1 band.
    #[must_use]
    pub fn inside_read_band(&self) -> bool {
        let (reads, writes) = self.reads_writes();
        writes > 0
            && reads >= writes.saturating_mul(READ_BAND.0)
            && reads <= writes.saturating_mul(READ_BAND.1)
    }

    /// Returns the mean number of operations one interaction arrival sends.
    #[must_use]
    pub fn mean_operations_per_interaction(&self) -> f64 {
        ratio(self.total(), u64::from(WHOLE_PPM))
    }

    /// Returns reads per write, or `None` with no write.
    #[must_use]
    pub fn read_write_ratio(&self) -> Option<f64> {
        let (reads, writes) = self.reads_writes();
        (writes > 0).then(|| ratio(reads, writes))
    }
}

/// The read band of the population model's step 4: reads per write from 10
/// to 50.
pub const READ_BAND: (u64, u64) = (10, 50);

/// `num / den` as a float, for display and weighting only.
fn ratio(num: u64, den: u64) -> f64 {
    #[expect(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        reason = "mix weights are below 2^40, far inside the f64 mantissa"
    )]
    {
        num as f64 / den as f64
    }
}

impl Workload {
    /// Returns the expanded per-operation mix.
    #[must_use]
    pub fn expansion(&self) -> ExpandedMix {
        let mut mix = ExpandedMix::default();
        for interaction in self.interactions.values() {
            for stage in interaction.stages() {
                let entry = mix.operations.entry(stage.key()).or_insert(MixEntry {
                    weight: 0,
                    write: stage.op.is_write(),
                });
                entry.weight += u64::from(interaction.share.ppm());
            }
        }
        mix
    }

    /// Returns the capability folders and option arms an interaction needs:
    /// the folder of every operation it sends and the `requires` of every
    /// query it runs.
    #[must_use]
    pub fn requirements(&self, interaction: &Interaction) -> BTreeSet<String> {
        let mut needs = BTreeSet::new();
        for stage in interaction.stages() {
            needs.insert(stage.op.folder().to_owned());
            if let Some(class) = stage.query.as_ref().and_then(|q| self.queries.get(q)) {
                needs.extend(class.requires.iter().cloned());
            }
        }
        needs
    }

    /// Returns the longest expected session span of the workload, in
    /// milliseconds, or zero without a session.
    #[must_use]
    pub fn longest_session_span_ms(&self) -> u64 {
        self.interactions
            .values()
            .filter_map(|i| match &i.shape {
                InteractionShape::Session(session) => Some(session.expected_span_ms()),
                InteractionShape::Single(_) => None,
            })
            .max()
            .unwrap_or(0)
    }

    /// Returns the share of operation arrivals a query class receives.
    #[must_use]
    pub fn query_weight(&self, query: &QueryName) -> u64 {
        let key = OperationKey(format!("aql:{query}"));
        self.expansion()
            .operations
            .get(&key)
            .map_or(0, |entry| entry.weight)
    }

    /// Returns what a statement's claims keep of the mix: the interactions
    /// whose requirements it claims, renormalized, and the ones it drops with
    /// what each misses.
    #[must_use]
    pub fn apply_claims(&self, claimed: &Claimed) -> Applied {
        let mut kept = Vec::new();
        let mut dropped = Vec::new();
        for (name, interaction) in &self.interactions {
            let missing: Vec<String> = self
                .requirements(interaction)
                .into_iter()
                .filter(|need| !claimed.folders.contains(need) && !claimed.arms.contains(need))
                .collect();
            if missing.is_empty() {
                kept.push((name.clone(), interaction.share));
            } else {
                dropped.push(DroppedInteraction {
                    interaction: name.clone(),
                    share: interaction.share,
                    missing,
                });
            }
        }
        let kept_ppm: u64 = kept.iter().map(|(_, share)| u64::from(share.ppm())).sum();
        let kept = kept
            .into_iter()
            .map(|(name, share)| {
                let weight = if kept_ppm == 0 {
                    0.0
                } else {
                    ratio(u64::from(share.ppm()), kept_ppm)
                };
                (name, weight)
            })
            .collect();
        Applied { kept, dropped }
    }
}

/// What a statement claims, as the workload reads it: capability folders and
/// option arms.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Claimed {
    /// The claimed capability folders.
    pub folders: BTreeSet<String>,
    /// The selected option arms.
    pub arms: BTreeSet<String>,
}

impl Claimed {
    /// Returns the claims of a party statement.
    #[must_use]
    pub fn from_statement(statement: &crate::party::Statement) -> Self {
        Self {
            folders: statement
                .claims
                .capabilities
                .iter()
                .map(|c| c.as_str().to_owned())
                .collect(),
            arms: statement
                .options
                .iter()
                .map(|o| o.as_str().to_owned())
                .collect(),
        }
    }
}

/// One interaction a statement drops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedInteraction {
    /// The interaction.
    pub interaction: InteractionName,
    /// Its share before the drop.
    pub share: Share,
    /// The folders and arms the statement does not claim.
    pub missing: Vec<String>,
}

/// The mix that remains under a statement.
#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    /// The kept interactions with their renormalized weights, summing to one
    /// when anything is kept.
    pub kept: BTreeMap<InteractionName, f64>,
    /// The dropped interactions.
    pub dropped: Vec<DroppedInteraction>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_parse_exactly_and_print_back() {
        for (text, ppm, back) in [
            ("100%", 1_000_000, "100%"),
            ("52.6%", 526_000, "52.6%"),
            ("0.0125%", 125, "0.0125%"),
            ("0%", 0, "0%"),
            ("7.50%", 75_000, "7.5%"),
        ] {
            let share = Share::parse(text).expect(text);
            assert_eq!(share.ppm(), ppm, "{text}");
            assert_eq!(share.to_string(), back, "{text}");
        }
        for bad in [
            "100.0001%",
            "101%",
            "1.23456%",
            "12",
            "-1%",
            ".5%",
            "5.%",
            "a%",
        ] {
            assert!(Share::parse(bad).is_err(), "{bad} parsed");
        }
    }

    #[test]
    fn the_ruled_qualification_is_the_default() {
        let q = Qualification::default();
        assert_eq!(q.operation_p99_ms, 1_000);
        assert_eq!(q.population_aql_p99_ms, 5_000);
        assert_eq!(q.max_error_share.to_string(), "0.1%");
        assert_eq!(q.min_achieved_share.to_string(), "95%");
        assert_eq!(SampleTarget::default().0, 5_000);
    }
}
