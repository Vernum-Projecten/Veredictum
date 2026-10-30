`show` reads the `results.json` the Arazzo evaluator writes and prints it in
the terminal, so a result can be read without exporting it first.
[`test`](#test) prints the same view when a run finishes.

Each line is one group of rows, with a bar of 24 cells and the count of each
mark. The bar gives every mark that has a row at least one cell, so a single
failure among a thousand passes still shows. The cells differ in shape as well
as colour: `█` passed, `▓` failed, `▒` errored, `░` advisory, `·` skipped.

```text
BASE
  Every BASE case                           ████████████▓▓▓▓▓▓▓▓░░░░  ✓   3  ✕   2  ?   0  !   1  ○   0
  EHR                                       ████████████▓▓▓▓▓▓▓▓░░░░  ✓   3  ✕   2  ?   0  !   1  ○   0
    COMPOSITION                             ████████████▓▓▓▓▓▓░░░░░░  ✓   2  ✕   1  ?   0  !   1  ○   0
    DIRECTORY                               no cases
```

The groups are the ones every view of a result uses. BASE lists each chapter
and band in the order the catalogue declares. RECOMMENDED and OPTIONS list one
line per option with the arm the statement selected; an option with no arm
selected is listed once and its rows count nowhere. The cases whose citations
rest on text the release marks TRIAL are listed under OPTIONS, never in a BASE
count. The last part lists the options that exist only because the released
text is defective, and every case resting on text an upstream report records,
grouped by report, with the report's title and state from the snapshot the
catalogue commits. Nothing is fetched while the view is printed.

An advisory row is a SHOULD the server did not follow. It is counted apart
from the failed rows, and `--failed` lists it under its own heading after the
failed and errored rows, never among them.

**Colour.** With `--color auto`, the default, the view is coloured only when
standard output is a terminal and `NO_COLOR` is unset or empty, as
<https://no-color.org/> asks. `--color always` and `--color never` override
both, so `veredictum show results.json --color always | less -R` keeps the
colour.
