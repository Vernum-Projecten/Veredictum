// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The histogram encoding a record embeds.
//!
//! It is the standard `HdrHistogram` V2 serialization, base64-wrapped
//! (`hdrhistogram` 7.5 `serialization`,
//! <https://docs.rs/hdrhistogram/7.5.4/hdrhistogram/serialization/>).
//!
//! No openEHR specification governs this: our own design.

use base64::Engine as _;
use hdrhistogram::Histogram;
use hdrhistogram::serialization::{Deserializer, Serializer as _, V2Serializer};

/// Encodes a recorded histogram in the V2 serialization, base64-wrapped.
///
/// # Errors
/// Returns a message on serialization failure.
pub fn encode_hdr_v2(histogram: &Histogram<u64>) -> Result<String, String> {
    let mut buffer = Vec::new();
    V2Serializer::new()
        .serialize(histogram, &mut buffer)
        .map_err(|e| format!("hdr serialize: {e}"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&buffer))
}

/// Decodes a base64-wrapped V2 serialization.
///
/// # Errors
/// Returns a message on a corrupt encoding.
pub fn decode_hdr_v2(encoded: &str) -> Result<Histogram<u64>, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| format!("hdr base64: {e}"))?;
    Deserializer::new()
        .deserialize(&mut bytes.as_slice())
        .map_err(|e| format!("hdr decode: {e}"))
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a Result-returning test in the Book ch11 shape that also asserts; \
              clippy offers no allow-in-tests knob for this lint"
)]
mod tests {
    use super::*;

    #[test]
    fn a_histogram_round_trips_through_its_encoding() -> Result<(), String> {
        let mut histogram = Histogram::<u64>::new(3).map_err(|e| e.to_string())?;
        for value in [1_u64, 10, 100, 1_000, 10_000] {
            histogram.record(value).map_err(|e| e.to_string())?;
        }
        let decoded = decode_hdr_v2(&encode_hdr_v2(&histogram)?)?;
        assert_eq!(decoded, histogram);
        assert!(decode_hdr_v2("not base64!").is_err());
        Ok(())
    }
}
