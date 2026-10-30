// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The measured run: a workload document bound to the open-loop generator
//! and driven through the phases of a run of at most ten minutes after
//! seeding.
//!
//! [`binding`] turns a workload into the generator's mix and the limits a
//! plateau is judged by; [`run`] drives settle, warm-up, scout, drain, the
//! ladder and the closing phase against any performer; [`server`] reads the
//! declared containers and the database beside every plateau.
//!
//! No openEHR specification governs measured performance (CNF guide
//! `master03-overview.adoc`: "Non-functional conformance (performance, etc)
//! is not addressed by this guide"): our own design.

pub mod binding;
pub mod corpus;
pub mod http;
pub mod performer;
pub mod run;
pub mod server;
