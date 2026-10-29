- The quickstart compose file moved from `docker/docker-compose.yml` to
  `deploy/docker/docker-compose.yml`, so its raw download URL is now
  `https://raw.githubusercontent.com/Vernum-Projecten/Veredictum/main/deploy/docker/docker-compose.yml`.
  The image's `Dockerfile` and the browser-journey compose files
  (`e2e-ehrbase.yml`, `e2e-ferroehr.yml` and its `e2e-ferroehr.toml`) moved
  with it, and a local image build names `deploy/docker/Dockerfile`. The
  `docker-compose.yml` attached to each release keeps its name.
- The OpenPGP version-signing test pair moved from `fixtures/keys/` to
  `catalogue/keys/openpgp/`, and the SMART lane's test issuer from
  `fixtures/smart-test-issuer/` to `catalogue/keys/smart-test-issuer/`. An
  IXIT that names the issuer's key file by path names the new one.
- The design and governance documents (`ARCHITECTURE.md`,
  `ASSURANCE_CASE.md`, `GOVERNANCE.md`, `MAINTAINERS.md` and
  `AI_STATEMENT.md`) moved to `docs/`. The Trivy configuration and its
  ignore file moved to `security/trivy.yaml` and `security/trivyignore.yaml`,
  the hadolint configuration to `deploy/docker/hadolint.yaml`, and the
  mdbook-lint configuration to `website/book/mdbook-lint.toml`.
