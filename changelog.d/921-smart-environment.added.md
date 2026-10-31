- A CDR environment in the SMART on openEHR posture,
  `deploy/docker/e2e-ferroehr-smart.yml`: FerroEHR pinned by digest as the
  resource server, a Platform base URL, and the committed static test issuer
  it trusts, with a statement and an IXIT for it under `deploy/docker/smart/`.
  `scripts/smart-lane.sh` runs every SMART document against it and refuses a
  run that does not exit `0` or that skips a SMART row; CI's `smart` job runs
  it on every pull request that touches the SMART documents, the test issuer
  or the evaluator's token handling.
