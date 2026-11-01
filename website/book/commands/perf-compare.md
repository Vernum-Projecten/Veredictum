Two GitHub-hosted runs of the same code have read up to 156 % apart, so
numbers from separate runs say little about two systems. An interleaved run
puts both sides on one host in one window: each rung of the shared ladder runs
A then B, the next B then A, so host drift lands on both sides.

- **`--aa`:** the same target on both sides, measuring what interleaving
  leaves. Its floor, the largest ratio per metric, is attached to both records
  and turns their `between_run_noise` from `unmeasured` into the floor.
- **`--ab` with `--floor`:** pass an A/A record or its `perf-compare.json`, and
  a ratio must clear the floor as well as `--tolerance` to read as a
  difference.
- **`--result`:** records measured apart were offered different rates. The
  table still lines them up, with ratios shown as `different rates`, unjudged.
