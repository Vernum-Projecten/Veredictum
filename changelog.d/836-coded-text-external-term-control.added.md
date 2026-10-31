- `content/dv_coded_text_ext_term_accepted` commits a complete external
  DV_CODED_TEXT term (`SNOMED-CT::254626006`, "adenocarcinoma of lung", the
  code and rubric the BASE terminology overview writes) on the template of
  the three external-term refusals, and expects 201 under the accepted arm of
  `terminology-binding-unresolvable`. It is the control of those refusals,
  whose rows were recorded errored while they had none. A run that declares
  the refused arm runs no control for them, so their rows stay errored there.
