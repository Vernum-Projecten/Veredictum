- `veredictum test` refuses an empty credential variable the way it refuses
  an unset one. A `VEREDICTUM_SUT_*` or `VEREDICTUM_ADMIN_*` variable, or its
  `_FILE` form, set to the empty string counts as unset, so a run whose CI
  secret does not exist stops with exit status `2` naming the variable instead
  of presenting an empty credential to the server.
