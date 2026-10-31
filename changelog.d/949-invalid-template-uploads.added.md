- Nine template upload cases adjudicate the invalid templates. A compact valid
  ADL 1.4 operational template is accepted, and six copies of it that break the
  published ITS-XML Template.xsd in one place each (no template_id, no
  concept, no definition, the concept or the template_id twice, a description
  with no lifecycle_state) are refused with 400, the only refusal the upload
  declares. Two ADL 2 templates that break AOM2 VCORM and VCARM are refused
  with 400 against the minimal template as control. The largest published OPT
  is recorded in the id map with its vendored payload and provenance.
