- The console runs in trial mode unless `VEREDICTUM_CERTIFICATES=issue` is set:
  runs, results and verdicts work, every page's status banner says no
  certificate is issued, and no signing key is read. With issuance on, it
  refuses to start on a key its published JWK Set does not carry, and each
  finished run gets `certificate.jws`, downloadable from its results page. The
  console serves that set at `/.well-known/jwks.json`, byte for byte the
  committed `deploy/hosted/jwks.json`.
