- The Arazzo catalogue carries SMART on openEHR in a `smart` capability folder
  a statement may leave unclaimed, since the specification is in the
  DEVELOPMENT state (#745). `smart_discovery` reads the SMART configuration
  document from the Platform base URL and checks its `application/json` media
  type, the required `org.openehr.rest` service and that service's absolute
  `baseUrl`; `smart_scopes` reads the ADL 1.4 template list with a
  `system/template-*` read scope per row. The refusal of a scope that does not
  grant an operation is not carried, because the released text assigns it no
  status (upstream report #758).
- `veredictum test --platform-url` names the SMART Platform base URL, which
  can differ from `--base-url`; a source whose root says
  `x-veredictum-base: platform` is sent there, and without the flag its
  workflows are skipped as `not-declared` (#745).
- A step's `x-veredictum-scope` names the scope set of the access token it
  presents, so each row can carry its own (#745). The IXIT's `scoped_tokens`
  member supplies a token per scope set from a `VEREDICTUM_SCOPED_TOKEN_<NAME>`
  variable or its `_FILE`, or declares a test issuer whose RSA key in
  `VEREDICTUM_SCOPED_TOKEN_KEY` mints one. Only the credential transport reads
  the tokens; the results and the recording never hold one. A declared
  variable that is not set stops the run with exit status 2, and a row whose
  scope set is not covered is skipped as `not-declared`. The published
  `arazzo-extensions` schema gains the `scope` definition, and the published
  IXIT schema the `scoped_tokens` member.
