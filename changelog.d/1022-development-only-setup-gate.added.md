- `validate` refuses a workflow that rests on released text and depends,
  through `dependsOn` directly or through other setups, in its own document
  or another, on a development-only workflow, under the new
  `arazzo-development-setup` gate. The finding names both workflows and the
  development citation. A run refuses such a catalogue the same way, so a
  released verdict never rests on a setup grounded in development text alone.
