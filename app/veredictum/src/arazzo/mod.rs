// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The Arazzo 1.1.0 catalogue format, its gate and its evaluator.
//!
//! A document holds workflows over the released ITS-REST OpenAPI operations.
//! The gate refuses an uncited or out-of-profile criterion, and the evaluator
//! drives every workflow, records each exchange as HAR 1.2 and classifies
//! each row. The documents parse with `roas-arazzo`
//! (<https://docs.rs/roas-arazzo/0.3.0>); the condition profile, the
//! citation check and every classification are this crate's own code.
//! Arazzo 1.1.0 is <https://spec.openapis.org/arazzo/v1.1.0.html>.

pub mod capabilities;
pub mod catalogue;
pub mod check;
pub mod cite;
pub mod control;
pub mod credential;
pub mod depends;
pub mod derive;
pub mod document;
pub mod evaluate;
pub mod export;
pub mod expr;
pub mod extension;
pub mod gates;
pub mod har;
pub mod maturity;
pub mod oas;
pub mod platform;
pub mod principal;
pub mod publication;
pub mod reading;
pub mod recipe;
pub mod regex_profile;
pub mod results;
pub mod rows;
pub mod scope;
pub mod source;
pub mod spec_profile;
pub mod structure;
pub mod terminal;
pub mod xpath;
