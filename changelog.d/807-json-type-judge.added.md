- The `x-veredictum-check` extension gains the `json_type` judge, which
  asserts that a value is one of the six JSON types of RFC 8259 §3; `validate`
  refuses any other type name. The SMART discovery case
  `smart_discovery/smart_configuration-document` now asserts that
  `capabilities` is an array.
