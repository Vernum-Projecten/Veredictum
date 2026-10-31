- `content/adl2_dv_coded_text` carries the three DV_CODED_TEXT 400
  refusals (no `defining_code`, no `terminology_id`, no `code_string`) and a
  422 refusal of a code outside the value set, against an ADL 2 template
  whose `defining_code` matches a value set the template defines itself
  (ADL2 `value_sets`). Their accepted twin commits a member of that value
  set, which no external terminology service has to resolve, so the
  refusals have a control that holds under either arm of
  `terminology-binding-unresolvable`. ADL 1.4 has no such binding: AOM 1.4
  places the constraint of a `CONSTRAINT_REF` in a query into an external
  service, so the ADL 1.4 refusals keep their accepted-arm control.
