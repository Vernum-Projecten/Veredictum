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
pub mod check;
pub mod cite;
pub mod derive;
pub mod document;
pub mod evaluate;
pub mod export;
pub mod expr;
pub mod gates;
pub mod har;
pub mod oas;
pub mod principal;
pub mod recipe;
pub mod regex_profile;
pub mod results;
pub mod rows;
pub mod source;
