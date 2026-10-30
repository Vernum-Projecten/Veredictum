// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

#![no_main]
//! The HDR histogram V2 decode path, which re-derives a published
//! performance record from base64 the runner did not produce.
//!
//! A `perf-result` is re-checkable precisely because every percentile comes
//! back out of the encoded histograms rather than out of the derived part
//! beside them: `perf-judge` decodes and re-reads every one. So a party
//! publishing a record hands this decoder an arbitrary byte string, which
//! makes it the one place where third-party bytes reach a binary format
//! reader.
//!
//! The harness base64-encodes the fuzzer's bytes rather than treating them as
//! base64 text: base64 decoding is a well-exercised third-party path, and
//! spending the budget discovering the alphabet would leave the V2 decoder
//! itself barely reached.
//!
//! The property is that a corrupt encoding is a typed refusal, never a panic
//! and never an allocation driven by a length the encoding claims. The decoded
//! histogram is then read the way the record's derivation reads it.

use base64::Engine;
use libfuzzer_sys::fuzz_target;

use veredictum::perf_result::hdr::decode_hdr_v2;

fuzz_target!(|data: &[u8]| {
    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
    let Ok(histogram) = decode_hdr_v2(&encoded) else {
        return;
    };

    // The quantiles a record reports, plus the count and the maximum.
    for quantile in [0.50, 0.90, 0.95, 0.99, 0.999] {
        let _ = histogram.value_at_quantile(quantile);
    }
    let _ = histogram.len();
    let _ = histogram.max();
});
