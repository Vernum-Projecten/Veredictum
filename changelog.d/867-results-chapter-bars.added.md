- The console results page draws the outcome as chapter bars above the rows:
  one shaded header per chapter and one stacked bar per capability folder,
  with passed, failed, advisory, errored and skipped counts beside each bar.
  The chart is server-rendered SVG in the Okabe-Ito palette, with a glyph
  for every outcome, and `?view=options` draws the option tiers instead of
  BASE. Each bar links to its rows, and the rows are grouped the same way:
  BASE chapter, folder and section, then RECOMMENDED, OPTIONS with the rows
  resting on TRIAL text, and the rows resting on text reported upstream as
  defective. Each row shows its formalism, TRIAL and report marks, and an
  advisory row has its own stat card and is never counted as failed.
