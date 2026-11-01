- An option in `catalogue/capabilities.yaml` may carry `profile: development`,
  and the published capabilities schema accepts no other value. Under the
  development profile it is an option like any other: its documents are
  skipped as `option-not-selected` when the statement selects another arm,
  and tally with the OPTIONS. Under the released profile the run reads the
  catalogue without it, so its documents stay BASE, and a statement answering
  it is accepted and selects nothing. The new option `adl2-id-coded` uses it:
  development AM requires openEHR systems to "support at least the at-coded
  ADL2 archetypes", so under the development profile the id-coded ADL 2
  documents (the 26 `content/adl2_*` documents, `template_adl2`, `flat_adl2`
  and the ADL 2 template examples) test the arm
  `adl2-id-coded-supported`. The ADL 2 workflows of the six
  `template_example_*` documents move to their own `template_example_*_adl2`
  documents, so their case ids now carry the `_adl2` document name. The five
  `template_adl2` workflows that touch no id-coded material (the listing, the
  three reads of an unknown template_id and the upload declared as XML) move
  to the BASE document `template_adl2_endpoint` and stay BASE under both
  profiles (#1021).
