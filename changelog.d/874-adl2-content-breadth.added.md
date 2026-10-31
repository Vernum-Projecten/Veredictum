- ADL 2 counterparts of the remaining ADL 1.4 content templates: 49 more
  ADL 2 operational templates in `_setup/content_templates_adl2`, and 78
  content workflows over them in 22 new `content/adl2_*` documents and the
  existing `adl2_dv_count` and `adl2_dv_interval_count`. Every template has
  an accepted case, and every constraint an ADL 2 template carries where
  the ADL 1.4 case refuses has its 422 refusal, citing the ADL2 cADL text
  for it. An integration test holds every inline ADL 2 composition to
  the published ITS-JSON schema: a 400 refusal carries a body the schema
  refuses, and every other commit a body it admits.
