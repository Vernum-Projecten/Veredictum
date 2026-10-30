- The console results page draws the outcome as chapter bars above the rows:
  one shaded header per chapter and one stacked bar per capability folder,
  with passed, failed, advisory, errored and skipped counts beside each bar.
  The chart is server-rendered SVG in the Okabe-Ito palette, with a glyph
  for every outcome, and `?view=options` draws OPTIONS instead of BASE, read
  by chapter and folder with each option and the TRIAL rows indented under
  their folder. An option row carries its kind after its name, Recommended
  (SHOULD) or Optional (MAY), and an option of the spec-boundary tier the
  `@SpecDiverge` tag. Each bar links to its rows, and the rows are grouped
  the same way. Each row shows its formalism and TRIAL marks, and an advisory
  row has its own stat card and is never counted as failed.
