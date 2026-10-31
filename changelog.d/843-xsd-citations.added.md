- A citation into an ITS-XML schema (`.xsd`) names one top-level
  `xs:element`, `xs:complexType` or `xs:simpleType` by its `name` in
  `section`, optionally after its kind (`complexType COMPOSITION`), and its
  quote must occur inside that definition. `validate` refuses an unknown or
  ambiguous name and a quote outside the named definition.
