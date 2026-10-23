// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The Arazzo 1.1.0 catalogue format, its gate and its evaluator.
//!
//! A document holds workflows over the released ITS-REST OpenAPI operations.
//! The gate refuses an uncited or out-of-profile criterion, and the evaluator
//! drives a workflow and classifies it. The documents parse with `roas-arazzo`
//! (<https://docs.rs/roas-arazzo/0.3.0>); the condition profile, the
//! citation check and every classification are this crate's own code.
//! Arazzo 1.1.0 is <https://spec.openapis.org/arazzo/v1.1.0.html>.
//!
//! This is the first slice of the catalogue format: it drives `simple` and
//! `regex` criteria over JSON exchanges, and the gate refuses every construct
//! it does not drive yet.

pub mod cite;
pub mod document;
pub mod evaluate;
pub mod expr;
pub mod gates;
pub mod source;
