<!--
SPDX-FileCopyrightText: Veredictum contributors
SPDX-License-Identifier: Apache-2.0
-->

# Test fixtures

Test material, at the repository root because both crates' test suites and the
browser-journey script read it by path. **Nothing here is a declaration about
any product, and no gate reads it as one.**

## Why a declaration lives under a fixtures tree

ISO/IEC 9646-7 splits an ICS in exactly one place: "All table cells should be
completed by the ICS proforma specifier, except the support and supported
values columns, which need to be completed by the supplier of the
implementation." The form is this repository's to author: the capability
folders under `catalogue/cases/` are its rows, and `catalogue/capabilities.yaml`
lists each option with the arms a supplier picks from.

The support columns are the supplier's. ISO/IEC 17050-1 is titled *supplier's*
declaration of conformity, so a filled-in declaration written by the test
instrument is a contradiction in terms. A real declaration therefore travels
with the run it describes: pasted into a console run, or passed to the command
line as `--statement`.

What the test suites still need is the SHAPE of a filled-in declaration, so the
static conformance review can be seen to fire. That is what
`declaration/statement.json` is, and its product does not exist.

| Path | What it is |
|---|---|
| `declaration/statement.json` | A filled-in ICS for a product that does not exist, claiming every row of the capability matrix. The static conformance review (`veredictum validate --statement`) runs over it. |
| `declaration/ixit.json` | The ixit beside it, exercising every deployment posture the runner reads. Every address in it is fictional. |
| `openpgp/subkey.sig.asc`, `openpgp/primary.sig.asc` | Two armored RFC 9580 detached signatures over the 32 bytes `ferroehr-cnf subkey verification`, made once with the committed version-signing secret key: one by its signing subkey, one by its primary key. The version-signature verifier is tested against them, so its tests sign nothing. |
| `certificates/test-signing.key.pem` | **A committed Ed25519 private key, so it is not secret and must never sign a real certificate.** PKCS#8 PEM, made with `openssl genpkey -algorithm ed25519`. The certificate tests in both crates issue with it. |
| `certificates/jwks.json` | Its public half as a JWK Set, `kid` `veredictum-test`. No published set ever carries this key, so a certificate it signed never verifies against `deploy/hosted/jwks.json`, and a test asserts exactly that. |

The key material the ixit names, the OpenPGP version-signing pair and the
SMART lane's test issuer, is catalogue material and lives under
`catalogue/keys/`.
