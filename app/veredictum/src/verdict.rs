// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The verdict pipeline — a **pure** function of (statement, results,
//! performance cases).
//!
//! Verdicts are computed, never asserted: the same inputs always produce the
//! same [`VerdictReport`]. A measured class verdict is re-derived from the
//! measurement records' embedded histograms against the performance
//! schedule's case, and the performance claim is reviewed: a claimed class
//! needs measured evidence in a described deployment, a measurement needs its
//! case, and a stored verdict that does not re-derive is flagged.
//!
//! Conformance verdicts over the Arazzo catalogue are the rows of
//! `veredictum test`'s results document, and its exit status.

#![allow(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT; the carriers \
              here are cfg(test)-only, so #[expect] would be unfulfilled in the non-test build"
)]

use serde::Serialize;

use crate::ids::CaseId;
use crate::party::{Results, Statement};
use crate::perf::{ClassVerdict, PerfClass, PerformanceCase, class_verdict};

/// One static-review problem with the claim (never a per-case verdict).
#[derive(Debug, Clone, Serialize)]
pub struct ReviewFinding {
    /// Human-readable description of the illegal or incomplete claim.
    pub message: String,
}

/// One measured performance-class verdict.
///
/// The second machinery (§8.11 step 5): re-derived from the measurement's
/// decoded histograms against the catalogue case's thresholds — the stored
/// verdict is never trusted.
#[derive(Debug, Clone, Serialize)]
pub struct PerformanceVerdict {
    /// The performance case measured.
    pub case: CaseId,
    /// The volumetric class the case belongs to.
    pub class: PerfClass,
    /// Whether the statement claims this class.
    pub claimed: bool,
    /// The re-derived verdict.
    pub verdict: ClassVerdict,
    /// The named threshold violations behind a `not-earned` verdict.
    pub violations: Vec<String>,
}

/// The computed verdict report.
#[derive(Debug, Clone, Serialize)]
pub struct VerdictReport {
    /// Static-review findings against the performance claim and the
    /// measurement records (empty ⇒ the claim is legal and every stored
    /// verdict re-derives).
    pub review: Vec<ReviewFinding>,
    /// The measured performance-class verdicts (one per measurement in the
    /// results; empty when the campaign carried no measured runs).
    pub performance: Vec<PerformanceVerdict>,
}

/// Compute the verdict report from a statement and its results against the
/// performance schedule's cases. Pure and total.
#[must_use]
pub fn compute(
    statement: &Statement,
    results: &Results,
    perf_cases: &[PerformanceCase],
) -> VerdictReport {
    let mut review = Vec::new();
    // A performance claim requires an environment reference: a class is only
    // earned in a described deployment.
    if let Some(perf) = &statement.performance
        && perf.environment_ref.trim().is_empty()
    {
        review.push(ReviewFinding {
            message: format!(
                "performance class {:?} is claimed without an environment reference",
                perf.class
            ),
        });
    }
    let performance = measured_verdicts(statement, results, perf_cases, &mut review);
    VerdictReport {
        review,
        performance,
    }
}

