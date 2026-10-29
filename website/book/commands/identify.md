Prints what the server states about itself through the ITS-REST System
`OPTIONS` operation, and exits `0` whatever the server answered: a server that
does not offer the document is within the specification, which says services
SHOULD answer. It exits `2` only when no answer arrived at all, or when the
credential variable `--auth` names is unset.

Run it before `test` to see which product and version a run against the same
base URL is about to grade.
