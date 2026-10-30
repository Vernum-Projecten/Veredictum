// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! What a run says about capacity: the knee, the peak achieved throughput,
//! a Universal Scalability Law fit and the population reading.
//!
//! The knee is the highest offered rate whose plateau and every lower one
//! qualify (Jain, "The Art of Computer Systems Performance Analysis", 1991,
//! ch. 3, usable capacity), stated as a bracket up to the first failing
//! plateau or as a lower bound when the top plateau qualifies. The USL
//! `X(N) = γN / (1 + α(N − 1) + βN(N − 1))` is fitted to achieved
//! throughput against mean in flight (Gunther, "Guerrilla Capacity
//! Planning", 2007; <https://arxiv.org/abs/0808.1431>) as an explanatory
//! model, never a verdict. The fit is pinned arithmetic: a fixed grid, then
//! a fixed number of Levenberg-Marquardt steps (Marquardt, SIAM J. Appl.
//! Math. 11(2), 1963) using only `+`, `−`, `×`, `÷` and a correctly rounded
//! square root, so `perf-judge` reproduces it byte for byte.
//!
//! No openEHR specification governs this: our own design.

use serde::{Deserialize, Serialize};

use crate::ixit::Environment;
use crate::perf_result::derive::{DerivedPlateau, DerivedScoutWindow};
use crate::workload::CorpusScale;

/// The Levenberg-Marquardt steps the fit takes, always all of them.
pub const USL_ITERATIONS: u32 = 200;

/// The fewest points the fit is made from: one more than its parameters.
pub const USL_MIN_POINTS: usize = 4;

/// The peak API arrivals per second per 100,000 population, low and high.
pub const ARRIVALS_PER_100K: [f64; 2] = [13.0, 75.0];

/// The derivation the population reading rests on.
pub const POPULATION_DERIVATION: &str = "About 46 clinical documents per capita per year \
     (consultations, discharges and ED visits from OECD Health at a Glance 2023 and Eurostat; \
     laboratory results from RCPath; imaging from the NHS Diagnostic Imaging Dataset 2023/24; \
     prescription items from NHS BSA PCA 2024/25) give 0.15 writes per second per 100,000 \
     population; a peak factor of 8 (the ITU-T E.500 busy hour) and a read multiplier of 10:1 \
     to 50:1 (OLTP-Bench and YCSB; PMC10148376) give 13 to 75 peak API arrivals per second per \
     100,000. The knee divided by that band is the population a deployment of this shape could \
     serve at its peak, read only at the corpus scale and on the environment the run used.";

/// The knee of a ladder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "bound", rename_all = "snake_case", deny_unknown_fields)]
pub enum Knee {
    /// The knee lies between a qualifying plateau and the failing one above
    /// it.
    Bracket {
        /// The highest offered rate that qualifies with every lower one.
        qualified_rate: f64,
        /// Its achieved throughput, per second.
        achieved_per_s: f64,
        /// The first failing plateau's offered rate.
        failing_rate: f64,
    },
    /// Every plateau qualifies: the knee lies above the top one.
    LowerBound {
        /// The top plateau's offered rate.
        qualified_rate: f64,
        /// Its achieved throughput, per second.
        achieved_per_s: f64,
    },
    /// The lowest plateau fails: the knee lies below the ladder.
    BelowLadder {
        /// The lowest plateau's offered rate.
        failing_rate: f64,
    },
    /// The run has no plateau.
    NoPlateau,
}

impl Knee {
    /// The qualified rate, when the ladder found one.
    #[must_use]
    pub fn qualified_rate(&self) -> Option<f64> {
        match self {
            Knee::Bracket { qualified_rate, .. } | Knee::LowerBound { qualified_rate, .. } => {
                Some(*qualified_rate)
            }
            Knee::BelowLadder { .. } | Knee::NoPlateau => None,
        }
    }
}

/// Reads the knee off plateaus in any order, by offered rate.
#[must_use]
pub fn knee(plateaus: &[DerivedPlateau]) -> Knee {
    let mut ordered: Vec<&DerivedPlateau> = plateaus.iter().collect();
    ordered.sort_by(|a, b| a.planned_rate.total_cmp(&b.planned_rate));
    let mut best: Option<&DerivedPlateau> = None;
    for plateau in ordered {
        if !plateau.qualification.qualified {
            return match best {
                Some(q) => Knee::Bracket {
                    qualified_rate: q.planned_rate,
                    achieved_per_s: q.throughput.achieved_per_s,
                    failing_rate: plateau.planned_rate,
                },
                None => Knee::BelowLadder {
                    failing_rate: plateau.planned_rate,
                },
            };
        }
        best = Some(plateau);
    }
    match best {
        Some(q) => Knee::LowerBound {
            qualified_rate: q.planned_rate,
            achieved_per_s: q.throughput.achieved_per_s,
        },
        None => Knee::NoPlateau,
    }
}

