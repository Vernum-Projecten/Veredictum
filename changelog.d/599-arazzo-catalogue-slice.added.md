- An experimental Arazzo 1.1.0 catalogue slice (#599): the EHR create and
  read cases in `catalogue/cases/ehr/ehr.arazzo.yaml`, with their 400,
  404, 409 and 422 refusal twins, each criterion citing the released ITS-REST
  or RM sentence that assigns it through `x-veredictum-cite`. The library gains an
  `arazzo` module that parses the documents with `roas-arazzo` 0.3.0, gates
  them (every criterion resolves to a citation whose quote occurs in the
  vendored file, every operation resolves in its released OAS, every
  condition is inside the profile), and drives a workflow into passed,
  failed, errored or skipped with a four-part explanation. The CLI and the
  existing catalogue are unchanged.
