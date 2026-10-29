Prints what the server states about itself through the ITS-REST System
`OPTIONS` operation, and exits `0` whatever the server answered: a server that
does not offer the document is within the specification, which says services
SHOULD answer. It exits `2` only when no answer arrived at all, or when the
credential `--auth` names is not supplied. It reads the credential exactly as
`test` does ([Logging in to the CDR](authentication.md)), `_FILE` forms included.

Run it before `test` to see which product and version a run against the same
base URL is about to grade.

The document it prints has an `outcome` of `offered`, `not_offered` or
`not_options_document`, and `schemas/identity.schema.json` describes it, so a
script reading it has a schema to validate against.
