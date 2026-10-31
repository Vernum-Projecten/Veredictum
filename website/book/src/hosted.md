# The hosted instrument

<!-- toc -->

[console.veredictum.eu](https://console.veredictum.eu) is the hosted
conformance instrument. It drives the catalogue against the endpoint you name,
records every exchange, and computes the verdicts, so you read the full
results and verdicts for your own server. It runs functional conformance only:
measured performance (`perf` and `perf-compare`) is run locally with the
CLI. Nothing a run produces is published, and the repository
keeps no list of results.

What a hosted run establishes is that the judgement was made by an instrument
nobody in the exchange controls, and that the judgement is arithmetic anyone
can repeat from the recorded exchanges. What it cannot establish is the
environment: you chose the endpoint, and the record says so. Running the
instrument yourself answers the same question on your own hardware
([Installation](installation.md)). The delivery pipeline behind the hosted
instrument is documented in
[`deploy/hosted/README.md`][hosted-readme].

## Results and certificates

The repository publishes the instrument, never results about somebody's
product. A run's record stays with whoever drove the run.

**A test report is not a certificate.** A record says what happened when a
named version of a named system was driven by a named version of this
instrument on a named machine. Certificates are issued by the hosted
instrument
([#593](https://github.com/Vernum-Projecten/Veredictum/issues/593)). It runs
in trial mode for now: a vendor sees the full results and verdicts, and no
certificate is issued.

## Where it runs

An instrument that grades other people's products in public states its own
conditions in public.

| | |
|---|---|
| Provider | Hetzner Cloud |
| Location | Falkenstein, Germany, network zone `eu-central` |
| Machine | CAX21: 4 vCPU Arm (Ampere Altra), 8 GB RAM, 80 GB local disk |
| Price | €10.49 per month, from the account's own record on 2026-10-23 |
| Paid by | The maintainer, out of pocket. No vendor funds the instrument that grades them |
| Concurrent runs | Two |

The cap comes from the machine. An engine process loads the whole catalogue,
and every concurrent run shares the box's 8 GB with the console, the proxy and
the operating system. A cap above what the memory holds would be the OOM
killer ending a conformance run halfway through, which looks exactly like a
defect in the instrument, so the value is the one
[#388](https://github.com/Vernum-Projecten/Veredictum/issues/388) reasoned for
an 8 GB machine and it moves only with a measurement. The number lives in one
place, `VEREDICTUM_MAX_CONCURRENT_RUNS` in the box's environment file, beside
the container's memory limit. Raising it is an environment edit and a
redeploy, never a release, so a bigger machine changes what the instrument
admits without changing a line of code.

**The host holds no signing credential.** What the box can do is drive the
catalogue, record the exchanges and show the verdicts. Its one secret is the
token that opens the operator's counters at `/metrics`: runs by outcome,
refusals by reason, queue depth, error classes and the container's memory and
CPU. They are counts and durations only, and name no server, no target and no
result.

The box is disposable. The only thing it keeps past a restart is one line per
run in the host's journal: the run's id, how it ended, how long it waited and
drove, and how many cases it completed. The whole posture is
committed under
[`deploy/hosted/`](https://github.com/Vernum-Projecten/Veredictum/tree/main/deploy/hosted):
the cloud-init that built it, the compose file, the proxy configuration. A
rebuild from that directory produces the same machine.

## Who can change the thing that judges

A verdict is trusted because the engine and the catalogue that produced it are
the ones on `main`. So who can move `main` is part of the trust claim, and the
answer is published rather than implied.

Every change to `main` requires a pull request, with its diff, its CI run and
its audit-log entry. The maintainer holds a pull-request-only bypass, which
means they may merge their own pull request without a second reviewer, and may
merge one whose checks have not passed. They cannot push to `main` at all.
Signed commits, no force-pushes and no branch deletion are enforced with no
exception for anyone.

For a project with one maintainer, a second reviewer is a promise nobody could
keep. A visible object for every change is one that can be kept, and it is the
one that matters when somebody wants to audit how a verdict was produced.

## One measurement caveat

The hosted instrument performs functional conformance runs only. The measured
instruments (`perf` and `perf-compare`) are run locally with the CLI, and
the hosted console exposes none of them. A latency
measured from a fixed origin in Falkenstein carries the network path to
wherever the system under test lives, and distance is not a property of the
database being graded. Measured runs are driven by the operator, on hardware
they describe.

[hosted-readme]: https://github.com/Vernum-Projecten/Veredictum/blob/main/deploy/hosted/README.md
