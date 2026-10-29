- The console asks for an optional administrative credential beside the server
  credential and passes it to `veredictum test` through `--admin-auth` and the
  `VEREDICTUM_CDR_ADMIN_*` variables, so a console run can drive the
  administrative workflows. Without one it passes `--admin-auth none`, and the
  IXIT it writes carries no `admin` instance instead of repeating the server
  credential as a second principal.