/// Where the peak achieved throughput was read (a closed vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeakSource {
    /// A plateau's hold.
    Plateau,
    /// A scout window.
    Scout,
}

/// The highest achieved throughput of the run, whatever its latency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Peak {
    /// The achieved throughput, per second.
    pub achieved_per_s: f64,
    /// Where it was read.
    pub source: PeakSource,
    /// The plateau or scout window, by its position.
    pub index: usize,
}

/// Reads the peak achieved throughput over the plateaus and the scout
/// windows; the first of equal values wins, plateaus before windows.
#[must_use]
pub fn peak(plateaus: &[DerivedPlateau], scout: &[DerivedScoutWindow]) -> Option<Peak> {
    let candidates = plateaus
        .iter()
        .enumerate()
        .map(|(i, p)| (p.throughput.achieved_per_s, PeakSource::Plateau, i))
        .chain(
            scout
                .iter()
                .enumerate()
                .map(|(i, w)| (w.throughput.achieved_per_s, PeakSource::Scout, i)),
        );
    let mut best: Option<Peak> = None;
    for (achieved_per_s, source, index) in candidates {
        if achieved_per_s > 0.0
            && best
                .as_ref()
                .is_none_or(|b| achieved_per_s > b.achieved_per_s)
        {
            best = Some(Peak {
                achieved_per_s,
                source,
                index,
            });
        }
    }
    best
}

/// A fitted Universal Scalability Law.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum UslReading {
    /// The fit was made.
    Fitted {
        /// Contention.
        alpha: f64,
        /// Coherency.
        beta: f64,
        /// Throughput at one in flight, per second.
        gamma: f64,
        /// The concurrency of the predicted maximum, `√((1 − α) / β)`;
        /// absent when `β` is zero or `α` reaches one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        n_star: Option<f64>,
        /// The predicted maximum throughput, `X(N*)`, per second.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        predicted_max_per_s: Option<f64>,
        /// The share of throughput variance the fit explains.
        r_squared: f64,
        /// The points fitted.
        points: usize,
        /// The Levenberg-Marquardt steps taken.
        iterations: u32,
    },
    /// Too few points to fit.
    NotFitted {
        /// The points available.
        points: usize,
        /// Why no fit was made.
        reason: String,
    },
}

/// The USL's throughput at concurrency `n`.
fn usl(n: f64, gamma: f64, alpha: f64, beta: f64) -> f64 {
    gamma * n / (1.0 + alpha * (n - 1.0) + beta * n * (n - 1.0))
}

/// The least-squares `γ` for fixed `α` and `β`, and the residual sum.
fn gamma_for(points: &[(f64, f64)], alpha: f64, beta: f64) -> (f64, f64) {
    let (mut fx, mut ff) = (0.0, 0.0);
    for (n, x) in points {
        let f = usl(*n, 1.0, alpha, beta);
        fx += f * x;
        ff += f * f;
    }
    let gamma = if ff > 0.0 { fx / ff } else { 0.0 };
    (gamma, sse(points, gamma, alpha, beta))
}

fn sse(points: &[(f64, f64)], gamma: f64, alpha: f64, beta: f64) -> f64 {
    points
        .iter()
        .map(|(n, x)| {
            let r = x - usl(*n, gamma, alpha, beta);
            r * r
        })
        .sum()
}

