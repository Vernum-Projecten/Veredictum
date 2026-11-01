- Development-only cases for the committal defaults and refusals the
  development ITS-REST adds to direct commits: a COMPOSITION, EHR_STATUS or
  party create or update with no `openehr-version` header commits the
  `lifecycle_state` 532, a party delete with no `openehr-audit-details` header
  commits the change type 523, and a change type that contradicts the method
  (249 on a PUT or a DELETE, 523 on a POST) is refused with 400 at the SHOULD
  level, each refusal beside its accepted control. A released run skips them as
  `development-only`.
