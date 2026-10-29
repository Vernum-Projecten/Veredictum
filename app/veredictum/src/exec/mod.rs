// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The wire and judgement building blocks the Arazzo evaluator and the
//! measured instruments share.
//!
//! The HTTP transport, the credentials it presents, the pure assertion
//! evaluators the judges wrap, version-signature verification, result-set
//! comparison, and the content recipes and OPT synthesis a data-driven row
//! commits through.

pub mod assertions;
pub mod auth;
pub mod content_synth;
pub mod documents;
pub mod opt_synth;
pub mod recipes;
pub mod resultset;
pub mod signature;
pub mod transport;
