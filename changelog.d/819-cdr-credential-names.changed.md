- The credential variables of `veredictum test`, `veredictum identify` and the
  GitHub Action name the CDR: `VEREDICTUM_SUT_USER`, `VEREDICTUM_SUT_PASSWORD`
  and `VEREDICTUM_SUT_TOKEN` are now `VEREDICTUM_CDR_USER`,
  `VEREDICTUM_CDR_PASSWORD` and `VEREDICTUM_CDR_TOKEN`, and the administrative
  principal's `VEREDICTUM_ADMIN_USER`, `VEREDICTUM_ADMIN_PASSWORD` and
  `VEREDICTUM_ADMIN_TOKEN` are now `VEREDICTUM_CDR_ADMIN_USER`,
  `VEREDICTUM_CDR_ADMIN_PASSWORD` and `VEREDICTUM_CDR_ADMIN_TOKEN`. Each keeps
  its `_FILE` form. No released version read the old names, so they have no
  deprecated alias. A missing credential names the flag, the mode and the
  variables it reads (`--auth basic needs VEREDICTUM_CDR_USER and
  VEREDICTUM_CDR_PASSWORD (or their _FILE forms)`), the help of `test` and
  `identify` states the three login modes, and the book has a page on logging
  in to the CDR.
- `veredictum identify` reads its credential exactly as `test` does, so the
  `_FILE` forms and the empty-is-unset rule apply to it too.
