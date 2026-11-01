- A citation's `development` member may be `{ contested: <report> }`, naming
  the upstream report that records a contradiction the development snapshot
  holds against the unchanged cited text. Under the development profile its
  workflow is skipped with the new reason `contested-in-development`; the
  released profile judges it as before. `validate --profile development`
  counts the contested cases, and the published extension and results schemas
  carry the new form and reason. `composition_delete`
  `delete_composition-lifecycle_deleted` is the first contested case (#1008).
