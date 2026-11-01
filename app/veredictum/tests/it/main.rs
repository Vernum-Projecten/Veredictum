// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Integration tests for `veredictum`: the Arazzo catalogue's gates, its CLI,
//! its evaluator and verification pack, the results export, the schema-drift
//! guards, the measured-performance run, its records and comparisons, and the
//! wire-speaking modules against in-process fixture servers.
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
mod arazzo_extension;
mod arazzo_pack;
mod arazzo_smart;
mod badges;
mod certificate_cli;
mod content_payloads;
mod defect_rejection;
mod env_coupled;
mod fake_sut;
mod gitlab_template;
mod identity_wire;
mod perf_compare;
mod perf_corpus;
mod perf_generator;
mod perf_posture;
mod perf_result;
mod perf_run_ladder;
mod perf_server;
mod perf_views;
mod pipeline_seams;
mod results_export;
mod schema_drift;
mod spec_profile;
mod workload_gates;
mod workload_wire;
