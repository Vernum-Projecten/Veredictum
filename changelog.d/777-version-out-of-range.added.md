- `veredictum test` skips every workflow of a document as `version-out-of-range`
  when the statement's `spec_versions` declares a version outside the
  document's `applies` range, a new `documents.<id>.applies` entry of
  `capabilities.yaml` with an inclusive `min`/`max` and the released sentence
  that bounds it. `validate` refuses a range naming no document, an unknown
  component, or a start after its end.
