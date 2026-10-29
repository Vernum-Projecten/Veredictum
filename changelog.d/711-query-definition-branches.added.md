- The Arazzo catalogue covers more of the Query and stored-query surface:
  the 400 of a refused stored-query execution and version store, the
  listing by a name prefix and of every version, the dotted and unqualified
  query names, the 406 and 415 of the stored-query definition operations,
  the `openehr-ehr-id` request header on ad-hoc and stored executions, MIN
  and MAX over a String, NOT over a parenthesised conjunction, fetch beside
  a stored TOP query (on the `aql-top-supported` arm), and the AQL lexer's
  whitespace and comment rules. The 408 of a query that times out is
  recorded against upstream report #754, since the release gives a client
  no way to provoke it.
- The condition profile (`schemas/arazzo-profile.schema.json`) lets a step
  send the `openehr-ehr-id` request header its operation does not declare.
- A citation may quote a grammar rule under the AsciiDoc chapter that
  includes the grammar file, and the AQL gate no longer reads a stored-query
  body sent in another media type as AQL text.
