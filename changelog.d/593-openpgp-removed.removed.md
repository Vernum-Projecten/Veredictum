- The OpenPGP-signed run record: `verdicts --sign-key` and its passphrase
  variable `VEREDICTUM_SIGN_PASSPHRASE`, the `verify-record` subcommand, and the
  library's `record` module. A conformance certificate replaces it. OpenPGP
  stays only where a server under test signs its versions with it, which the
  instrument verifies.
