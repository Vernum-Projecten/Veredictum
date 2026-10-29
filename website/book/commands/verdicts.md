The pure step for a measured campaign. It reaches no network and reads nothing
but its inputs, which is what makes a published verdict re-derivable by anyone
who has the same three: the statement, the results record carrying the
measurements, and the performance schedule under `perf/`.

Each measured class verdict is re-derived from the measurement record's
embedded histograms against its performance case, and a stored verdict that
does not re-derive is a review finding. A claimed class with no measurement,
or a claim with no environment reference, is a review finding too. The command
writes `verdicts.json` and exits `1` when the review found anything.

Conformance verdicts over the Arazzo catalogue are `veredictum test`'s results
document and its exit status.
