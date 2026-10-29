- The YAML reader behind every catalogue artifact, workload file and supplied
  declaration is `serde-saphyr` 2.0.0 (#729). A tag that selects a unit enum
  variant now refuses a non-null payload instead of discarding it, and only an
  empty value, `~`, `null`, `Null` and `NULL` read as null, so a spelling such
  as `NulL` is a string. A document that relied on either leniency now fails
  to load with a typed error.
