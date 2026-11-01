- Every place that follows a workflow's `dependsOn` resolves it the same way,
  by the canonical path of the setup document. The `canonical-json` needs gate
  follows a setup that lives outside `catalogue/cases/` and no longer mistakes
  one setup for another file of the same name. A run resolves a setup
  document's own `$sourceDescriptions` entries against that document, reads the
  scopes of a setup in another document that shares a workflow id with a local
  one, and refuses a `dependsOn` cycle through a `type: arazzo` source as a
  cycle. The committed catalogue validates and judges exactly as before.
