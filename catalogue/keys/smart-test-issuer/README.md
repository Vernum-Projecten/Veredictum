# CNF SMART lane — the static test issuer

## THIS IS COMMITTED TEST KEY MATERIAL. IT IS NOT SECRET AND MUST NEVER BE USED IN PRODUCTION.

`cnf-smart-test.key.pem` is a **passphrase-less RSA-2048 private key that lives
in public version control**. Anyone with a checkout can mint tokens with it. It
exists for exactly one purpose: to let the CNF runner act as the
Authorization Server the SMART conformance lane needs, without the conformance
stack having to run one.

Never configure a deployment that holds real data to trust this issuer. If a
deployment ever does, every token it accepts is forgeable by anyone.

## What is here

| File | What it is |
|---|---|
| `cnf-smart-test.key.pem` | The RSA-2048 private key (PKCS#8 PEM) the runner signs RS256 access tokens with. |
| `jwks.json` | The matching **public** JWKS (`kid: cnf-smart-test`), mounted into the SUT as its trusted key set. |

The pair is generated once and committed, so a conformance run is reproducible
and needs no key-generation step. It was produced with:

```bash
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 \
  -out catalogue/keys/smart-test-issuer/cnf-smart-test.key.pem
# jwks.json is the public half: kty=RSA, kid=cnf-smart-test, alg=RS256,
# n = the modulus (base64url), e = AQAB.
```

## How the SMART lane uses it

1. `deploy/docker/e2e-ferroehr-smart.yml` brings up FerroEHR, pinned by
   digest, as the SMART resource server: it mounts `jwks.json` as the key set
   it verifies RS256 tokens with, names the issuer and audience the test
   issuer signs as, and serves the discovery document from its Platform base
   URL.
2. `deploy/docker/smart/ixit.json` declares the test issuer under
   `scoped_tokens.mint`: the issuer, the subject, the audience, the `kid`
   (`cnf-smart-test`) and the token lifetime. `veredictum test` signs one token
   per row, whose `scope` claim is the row's scope set, with this key read
   from `VEREDICTUM_SCOPED_TOKEN_KEY_FILE`.
3. `scripts/smart-lane.sh` runs every document under `catalogue/cases/smart/`
   against it, and CI's `smart` job runs that script on every pull request
   that touches those documents, this key, or the evaluator's token handling.

Spec: `specs/openehr/ITS-REST/docs/smart_app_launch/` —
`master04-service_discovery.adoc` (the discovery document),
`master08-scopes.adoc` (the resource-scope grammar),
`master06-authentication.adoc` (the CDR is a resource server: it validates
tokens and never issues them, which is why the *runner* mints here).
