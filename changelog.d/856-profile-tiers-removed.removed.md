- **Breaking for statement authors and certificate readers:** the statement's
  `claims.profiles` member and its tier vocabulary (`CORE`, `STANDARD`,
  `OPTIONS`, `SEC-BASIC`, `D`, `M`, `X`) are gone from
  `schemas/statement.schema.json`, and no certificate carries a tier or family
  rating. Nothing selected, gated or judged by them. A statement that still
  lists `profiles` is refused by the schema, and `validate --statement` names
  the field. Remove the member; a result already says what it covers, as BASE
  and OPTIONS chapter by chapter.
