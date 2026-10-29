// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

#![recursion_limit = "256"]
//! Veredictum, the conformance instrument for openEHR clinical data
//! repositories: the Arazzo catalogue's model, gates and evaluator, the
//! measured instruments, and JSON-Schema emission.
//!
//! The conformance oracle is the vendored released openEHR specification
//! text (`specs/openehr/`). The catalogue is Arazzo 1.1.0 workflows over the
//! released ITS-REST operations ([`arazzo`]), and every closed vocabulary is
//! a Rust enum or newtype so illegal states are unrepresentable.
//!
//! [`pipeline`] is the entry point for driving the instrument: one seam per
//! whole operation — gate and list a catalogue, run it against a system under
//! test, judge a measured record, render the published assets, measure a
//! class — each returning typed facts rather than console text. The bundled
//! `veredictum` binary is a command-line layer over exactly those seams, so
//! a second consumer drives the same engine and renders its own views.

// Doctests are copy-paste templates: they must use `?`, never unwrap
// (C-QUESTION-MARK, https://rust-lang.github.io/api-guidelines/documentation.html#c-question-mark).
#![doc(test(attr(deny(warnings))))]
#[expect(
    clippy::disallowed_types,
    reason = "dev/verification tooling over JSON artifacts (the catalogue, results, wire \
              exchanges), whose shapes belong to the artifacts and the SUT; the Arazzo model \
              carries inputs, payloads and extensions as JSON values"
)]
pub mod arazzo;
pub mod bench;
pub mod exec;
pub mod identity;
pub mod ids;
pub mod ixit;
pub mod literal;
pub mod load;
pub mod model;
pub mod party;
pub mod perf;
pub mod perf_assets;
pub mod perf_run;
pub mod perf_schedule;
pub mod pipeline;
pub mod probe;
pub mod record;
pub mod schema;
pub mod stress;
pub mod transcript;
pub mod validate;
pub mod verdict;
pub mod vocab;
