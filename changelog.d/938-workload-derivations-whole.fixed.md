- The reference workload `clinical-mix` is v2: thirteen derivations written
  as plain YAML scalars ended at the ` #` of "(removed in #892)", which YAML
  reads as a comment, so the loaded workload, `veredictum perf-workloads` and
  the published workload page carried them cut mid-sentence. They are block
  scalars now and load whole, the shares are unchanged, and a test refuses a
  pinned workload whose description or derivation is cut.
