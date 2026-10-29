---
name: access-control-is-delegated
description: an expect-forbidden row over a role/principal boundary has no released-component ground — ITS-REST puts authorization at SHOULD
metadata:
  type: project
---

A case whose expectation is "principal X is refused with 403 on operation Y"
has NO released-spec ground, so `expected forbidden, observed ok/not_found`
is a CATALOGUE defect (upstream-report candidate), never a SUT defect.

**Why:** ITS-REST
`specifications/docs/overview/Requests_and_responses.md` §Authentication and
authorization is SHOULD-strength ("Services SHOULD implement and support an
HTTP Authentication and Authorization framework"), and its MUST is
conditional and only about WHICH code to use ("If authentication and
authorization are required, services MUST properly use the
`WWW-Authenticate` … returning `403 Forbidden`, `401 Unauthorized` …"). The
§HTTP status codes table merely DEFINES 403 as "The service understood the
request but refuses to authorize it". (The unreleased SM, no longer vendored
since #604, delegated the choice outright.)

**How to apply:** this bites hardest on extension routes, where "no openEHR
spec governs this — our own design/extension" and the status choice is ours. Also check the case's
premise is DECLARED: a `-forbidden` row calling itself "the non-administrative
principal" needs the party to declare an administrative principal too,
otherwise the separation it asserts is unverifiable for that deployment.
