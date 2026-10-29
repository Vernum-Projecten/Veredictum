- `veredictum test` skips every workflow of a document as `format-not-claimed`
  when it needs a wire format the statement's its-rest `tech_profiles` entry
  does not list. The format is stated in `capabilities.yaml` under
  `documents.<id>.formats`, and every catalogue document that needs a server
  to serve or accept canonical XML, Simplified Flat or Structured JSON, or the
  Web Template now carries it.
