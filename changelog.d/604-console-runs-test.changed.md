- The web console runs the Arazzo catalogue. A run started from the console
  drives `veredictum test` over the catalogue the console is configured with
  (`VEREDICTUM_ROOT`, now `catalogue` by default and `/work/catalogue` in the
  image), passing the base URL, the statement and the IXIT, with the
  credentials in the spawned process's environment only. The catalogue
  explorer lists capability folders, their Arazzo documents and each
  workflow's citation and steps; the Scope screen composes a claim from
  capability folders and filters by document id; and the results screen reads
  `results.json` and `recording.har`, showing a red row's four-part
  explanation beside the exchange it was judged on. The "record the wire
  exchanges" choice is gone, because every run writes its recording.
