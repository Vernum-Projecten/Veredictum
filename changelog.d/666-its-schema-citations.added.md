- A citation may name a file of the ITS-JSON or ITS-XML schema bundles
  (`specs/its-json-schemas/`, `specs/its-xml-schemas/`), and a JSON Pointer
  into a JSON or YAML source may name a number, a boolean or `null`, whose
  quote is that value written as JSON and is matched by value. An ITS-JSON
  schema grounds an expectation only where every released component is
  silent on the canonical JSON form, and loses to any released text (#842).
