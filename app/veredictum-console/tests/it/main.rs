// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The console's integration suite: one binary, one module per topic.
//!
//! Cargo compiles and links every top-level `tests/*.rs` as its own crate
//! (<https://doc.rust-lang.org/cargo/reference/cargo-targets.html#integration-tests>),
//! so one binary saves the link waste while nextest still runs each test in
//! its own process.

#[cfg(feature = "ssr")]
mod body_limits;
#[cfg(feature = "ssr")]
mod bundle_names;
#[cfg(feature = "ssr")]
mod captures;
#[cfg(feature = "ssr")]
mod certificates_http;
#[cfg(feature = "ssr")]
mod connect_forms;

mod e2e_console;
#[cfg(feature = "ssr")]
mod engine_gate;

mod hosted_posture;
#[cfg(feature = "ssr")]
mod read_surfaces;
#[cfg(feature = "ssr")]
mod run_live;
#[cfg(feature = "ssr")]
mod run_scope;
#[cfg(feature = "ssr")]
mod run_start;
#[cfg(feature = "ssr")]
mod server_fns;
