Prints what the server states about itself through the ITS-REST System
`OPTIONS` operation. Run it before `test` to see which product and version a
run against that base URL will grade.

- **Exit `0`:** whatever the server answered, since the specification says
  services SHOULD answer and a server may decline.
- **Exit `2`:** no answer arrived, or the credential `--auth` names is not
  supplied. Credentials are read as `test` reads them
  ([Logging in to the CDR](authentication.md)), `_FILE` forms included.

The printed document's `outcome` is `offered`, `not_offered`,
`not_options_document` or `invalid_status`, and
`schemas/identity.schema.json` describes it.