/// Solves the 3×3 system `a · x = b` by Gaussian elimination with partial
/// pivoting; `None` for a singular system.
fn solve3(mut a: [[f64; 3]; 3], mut b: [f64; 3]) -> Option<[f64; 3]> {
    for col in 0..3 {
        let pivot =
            (col..3).max_by(|i, j| row(&a, *i, col).abs().total_cmp(&row(&a, *j, col).abs()))?;
        if row(&a, pivot, col).abs() <= f64::MIN_POSITIVE {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        let head = *a.get(col)?;
        let head_b = *b.get(col)?;
        for r in (col + 1)..3 {
            let factor = row(&a, r, col) / head.get(col).copied()?;
            let target = a.get_mut(r)?;
            for (cell, h) in target.iter_mut().zip(head.iter()) {
                *cell -= factor * h;
            }
            *b.get_mut(r)? -= factor * head_b;
        }
    }
    let mut x = [0.0; 3];
    for r in (0..3).rev() {
        let mut sum = *b.get(r)?;
        for c in (r + 1)..3 {
            sum -= row(&a, r, c) * x.get(c).copied()?;
        }
        *x.get_mut(r)? = sum / row(&a, r, r);
    }
    Some(x)
}

fn row(a: &[[f64; 3]; 3], r: usize, c: usize) -> f64 {
    a.get(r).and_then(|v| v.get(c)).copied().unwrap_or(0.0)
}

/// The grid's best start: residual, `γ`, `α` and `β`.
fn grid_start(points: &[(f64, f64)]) -> (f64, f64, f64, f64) {
    let mut alphas = vec![0.0];
    let mut alpha_step = 1e-4;
    for _ in 0..14 {
        alphas.push(alpha_step);
        alpha_step *= 2.0;
    }
    let mut betas = vec![0.0];
    let mut beta_step = 1e-8;
    for _ in 0..30 {
        betas.push(beta_step);
        beta_step *= 2.0;
    }
    let mut best = (f64::INFINITY, 0.0, 0.0, 0.0);
    for alpha in alphas.iter().copied().filter(|a| *a < 1.0) {
        for beta in &betas {
            let (gamma, residual) = gamma_for(points, alpha, *beta);
            if residual < best.0 {
                best = (residual, gamma, alpha, *beta);
            }
        }
    }
    best
}

/// Fits the USL to `(N, X)` points: mean in flight against achieved
/// throughput.
///
/// A grid over `α ∈ {0} ∪ {10⁻⁴ · 2ᵏ, k = 0…13}` and `β ∈ {0} ∪
/// {10⁻⁸ · 2ᵏ, k = 0…29}`, each with its least-squares `γ`, picks the start
/// with the smallest residual (the first on a tie); then
/// [`USL_ITERATIONS`] Levenberg-Marquardt steps refine all three, with `α`
/// held in `[0, 1)` and `β` at or above zero.
#[must_use]
pub fn fit_usl(points: &[(f64, f64)]) -> UslReading {
    let points: Vec<(f64, f64)> = points
        .iter()
        .copied()
        .filter(|(n, x)| n.is_finite() && x.is_finite() && *n > 0.0 && *x > 0.0)
        .collect();
    if points.len() < USL_MIN_POINTS {
        return UslReading::NotFitted {
            points: points.len(),
            reason: format!(
                "the fit needs at least {USL_MIN_POINTS} points with work in flight and a \
                 positive throughput"
            ),
        };
    }
    let best = grid_start(&points);
    let (mut current, mut gamma, mut alpha, mut beta) = best;
    let mut lambda = 1e-3;
    for _ in 0..USL_ITERATIONS {
        let mut jtj = [[0.0; 3]; 3];
        let mut jtr = [0.0; 3];
        for (n, x) in &points {
            let d = 1.0 + alpha * (n - 1.0) + beta * n * (n - 1.0);
            let model = gamma * n / d;
            let r = x - model;
            let j = [
                n / d,
                -gamma * n * (n - 1.0) / (d * d),
                -gamma * n * n * (n - 1.0) / (d * d),
            ];
            for (i, ji) in j.iter().enumerate() {
                if let Some(slot) = jtr.get_mut(i) {
                    *slot += ji * r;
                }
                for (k, jk) in j.iter().enumerate() {
                    if let Some(cell) = jtj.get_mut(i).and_then(|v| v.get_mut(k)) {
                        *cell += ji * jk;
                    }
                }
            }
        }
        let mut damped = jtj;
        for i in 0..3 {
            if let Some(cell) = damped.get_mut(i).and_then(|v| v.get_mut(i)) {
                *cell += lambda * row(&jtj, i, i).max(f64::MIN_POSITIVE);
            }
        }
        let Some([dg, da, db]) = solve3(damped, jtr) else {
            lambda *= 10.0;
            continue;
        };
        let next = (
            gamma + dg,
            (alpha + da).clamp(0.0, 1.0 - f64::EPSILON),
            (beta + db).max(0.0),
        );
        let residual = sse(&points, next.0, next.1, next.2);
        if residual < current {
            (gamma, alpha, beta) = next;
            current = residual;
            lambda /= 10.0;
        } else {
            lambda *= 10.0;
        }
    }
    let count = crate::perf_result::quantile::u64_to_f64(u64::try_from(points.len()).unwrap_or(0));
    let mean = points.iter().map(|(_, x)| x).sum::<f64>() / count;
    let total: f64 = points.iter().map(|(_, x)| (x - mean) * (x - mean)).sum();
    let n_star = (beta > 0.0 && alpha < 1.0).then(|| ((1.0 - alpha) / beta).sqrt());
    UslReading::Fitted {
        alpha,
        beta,
        gamma,
        n_star,
        predicted_max_per_s: n_star.map(|n| usl(n, gamma, alpha, beta)),
        r_squared: if total > 0.0 {
            1.0 - current / total
        } else {
            1.0
        },
        points: points.len(),
        iterations: USL_ITERATIONS,
    }
}

/// The USL points of a run: every scout window and plateau with work in
/// flight, as `(mean in flight, achieved throughput)`.
#[must_use]
pub fn usl_points(plateaus: &[DerivedPlateau], scout: &[DerivedScoutWindow]) -> Vec<(f64, f64)> {
    scout
        .iter()
        .map(|w| (w.throughput.mean_in_flight, w.throughput.achieved_per_s))
        .chain(
            plateaus
                .iter()
                .map(|p| (p.throughput.mean_in_flight, p.throughput.achieved_per_s)),
        )
        .collect()
}

/// The population a knee could serve at its peak, read at the run's corpus
/// scale and environment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum PopulationReading {
    /// The knee gives a reading.
    Read {
        /// The knee's offered rate, operation arrivals per second.
        knee_rate: f64,
        /// Whether the knee is a lower bound, so the band is too.
        lower_bound: bool,
        /// The peak arrivals per second per 100,000 population the band
        /// divides by, low and high.
        arrivals_per_100k: [f64; 2],
        /// The population band, smallest first: the knee over the high
        /// rate, then over the low one.
        population: [f64; 2],
        /// The corpus scale the run seeded.
        corpus_scale: CorpusScale,
        /// The EHRs that scale names.
        corpus_ehrs: u64,
        /// The environment the run ran on, as the IXIT declares it.
        environment: String,
        /// What the band rests on.
        derivation: String,
    },
    /// No plateau qualified, so there is no knee to read.
    NotRead {
        /// Why not.
        reason: String,
    },
}

