# The hosted instrument (console.veredictum.eu)

console.veredictum.eu is the hosted conformance instrument (#388). The console
drives the catalogue against the endpoint a visitor names, records every
exchange, and computes the verdicts, so a vendor sees the full results and
verdicts for its own server. Nothing it produces is published anywhere: the
record stays with the run, and the repository keeps no list of results. Anyone
may run the console themselves against a deployment the internet cannot reach.

It runs **functional conformance only**. The measured instruments (`perf`,
`stress`, the AQL probe and the comparative `bench`) are run locally with the
`veredictum` CLI, and the hosted console exposes none of them.

It also serves the catalogue as a reading surface.

## Why a box, and not a platform

A conformance run is a process that outlives the request that started it: the
console spawns the pinned engine, streams its output for minutes, and writes the
record to the container's own filesystem. An autoscaling request platform breaks
all three, and #387 measured it live — several instances answered one service,
so a poll reached one that never held the run and the page said "No run is in
flight" about a run that was still executing; an instance with no traffic for
five minutes was terminated with the engine child inside it; and each instance
had a filesystem the others could not see.

So the instrument runs on one machine that does not stop. The cost of that
choice is ownership: the firewall, the certificate, the deploy path, the log
rotation and the answer to "is it up" are all things this directory states.

## The machine

A Hetzner **CAX21**: 4 vCPU Arm (Ampere Altra), 8 GB RAM, 80 GB local disk,
named `veredictum`, in Falkenstein (`fsn1`, `eu-central`), dual-stack, on
Ubuntu 26.04 LTS.

**So the caps are the box's, not the aspiration's.** #388 reasoned two
concurrent runs on an 8 GB machine and said plainly that those were starting
values to re-derive by measuring on the chosen host. This host is that 8 GB
machine, so it drives **two**, and the queue covers everything past the cap
with a position and an estimate rather than a refusal. The values stay
starting points until a measured run on this host confirms them.

Two numbers say so, and neither is a code change:

| Where | Value | Why |
|---|---|---|
| `VEREDICTUM_MAX_CONCURRENT_RUNS` in `.env` | `2` | What the host can hold. A value that is not a positive integer refuses to start, rather than falling back to something larger |
| `memory:` in `docker-compose.yml` | `6g` | Leaves roughly 2 GB for Caddy and the operating system. A limit above what the host has is not a limit |

**On a resize, move both together** — and nothing else. That is the whole point
of the cap being an environment value: a bigger box is an `.env` edit and a
redeploy, not a release.

Arm is fine here because the image publishes `linux/arm64` beside
`linux/amd64`, and the scheduled image scan covers both. The box pulls the arm64
variant of the same tag.

## The image

**One image, and this directory builds none of it.** `ghcr.io/vernum-projecten/veredictum`
carries the engine, the console and the release's own catalogue and vendored
specification oracle (#420), which is why this instance
mounts nothing at all. `docker/Dockerfile` is where that is built, at a release.

Two consequences worth stating. A run produced here names a catalogue
revision that belongs to a published version, because the data in the image is
the data of the release that built it. And an image built before #420 cannot be
deployed here at all: it declares no
`eu.veredictum.image.carries-catalogue` label, and the deploy lane refuses it
rather than serving an instrument whose `/work` is empty.

`CONSOLE_IMAGE` in `.env` names the exact reference. `:latest` is the release
pointer, and naming a version instead is how a rollback pins an older one — an
older one that still carries the label.

## What is in this directory

| File | What it is | How the box gets it |
|---|---|---|
| `cloud-init.yaml` | A fresh box to serving state on first boot: the `deploy` user, key-only SSH, both firewalls, unattended upgrades, Docker, capped logs, and the one command the CI key may run | the server's user data at creation, once |
| `docker-compose.yml` | What runs on the box: the console with its healthcheck and memory limit, behind Caddy | baked into the image at `/app/posture/`, and `deploy.sh` installs it from the image it pulled |
| `Caddyfile` | Automatic TLS, and the raised timeouts a run's streaming needs | the same way, and a change to it restarts Caddy |
| `env.example` | A copy-to-`.env` template: the machine-sized caps and the image reference | never. `.env` is the operator's file, written by hand on the box |

The box holds **no checkout of this repository**, and it fetches nothing from
here over the network. Everything the instrument needs is baked into the image,
which CI builds and pushes; the box only pulls.

## The posture travels in the image

The compose file and the Caddyfile the box runs are release data, exactly like
the catalogue (#423). `docker/Dockerfile` COPYs both to `/app/posture/`, and
`deploy.sh` extracts them out of the image it just pulled:

1. It reads `CONSOLE_IMAGE` from `.env`, which is what names the artifact to
   interrogate, and pulls it.
2. The base is distroless, so it carries no shell. The files come out through
   `docker create` plus `docker cp` rather than a `cat` that could never run.
3. An extracted file that is empty **stops the deploy**. So does a candidate
   compose file that `docker compose config` refuses to parse, because
   installing one would leave the box unable to run its next deploy at all.
4. Each replaced file is kept as `docker-compose.yml.prev` or `Caddyfile.prev`,
   so a bad posture is recovered by copying the previous file back and running
   `docker compose up -d` — never by editing YAML over SSH.
5. A changed Caddyfile restarts the caddy service explicitly. It is a bind
   mount, and `docker compose up -d` does not recreate a container because a
   mounted file's contents changed, so the restart is what applies the change.

A posture change therefore arrives at a release, carrying the provenance of the
image it travelled in: the same trade #420 accepted for the catalogue. Slower on
purpose, and nothing about it widens what the deploy key can do — that key still
runs one script and writes no arbitrary file.

**`deploy.sh` does not update itself.** A script that overwrites itself mid-run
is a footgun, so a change to it in `cloud-init.yaml` reaches the box by hand:
copy the new content to `/opt/veredictum-console/deploy.sh` over SSH, or rebuild
the box from the cloud-init document. `scripts/checks/hosted-deploy-script.sh`
extracts that script in CI and holds it to shellcheck, because the YAML it lives
in is a place no other gate reads as shell.

What only a real deploy proves: that the extraction and the restart behave on
the box. CI lints the script and checks that the image bakes the two paths the
script reads; the deploy lane's own verification, which fetches the public URL
from the runner and requires the served engine version, is what catches a Caddy
that will not start on a posture nobody has run before.

## What the instance keeps, and what it never holds

Nothing durable. A run's artifacts live long enough to be judged and shown.
Because the box does not restart,
the console sweeps expired run directories itself on a stated policy —
`ARTIFACTS_KEPT`, currently a day — and a swept run answers through #386's
honest "this console knows nothing about that run" rather than a stack trace.

**No credential lives on this host.** `.env` carries the image reference and the
concurrency cap, and nothing that signs or opens anything.

**No backups**, deliberately. There is nothing here worth backing up: the
container is rebuilt from a published image, and a run's artifacts are
transient by design.

## How a deploy happens

`.github/actions/hosted-deploy` is the only thing that deploys, and it carries
every step. Two callers reach it, on exactly two occasions:

1. **A real release.** The `hosted` job in `release.yml`, after the scan-and-tag
   leg applies the image tags. While Veredictum is beta, every release is a
   pre-release (`1.0.0-beta.N`) and `:latest` moves on every one, so every
   beta deploys here.
2. **A manual `workflow_dispatch`** of `.github/workflows/hosted-deploy.yml`.

Both callers are a checkout plus the action, so they run identical steps. The
release path is an ordinary job rather than a call to `hosted-deploy.yml`
because the deploy key lives in the `hosted` environment, and a job inside a
`workflow_call` workflow cannot read environment secrets even when it names the
environment ([actions/runner#1490](https://github.com/actions/runner/issues/1490),
closed as not planned).

There is deliberately no push trigger. A push to `deploy/hosted/**` publishes no
image, and the posture on the box comes out of an image, so a push-triggered run
would report success having applied nothing (#420 removed it, #423 is what makes
the files actually travel).

The lane builds nothing: it asserts by digest that `:latest` is the tagged image
and deploys through
[`rubentalstra/hetzner-deploy-action`](https://github.com/rubentalstra/hetzner-deploy-action),
which was written for this. It waits for the container's own healthcheck and
then fetches the public URL **from the runner**, requiring the served engine
version — because a bare 200 is not proof, the deployment being replaced answers
200 too.

## How it is watched

Four layers, each answering a different question:

1. The container's healthcheck plus `restart: unless-stopped` — is it serving,
   and if not, is it coming back?
2. The deploy's own verification — did what we just shipped actually land?
3. `.github/workflows/hosted-watch.yml`, every fifteen minutes — is it up, and
   is it serving the release it should? It opens one issue and reuses it, then
   closes it on recovery, so an open issue always means a live problem.
4. Log rotation with a size cap, because a box that fills its disk fails in a
   way that looks like nothing at all.

## Provisioning

#403, once, and #583 again for the CAX21: buy the box with `cloud-init.yaml` as
its user data and a Hetzner Cloud Firewall allowing 22, 80 and 443, point
`console.veredictum.eu` at it, and store a deploy key restricted to one command
as `HOSTED_SSH_KEY` with the host's key as `HOSTED_KNOWN_HOSTS`. The deploy
connects by name, so the entries are keyed `console.veredictum.eu`, but scan the
keys by address (`ssh-keyscan <ipv4>`) and compare them with the box's own
`/etc/ssh/ssh_host_*_key.pub`: while DNS still points at an old box, a scan by
name records the wrong host (#403).

**`cloud-init.yaml` is user data, given at creation. Never apply it to a
running box.** Placed in `/etc/cloud/cloud.cfg.d/` and re-run with
`cloud-init clean`, its `runcmd` list is replaced by the one in Hetzner's
instance data rather than merged with it. Everything else applies, the SSH
policy included, so root login is gone while `deploy` never joins the
`docker` group and the firewall rules never run: the box can be neither
administered nor deployed to (#583). A box that needs a different
configuration is deleted and created again, keeping its Primary IPs.

**Both are environment secrets, in an environment named `hosted`, not
repository secrets.** The deploy job names that environment, so nothing else in
this repository can reach the key, and the environment's own protection rules
are what stands in front of it.

No Hetzner API token is created for CI at all — the deploy talks to the host and
nothing else, so a key that leaks cannot destroy the server.
