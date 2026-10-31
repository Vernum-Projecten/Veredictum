- The evaluator judges `xpath` criteria: XPath 3.1 over the response body
  parsed as XML, holding when the Effective Boolean Value of the result is
  true (Arazzo 1.1.0 §XPath Conditions). The prefixes `oe`, `oe2` and `xsi`
  are bound to the ITS-XML v1 and v2 namespaces and the XML Schema instance
  namespace, `validate` compiles every condition, and an unparsable
  condition, a body that is not well-formed XML or a dynamic error leaves the
  row errored. The engine is `oxixml-xpath` 0.1.2. `ResponseView` gains the
  `text` field carrying the body as received.
