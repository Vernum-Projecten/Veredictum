- A rows file may say `server: exclusive` without `rows`, so a workflow that
  declares no inputs can carry the marker; it then runs once. `validate`
  still refuses a row for such a workflow, and refuses a row-less file whose
  workflow requires an input no generated or declared value binds
  (`schemas/arazzo-rows.schema.json`).
