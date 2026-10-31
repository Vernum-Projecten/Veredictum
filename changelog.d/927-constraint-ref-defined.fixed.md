- The ADL 1.4 template `cnf.tpl.dv_coded_text_constraint_ref` defines the
  constraint code `ac0001` its `CONSTRAINT_REF` uses and binds it to a
  SNOMED-CT query, in a `component_ontologies` entry for the OBSERVATION
  archetype. It used the code without defining it, which ADL 1.4 rule VACDF
  forbids, so a server checking the rule was right to refuse the upload.
  Integration tests now hold every ADL 1.4 content template to that rule,
  and the composition the content cases commit to the archetype ids and
  at-codes of every template it is committed against.
- The ADL 2 template uploads, `definition/template_adl2/upload_template_adl2-minimal`
  and every setup in `_setup/content_templates_adl2`, name upstream report
  #939: they follow the OPT2 grammar, which has no `specialize` section,
  while the AOM2 class model requires every operational template to be
  specialised.