/// Re-derive every measured class verdict from the measurement records
/// (decoded histograms, never the summary fields) against the catalogue's
/// performance cases, and statically review the performance claim: a claimed
/// class needs measured evidence, a measurement needs its catalogue case,
/// and a stored verdict that does not re-derive is flagged (tamper check).
fn measured_verdicts(
    statement: &Statement,
    results: &Results,
    perf_cases: &[PerformanceCase],
    review: &mut Vec<ReviewFinding>,
) -> Vec<PerformanceVerdict> {
    let claimed_class: Option<PerfClass> = match &statement.performance {
        None => None,
        Some(claim) => match PerfClass::parse(&claim.class) {
            Ok(class) => Some(class),
            Err(e) => {
                review.push(ReviewFinding {
                    message: format!("performance claim: {e}"),
                });
                None
            }
        },
    };

    let mut verdicts = Vec::new();
    for measurement in &results.measurements {
        let Some(case) = perf_cases.iter().find(|c| c.id == measurement.case) else {
            review.push(ReviewFinding {
                message: format!(
                    "measurement references performance case {} not in the catalogue",
                    measurement.case
                ),
            });
            continue;
        };
        match class_verdict(
            case,
            measurement.offered_load_sustained,
            &measurement.operations,
        ) {
            Ok((verdict, violations)) => {
                if verdict != measurement.verdict {
                    review.push(ReviewFinding {
                        message: format!(
                            "measurement {}: stored verdict does not re-derive from the \
                             embedded histograms (stored {:?}, derived {verdict:?})",
                            measurement.case, measurement.verdict
                        ),
                    });
                }
                verdicts.push(PerformanceVerdict {
                    case: measurement.case.clone(),
                    class: case.class,
                    claimed: claimed_class == Some(case.class),
                    verdict,
                    violations,
                });
            }
            Err(e) => review.push(ReviewFinding {
                message: format!("measurement {}: {e}", measurement.case),
            }),
        }
    }

    if let Some(class) = claimed_class
        && !verdicts.iter().any(|v| v.class == class)
    {
        review.push(ReviewFinding {
            message: format!(
                "performance class {} is claimed but the results carry no measurement for it",
                class.token()
            ),
        });
    }
    verdicts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn statement() -> Statement {
        serde_json::from_value(serde_json::json!({
            "product": { "name": "p", "version": "1", "vendor": "v", "identifier": "i" },
            "schedule_release": "CNF-2.0",
            "claims": { "capabilities": ["ehr"] }
        }))
        .unwrap()
    }

    fn results() -> Results {
        serde_json::from_value(serde_json::json!({
            "sut": { "name": { "value": "s", "source": "operator" }, "version": { "value": "1", "source": "operator" } },
            "runner": { "name": "veredictum", "version": "0", "verification_pack_status": "passed" },
            "instrument_status": "pre-approval",
            "schedule_release": "CNF-2.0",
            "tech_profile": { "its": "its-rest", "formats": ["canonical-json"] },
            "ixit_digest": "d",
            "outcomes": []
        }))
        .unwrap()
    }

    fn messages(report: &VerdictReport) -> Vec<&str> {
        report.review.iter().map(|f| f.message.as_str()).collect()
    }

    fn statement_with_performance(class: &str, environment_ref: &str) -> Statement {
        let mut st = statement();
        st.performance = Some(crate::party::Performance {
            class: class.to_owned(),
            environment_ref: environment_ref.to_owned(),
        });
        st
    }

    /// The performance ladder is closed, so a class token outside it is a
    /// review finding rather than a silently dropped claim. A claim with no
    /// environment reference is meaningless in the same pass: a class is only
    /// earned in a described deployment.
    #[test]
    fn a_malformed_performance_claim_is_reviewed_on_both_of_its_parts() {
        let report = compute(&statement_with_performance("XL", "  "), &results(), &[]);
        assert!(
            messages(&report).contains(&"performance claim: unknown performance class \"XL\""),
            "{:?}",
            report.review
        );
        assert!(
            messages(&report)
                .iter()
                .any(|m| m.contains("is claimed without an environment reference")),
            "{:?}",
            report.review
        );
    }

    /// A claimed class with no measurement carrying it has no evidence at all,
    /// so the claim is reviewed rather than quietly unverdicted.
    #[test]
    fn a_claimed_class_with_no_measurement_is_a_review_finding() {
        let report = compute(&statement_with_performance("POC", "env-1"), &results(), &[]);
        assert_eq!(
            messages(&report),
            ["performance class POC is claimed but the results carry no measurement for it"]
        );
    }

    /// One measured record over a class-POC workload; `verdict` is what the
    /// results FILE stored, which the pipeline re-derives rather than trusts.
    fn measurement(stored_verdict: &str, operation: &str) -> crate::perf::Measurement {
        serde_json::from_value(serde_json::json!({
            "case": "PERF-hospital_sim-class_POC",
            "class": "POC",
            "verdict": stored_verdict,
            "offered_load_sustained": 2.0,
            "duration_s": 3600,
            "warmup_s": 300,
            "environment": { "hardware_class": "laptop", "cores": 8, "memory_gb": 16,
                              "storage_class": "nvme", "topology": "single-node" },
            "operations": [
                { "operation": operation, "requests": 10, "errors": 0,
                   "latency_ms_p50": 1.0, "latency_ms_p90": 2.0, "latency_ms_p99": 3.0,
                   "hdr_v2_base64": "" }
            ],
            "violations": []
        }))
        .unwrap()
    }

    /// A class-POC case whose only bound is the offered-load floor, so the
    /// verdict re-derives without decoding a histogram.
    fn perf_case(threshold: &serde_json::Value) -> PerformanceCase {
        serde_json::from_value(serde_json::json!({
            "id": "PERF-hospital_sim-class_POC",
            "kind": "performance", "component": "PERFORMANCE",
            "description": "d", "test_purpose": "t", "spec_refs": [],
            "class": "POC", "corpus": "cnf.scale.10k",
            "workload": {
                "arrival_rate": "2/s", "warmup": "PT5M", "duration": "PT1H",
                "journeys": { "chart_review": "100%" }
            },
            "thresholds": [threshold]
        }))
        .unwrap()
    }

    fn results_with(measurement: crate::perf::Measurement) -> Results {
        let mut rs = results();
        rs.measurements = vec![measurement];
        rs
    }

    /// A measurement naming a case the catalogue does not carry cannot be
    /// re-derived against anything, so it is reviewed rather than accepted on
    /// its stored verdict.
    #[test]
    fn a_measurement_without_its_catalogue_case_is_a_review_finding() {
        let report = compute(
            &statement(),
            &results_with(measurement("earned", "ehr_create")),
            &[],
        );
        assert_eq!(
            messages(&report),
            [
                "measurement references performance case PERF-hospital_sim-class_POC not in the catalogue"
            ]
        );
        assert!(report.performance.is_empty());
    }

    /// The stored verdict is never trusted: it is re-derived from the case's
    /// thresholds and the measurement, and a disagreement is named with both
    /// values. This is the tamper check the published record rests on.
    #[test]
    fn a_stored_verdict_that_does_not_re_derive_is_flagged() {
        let case = perf_case(&serde_json::json!({ "metric": "offered_load_sustained", "min": 2 }));
        let report = compute(
            &statement(),
            &results_with(measurement("not-earned", "ehr_create")),
            std::slice::from_ref(&case),
        );
        assert!(
            messages(&report).iter().any(|m| m.contains(
                "stored verdict does not re-derive from the embedded histograms (stored NotEarned, derived Earned)"
            )),
            "{:?}",
            report.review
        );
        assert_eq!(report.performance.len(), 1);

        // An agreeing record re-derives silently.
        let report = compute(
            &statement(),
            &results_with(measurement("earned", "ehr_create")),
            std::slice::from_ref(&case),
        );
        assert!(report.review.is_empty(), "{:?}", report.review);
    }

    /// A threshold scoped to an operation the run never drove cannot be
    /// judged, so the re-derivation fails loudly instead of returning a
    /// verdict computed over the wrong population.
    #[test]
    fn a_threshold_over_an_unmeasured_operation_fails_the_re_derivation() {
        let case = perf_case(
            &serde_json::json!({ "metric": "latency_p99", "operation": "aql_query", "max": 1000 }),
        );
        let report = compute(
            &statement(),
            &results_with(measurement("earned", "ehr_create")),
            &[case],
        );
        assert_eq!(
            messages(&report),
            [
                "measurement PERF-hospital_sim-class_POC: threshold references unmeasured operation aql_query"
            ]
        );
        assert!(report.performance.is_empty());
    }
}
