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

pub mod baselines;
pub mod binding;
pub mod canary;
pub mod client;
pub mod corpus;
pub mod fingerprint;
pub mod http;
pub mod jitter;
pub mod payload;
pub mod performer;
pub mod posture;
pub mod run;
pub mod server;

/// FNV-1a over a stream seed plus the caller's index parts: the deterministic
/// draw shared by the corpus template draws, the performer's target picks and
/// the payload jitter, each carrying its own seed so the streams never
/// correlate.
///
/// The parameters and the 64-bit offset basis are the reference ones
/// (<http://www.isthe.com/chongo/tech/comp/fnv/index.html>).
pub(crate) fn fnv1a(seed: u64, parts: &[u64]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325 ^ seed;
    for part in parts {
        for byte in part.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}
