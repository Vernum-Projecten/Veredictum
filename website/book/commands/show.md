`show` prints a `results.json` in the terminal, the same view
[`test`](#test) prints when a run finishes.

Each line is one group of rows: a 24-cell bar and the count of each mark.
Every mark with a row gets at least one cell, so one failure among a thousand
passes still shows. Cells differ in shape as well as colour: `█` passed, `▓`
failed, `▒` errored, `░` warning, `·` skipped.

```text
BASE
  Every BASE case                           ████████████▓▓▓▓▓▓▓▓░░░░  ✓   3  ✕   2  ?   0  !   1  ○   0
  EHR                                       ████████████▓▓▓▓▓▓▓▓░░░░  ✓   3  ✕   2  ?   0  !   1  ○   0
    COMPOSITION                             ████████████▓▓▓▓▓▓░░░░░░  ✓   2  ✕   1  ?   0  !   1  ○   0
    DIRECTORY                               no cases
```

- **BASE:** each chapter and band in catalogue order.
- **OPTIONS:** the same chapters and bands, one line per option with the
  selected arm and its kind: Recommended (SHOULD), Optional (MAY), or
  `@SpecDiverge` for an option that exists only because the released text is
  unclear or contradictory. An option with no arm selected is listed once and
  its rows count nowhere. Cases citing text the release marks TRIAL are listed
  in their band here, never in a BASE count.
- **Closing note:** counts the `@SpecDiverge` cases and names the upstream
  reports they are judged by. Report numbers come from the catalogue's
  committed snapshot; nothing is fetched.

Warning rows are counted apart from failures, and `--failed` lists them under
their own heading after the failed and errored rows.

**Colour.** `--color auto`, the default, colours only when standard output is
a terminal and `NO_COLOR` is unset or empty (<https://no-color.org/>).
`--color always` and `--color never` override both, so
`veredictum show results.json --color always | less -R` keeps colour.

**A performance record.** Given a `perf-result.json` (it names its `family`),
`show` prints, within 80 columns and only from the record:

- a header: target, environment, workload and digest, corpus, between-run
  noise and the scope of one run;
- a line per plateau: offered and achieved rates, p50 to max, error share,
  `✓` when it qualifies and `✕` when it fails, `unsteady` where the halves
  disagree;
- the operations at the knee and at the lowest plateau, with each one's
  judged quantile (`n/r`, not resolvable, below 10 samples above it);
- the knee, peak, population and fit sentences;
- a sparkline of p99 against offered rate, and the server-side readings.
