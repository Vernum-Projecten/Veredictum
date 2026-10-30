- Every catalogue document that sends `application/json`, itself or through a
  setup it names in `dependsOn`, now states `canonical-json` in
  `capabilities.yaml`, so a party whose statement claims only canonical XML
  skips it as `format-not-claimed`; `validate` refuses a document that sends
  JSON without stating the format. The XML option documents whose workflows
  need different formats are split, so a document-level format holds for
  each of its workflows.
