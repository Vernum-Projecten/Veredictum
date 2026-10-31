- `smart_scopes/template_list-system_scope_granted` no longer fails a `403`
  for the `system/template-*.r` and `.crud` rows: no released text says
  whether a template list needs `r` or `s` (upstream report #758), so either
  the list or a `403` passes and any other status fails. The new
  `smart_scopes/template_list-system_scope_read_search_granted` requires the
  list for `.rs` and `.cruds`, which hold both permissions.
