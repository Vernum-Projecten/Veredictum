- The case-core catalogue and the engine that drove it are deleted: the case
  cores under `artifacts/schedule/`, the operation bindings, the vocabularies,
  the corpus, the ambiguity register (#617) and every AMB id, the old
  `run` driver and its interpreter, the reference, selector and capture
  grammars, `validate --root artifacts`, and the verification pack's old
  transcript. The Arazzo catalogue under `catalogue/` is the one catalogue,
  and `catalogue/id-map.yaml` still resolves every old case id to the
  workflows that carry it.
- The `replay`, `evidence` and `conformance-assets` subcommands are removed:
  they read the old run's transcript and results. `veredictum test
  --rejudge` re-judges a recording, and the HAR recording beside a run's
  `results.json` is the triage input.
- The published schemas of the deleted formats are removed:
  `case-core`, `operation-binding`, `outcomes`, `selectors`,
  `capability-matrix`, `corpus-manifest`, `ambiguity-register`,
  `wire-surface`, `transcript`, `run-transcript` and `evidence-bundle`. The
  results schema loses `ambiguity_dispositions`, and the statement schema
  loses `served_extensions`.
- The fuzz targets of the deleted readers (`reference_grammar`,
  `artifact_yaml`, `citation`) are removed, with the vendor scripts of the
  ADL corpora nothing reads any more and the corpus-OPT reproducibility guard.
