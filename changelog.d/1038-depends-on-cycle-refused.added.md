- `validate` refuses a workflow whose `dependsOn` leads back to itself,
  directly, within its document or through other documents, under the new
  `arazzo-depends-on-cycle` gate. The finding names the chain, so a cycle is
  caught before a run instead of when the run reaches it.
