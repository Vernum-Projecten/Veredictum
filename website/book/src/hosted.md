# The hosted instrument

<!-- toc -->

[console.veredictum.eu](https://console.veredictum.eu) drives the catalogue
against the endpoint you name and shows the full results and verdicts.
Functional conformance only; `perf` and `perf-compare` run locally, since a
latency measured from Falkenstein includes the network path. Nothing is
published, and the record stays with whoever drove the run. You chose the
endpoint, and the record says so. Delivery pipeline:
[`deploy/hosted/README.md`][hosted-readme].

## Results and certificates

A test report is not a certificate. Certificates are issued by the hosted
instrument ([#593](https://github.com/Vernum-Projecten/Veredictum/issues/593)),
which runs in trial mode for now: full results and verdicts, no certificate.

## Where it runs

| | |
|---|---|
| Provider | Hetzner Cloud |
| Location | Falkenstein, Germany, network zone `eu-central` |
| Machine | CAX21: 4 vCPU Arm (Ampere Altra), 8 GB RAM, 80 GB local disk |
| Price | €10.49 per month, from the account's own record on 2026-10-23 |
| Paid by | The maintainer, out of pocket. No vendor funds the instrument that grades them |
| Concurrent runs | Two |

The cap fits 8 GB, since each engine loads the whole catalogue
([#388](https://github.com/Vernum-Projecten/Veredictum/issues/388)). It is
`VEREDICTUM_MAX_CONCURRENT_RUNS` in the box's environment file and changes
with a redeploy, not a release.

**The host holds no signing credential.** Its one secret is the token for
`/metrics`: runs by outcome, refusals by reason, queue depth, error classes,
container memory and CPU. No server, target or result is named.

The box is disposable. Past a restart it keeps one journal line per run (id,
outcome, wait and drive time, cases completed). Its cloud-init, compose file
and proxy configuration are under
[`deploy/hosted/`](https://github.com/Vernum-Projecten/Veredictum/tree/main/deploy/hosted).

## Who can change the thing that judges

Every change to `main` is a pull request, with its diff, CI run and audit-log
entry. The maintainer may merge their own pull request without a second
reviewer or with failing checks, but cannot push to `main`. Signed commits, no
force-pushes and no branch deletion apply to everyone.

[hosted-readme]: https://github.com/Vernum-Projecten/Veredictum/blob/main/deploy/hosted/README.md
