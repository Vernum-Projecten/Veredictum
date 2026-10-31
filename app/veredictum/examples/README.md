<!--
SPDX-FileCopyrightText: Veredictum contributors
SPDX-License-Identifier: Apache-2.0
-->

# Examples

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
