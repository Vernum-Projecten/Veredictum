- A GitLab CI template, `ci/gitlab/veredictum.yml`, runs the instrument in one
  `include: remote:` pinned to a release tag, with typed `spec: inputs`
  matching the GitHub Action's. Its run job pulls the release image of that
  tag, writes the results, the views and the badges into `out-dir`, keeps
  them as artifacts and publishes JUnit as the merge request's test report;
  its verdict job fails exactly as the Action's step does, `fail-on-inconclusive`
  included. Credentials are masked CI/CD variables read by name.
  `scripts/release/check-gitlab-template.sh` holds the template's image to the
  engine version and the version being released, and a test validates the
  template against GitLab's own CI schema, vendored under
  `ci/gitlab/schema/`. The book gains a GitLab CI page, and the Badges page
  uses the template.
