- The JUnit XML export validates against the schemas the tools that read it
  publish: the document against the Jenkins xUnit plugin's `junit-10.xsd` and
  each `<testsuite>` against Apache Maven Surefire's test report schema. Every
  `<testcase>` now carries the `time` Surefire requires, and the
  `<testsuites>` root no longer carries the `skipped` attribute junit-10 does
  not define. A suite carries its place in the result as `eu.veredictum.*`
  `<properties>`, and a CI gate checks the pinned views with `xmllint`.
- The CTRF export follows CTRF 0.2.0 §4.6.1: everything Veredictum adds sits
  in `extra` under the one namespaced key `eu.veredictum`. Each test carries
  its place in the result in the standard `suite` path and `labels`, and its
  `type` is `conformance`.
