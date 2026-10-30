<!--
SPDX-FileCopyrightText: Veredictum contributors
SPDX-License-Identifier: Apache-2.0
-->

# Examples

## `results.example.json`

The committed EXAMPLE results document (#48) — an explicit example, never a
conformance record: the SUT is `example-cdr`, the runner's verification-pack
status is `not_run`, and every number is synthetic. It exists because the
repository committed no results record at all, so the `party_document` and
`hdr_v2` fuzz targets started from mutations instead of a real record, and
readers of the results schema had no instance to look at.

It is generated, not hand-written: `make_example_results.rs` builds it through
the crate's own machinery — the outcome records satisfy
`Results::check_invariants`, the embedded histograms are real HdrHistogram V2
encodings from the serializer the runner uses, and the measurement verdict is
COMPUTED by `veredictum::perf::class_verdict` against the real catalogue's POC
case (it comes out `not-earned` with its violation named, which is the more
instructive branch to show). The document validates against
`schemas/results.schema.json`.

Regenerate with:

```shell
cargo run --example make_example_results
```

The output is deterministic — a re-run is byte-identical. `fuzz/seeds.sh`
links the document into the `party_document` seed set and decodes its embedded
histogram into the `hdr_v2` one.

## `perf-result.example.json`

The committed EXAMPLE `perf-result` record (#888): an explicit example, never
a measurement. The target is `example-cdr` at `cdr.example`, the workload is a
`custom` document, and every latency is synthetic. The raw part is built from
real HdrHistogram V2 encodings through the crate's own record types, and the
derived part is computed from it by `PerfResult::from_raw`, so
`veredictum perf-judge` reproduces it byte for byte. The scout breaches near
200 operations per second, three of the four plateaus qualify, the 0.9
plateau is unsteady, a 1 % operation is judged at p50, and the failing
plateau's database series is truncated. The golden tests of `show`,
`export --markdown` and `perf-assets` render it.

Regenerate with:

```shell
cargo run --example make_example_perf_result
```

The output is deterministic, and `scripts/checks/example-results-drift.sh`
fails when the committed copy is not what the generator writes.