/// Reads the population band of `knee` at `scale` on `environment`.
#[must_use]
pub fn population(knee: &Knee, scale: CorpusScale, environment: &Environment) -> PopulationReading {
    let Some(knee_rate) = knee.qualified_rate() else {
        return PopulationReading::NotRead {
            reason: "no plateau qualified, so the run has no knee to read".to_owned(),
        };
    };
    let [low, high] = ARRIVALS_PER_100K;
    PopulationReading::Read {
        knee_rate,
        lower_bound: matches!(knee, Knee::LowerBound { .. }),
        arrivals_per_100k: ARRIVALS_PER_100K,
        population: [knee_rate / high * 100_000.0, knee_rate / low * 100_000.0],
        corpus_scale: scale,
        corpus_ehrs: scale.ehrs(),
        environment: format!(
            "{}, {} cores, {} GiB, {} storage, {}",
            environment.hardware_class,
            environment.cores,
            environment.memory_gb,
            environment.storage_class,
            environment.topology
        ),
        derivation: POPULATION_DERIVATION.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic(alpha: f64, beta: f64, gamma: f64) -> Vec<(f64, f64)> {
        [
            1.0, 2.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0, 32.0, 48.0, 64.0, 96.0, 128.0,
        ]
        .iter()
        .map(|n| (*n, usl(*n, gamma, alpha, beta)))
        .collect()
    }

    fn within(found: f64, wanted: f64) -> bool {
        (found - wanted).abs() <= 0.05 * wanted.abs()
    }

    #[test]
    fn the_fit_recovers_known_alpha_and_beta_within_five_percent() {
        for (alpha, beta, gamma) in [
            (0.05, 0.000_5, 120.0),
            (0.02, 0.000_1, 900.0),
            (0.2, 0.002, 40.0),
        ] {
            let UslReading::Fitted {
                alpha: a,
                beta: b,
                gamma: g,
                r_squared,
                ..
            } = fit_usl(&synthetic(alpha, beta, gamma))
            else {
                panic!("no fit");
            };
            assert!(within(a, alpha), "alpha {a} for {alpha}");
            assert!(within(b, beta), "beta {b} for {beta}");
            assert!(within(g, gamma), "gamma {g} for {gamma}");
            assert!(r_squared > 0.99, "{r_squared}");
        }
    }

    #[test]
    fn the_fit_is_the_same_bytes_every_time() {
        let points = synthetic(0.05, 0.000_5, 120.0);
        let a = serde_json::to_string(&fit_usl(&points)).unwrap();
        let b = serde_json::to_string(&fit_usl(&points)).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn too_few_points_are_not_fitted() {
        assert!(matches!(
            fit_usl(&[(1.0, 10.0), (2.0, 19.0), (0.0, 5.0)]),
            UslReading::NotFitted { points: 2, .. }
        ));
    }
}
