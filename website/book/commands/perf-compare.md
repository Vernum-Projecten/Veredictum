A single run carries sampling intervals and nothing else. Two GitHub-hosted
runs of the same code have read up to 156 % apart, so a number from one run
set beside a number from another says little about the two systems. An
interleaved run puts both sides on one host in one window: each rung of the
shared ladder runs A then B, the next rung B then A, so a drift in the host
during the run lands on both sides.

`--aa` measures what that interleaving leaves: the same target on both sides.
Its floor, the largest ratio per metric, is attached to both records and turns
their `between_run_noise` from `unmeasured` into the floor. Pass that record or
its `perf-compare.json` as `--floor` to an `--ab` run, and a ratio has to clear
the floor as well as `--tolerance` before it reads as a difference.

With `--result`, the records were measured apart, so their plateaus were
offered different rates. The table still lines them up, and the ratios are
shown as `different rates` rather than judged.
