// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Integration tests for `veredictum`: the Arazzo catalogue's gates, its CLI,
//! its evaluator and verification pack, the results export, the schema-drift
//! guards, the measured-performance driver, the universal-benchmark engine and
//! its published pack manifest, and the wire-speaking modules against
//! in-process fixture servers.
//!
//! One binary per crate, split into topic modules: Cargo compiles and links
//! every top-level `tests/*.rs` as its own crate
//! (<https://doc.rust-lang.org/cargo/reference/cargo-targets.html#integration-tests>),
//! so one binary saves the link waste while nextest still runs each test in
//! its own process.

#![expect(
    clippy::disallowed_types,
    reason = "an independently authored wire input catches codec bugs a typed-then-serialized value cannot, so fixtures and wire assertions are raw JSON"
)]

mod aql_examples;
mod arazzo_catalogue;
mod arazzo_cli;
mod arazzo_derive;
mod arazzo_exclusive;
mod arazzo_pack;
mod arazzo_smart;
mod bench_engine;
mod bench_manifest;
mod bench_posture;
mod certificate_cli;
mod defect_rejection;
mod env_coupled;
mod fake_sut;
mod identity_wire;
mod measured_wire;
mod perf_driver;
mod perf_generator;
mod perf_result;
mod perf_seeding;
mod pipeline_seams;
mod results_export;
mod schema_drift;
mod workload_gates;
mod workload_wire;
