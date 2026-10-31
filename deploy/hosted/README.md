# The hosted instrument (console.veredictum.eu)

console.veredictum.eu is the hosted conformance instrument (#388). The console
drives the catalogue against the endpoint a visitor names, records every
exchange, and computes the verdicts, so a vendor sees the full results and
verdicts for its own server. Nothing it produces is published anywhere: the
record stays with the run, and the repository keeps no list of results. Anyone
may run the console themselves against a deployment the internet cannot reach.

It runs **functional conformance only**. The measured instruments (`perf`,
`stress` and the comparative `bench`) are run locally with the
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

Both rest on the headroom figures in "Headroom on the CAX21" below.

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
mounts nothing at all. `deploy/docker/Dockerfile` is where that is built, at a release.

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
| `env.example` | A copy-to-`.env` template: the machine-sized caps, the image reference and the certificate setting | never. `.env` is the operator's file, written by hand on the box |
| `jwks.json` | The JWK Set certificates verify against, compiled into the console and served at `/.well-known/jwks.json` | in the image: the console carries the bytes it serves |

The box holds **no checkout of this repository**, and it fetches nothing from
here over the network. Everything the instrument needs is baked into the image,
which CI builds and pushes; the box only pulls.

## The posture travels in the image

The compose file and the Caddyfile the box runs are release data, exactly like
the catalogue (#423). `deploy/docker/Dockerfile` COPYs both to `/app/posture/`, and
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

In trial mode, which is how it runs today, nothing durable of a run's
content. A run's artifacts live long enough to be judged and shown. The one
durable trace is the run record (see "The operator's signal"): its id, how it
ended, how long it waited and drove, and how many cases it completed, kept in
the host's journal. It names no target, no SUT and no result. Because the box does not restart,
the console sweeps expired run directories itself on a stated policy —
`ARTIFACTS_KEPT`, currently a day — and a swept run answers through #386's
honest "this console knows nothing about that run" rather than a stack trace.

**No signing credential lives on this host while it is in trial mode.** `.env`
carries the image reference, the concurrency cap, the issuance setting and the
metrics token, and the token opens the operator's counters and nothing else. The console reads no signing key in trial mode,
so the box runs without one.

**No backups yet**, because in trial mode there is nothing here worth backing
up: the container is rebuilt from a published image, and a run's artifacts are
transient. That stops being true the moment certificates are issued, which is
why issuance waits for the durable store and its backups (#824).

## Certificates

The hosted instrument is where a vendor certifies a CDR (#592). A run that
finishes here can receive a certificate, signed on this machine, which anyone
verifies offline with `veredictum verify` and the published public key. The
console does not verify certificates and no certificate is ever committed to
the repository.

### The format

A certificate is a JWS (RFC 7515) in the compact serialization, signed with
Ed25519 (`EdDSA`, RFC 8037). Its header names the key by `kid`. Its payload
names the server with who supplied each identity field, the base URL, the
catalogue digest, the engine version and approval status, the verdict summary,
and the SHA-256 digests of the run's `results.json` and `recording.har`. The
payload's schema is `schemas/certificate.schema.json`.

The verifying keys are a JWK Set (RFC 7517), `deploy/hosted/jwks.json` in this
directory. The console compiles that file in and serves it at
`https://console.veredictum.eu/.well-known/jwks.json`, and a console test holds
the served bytes to the committed ones. Two members of a key are this
instrument's own: `expires_at`, the last instant the key may sign, and
`revoked_at`, which withdraws everything the key signed. `veredictum verify`
honours both; a generic JOSE library ignores them, so a third party verifying
with one must check `revoked_at` itself. The set holds no key yet.

### Trial mode, and switching issuance on

One `.env` setting decides it: `VEREDICTUM_CERTIFICATES`. Only the exact value
`issue` turns issuance on. Unset, `trial`, or anything else is trial mode, and
the console logs an unrecognised value at startup, so a typo never starts
signing. In trial mode runs, results and verdicts work as ever, no certificate
is created, every page's status banner says so, and no signing key is read.

With issuance on, the console also reads `VEREDICTUM_SIGNING_KEY_FILE` and
`VEREDICTUM_SIGNING_KID`. It refuses to start when either is unset, when the
key does not parse or sign, or when the published set does not carry the key
under that `kid`, carries a different key there, revokes it or has let it
expire. Every finished run then gets `certificate.jws` beside its results, and
its results page offers the download.

Issuance stays off until the durable store and its backups exist (#824);
switching it on is #825. The order of the switch matters, because the served
set travels in the image:

1. Generate the key on the box (below) and append its public JWK to
   `deploy/hosted/jwks.json` in a pull request.
2. Cut a release that carries it, and let it deploy. The served set now
   publishes the key, and the console still issues nothing.
3. Add the three variables to `.env` and redeploy. That step is an `.env` edit
   and a redeploy, never a release.

### Where the private key lives

A file on this machine: `/opt/veredictum-console/signing/<kid>.pem`, mode
`0400`, owned by the console's user (uid `65532` in the distroless image), in a
directory mounted read-only into the container at `/run/veredictum/signing`.
It is generated on the box and never leaves it: no copy in CI, in the
repository, in a password manager or in a backup.

A hardware-backed key and a separate signing service were the alternatives.
Both are stronger against a compromise of this machine, and both add a second
system to run on a project with one box and one operator. The file is the
honest choice for that size, and the foundation hand-over below is the moment
to revisit it.

### Generating a key

```bash
kid="veredictum-$(date -u +%Y)"
install -d -m 0750 -o 65532 -g 65532 /opt/veredictum-console/signing
openssl genpkey -algorithm ed25519 -out "/opt/veredictum-console/signing/$kid.pem"
chown 65532:65532 "/opt/veredictum-console/signing/$kid.pem"
chmod 0400 "/opt/veredictum-console/signing/$kid.pem"
# The public half, as the JWK's `x`: the last 32 bytes of the DER public key.
openssl pkey -in "/opt/veredictum-console/signing/$kid.pem" -pubout -outform DER \
  | tail -c 32 | basenc --base64url | tr -d '=\n'
```

The JWK appended to the set is
`{ "kty": "OKP", "crv": "Ed25519", "x": "<the line above>", "kid": "<kid>", "alg": "EdDSA", "use": "sig" }`.
`schemas/jwks.schema.json` describes the members, and the set's reader refuses
any member it does not know and any private member `d`.

### Rotation

A rotation is a JWKS append. Generate the new key under a new `kid`, append its
JWK, and set the old key's `expires_at` to the instant the switch will happen.
Ship the set in a release, then point `VEREDICTUM_SIGNING_KEY_FILE` and
`VEREDICTUM_SIGNING_KID` at the new key and redeploy. The old key stays in the
set, so every certificate it signed before `expires_at` keeps verifying, and a
certificate it claims to have signed after that instant is refused. Delete the
old private key file once the new one is serving.

### Revocation

Set `revoked_at` on the key in the set and ship the set in a release. Every
certificate that key signed is then refused by `veredictum verify`, whatever
its `issued_at`, because a stolen key can sign any date. Issue a new key as in
a rotation, and re-issue the legitimate certificates under it from the stored
runs (#824 keeps them for exactly this).

### What a stolen key allows

Whoever holds the private key can sign a certificate for any server with any
verdict, and it verifies until the key is revoked. It cannot change a
certificate already issued, a run already recorded, or the catalogue, and it
cannot sign under another `kid`. Revocation ends it for every verifier holding
the updated set, which is the reason the set is served from this host and
committed in the repository rather than copied once.

### What the vendor receives, and what this machine keeps

The vendor receives the certificate and, with #824, the `results.json` and
`recording.har` whose digests it names, so `sha256sum` confirms the copy and
`veredictum test --rejudge` re-judges it. This machine keeps the same three
files per certified run: the recording is what a re-judgement and a re-issue
read. A recording carries the server's response bodies verbatim, which is why a
vendor certifies a test system, never one with real patient data.

### Backups

With issuance on, this machine holds durable state for the first time: every
certified run. It is backed up off the machine on a stated schedule, with a
restore rehearsed and written down here, before the first certificate is
issued (#824). The private key is excluded from every backup; losing it costs a
rotation, while a copy of it in a backup is a second place to steal it from.

### The foundation hand-over

Certification is expected to pass to the openEHR Foundation, which will use
this instrument to do it. What it takes over: the issuing machine (or its own,
built from `cloud-init.yaml`), the catalogue's governance in this repository,
and the signing. The signing moves by a rotation, never by copying a key: the
foundation generates its own key on its own machine, its JWK is appended, and
this project's key gets `expires_at` at the hand-over, so every certificate
issued before it keeps verifying and no private key ever changes hands.

## How a deploy happens

`.github/actions/hosted-deploy` is the only thing that deploys, and it carries
every step. Two callers reach it, on exactly two occasions:

1. **A real release.** The `hosted` job in `release.yml`, after the scan-and-tag
   leg applies the image tags. A pre-release moves no `:latest`, so it deploys
   nothing.
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

The watch also reads the operator's counters when the `HOSTED_METRICS_TOKEN`
repository secret is set: a `/metrics` that refuses the token is `blind`, and
a kernel OOM kill inside the console container is `strained`. Its log is
public, so it prints the status code and nothing of the body.

## The operator's signal

Nothing a run produces is published, and nothing here changes that. The signal
is counts, durations, queue depth, outcomes by kind, refusals by reason,
resource use and error classes. No target URL, credential, SUT name, result row
or response body is counted, labelled or logged by it. Every label is a closed
vocabulary the console spells itself (`app/veredictum-console/src/metrics.rs`),
so no text a visitor typed can reach it. The box holds no signing key, and
none of this is an input to a verdict or a record.

### The counters, without shell access

`https://console.veredictum.eu/metrics`, in the Prometheus text format, to a
request carrying `Authorization: Bearer <VEREDICTUM_METRICS_TOKEN>`:

```bash
curl -sS -H @<(printf 'Authorization: Bearer %s\n' "$TOKEN") \
  https://console.veredictum.eu/metrics
```

With no token in `.env` the route answers 404. A wrong or missing header gets
401. Caddy's access log redacts the `Authorization` header by default.

| Series | What it answers |
|---|---|
| `veredictum_runs_accepted_total`, `_started_total` | how many runs were taken, and how many reached an engine |
| `veredictum_runs_ended_total{outcome}` | `finished`, `cancelled`, `expired` (the wall-clock cap), `failed` (no parseable record), `not_started` (the engine would not spawn), `withdrawn` (cancelled while queued) |
| `veredictum_run_duration_seconds` | a histogram of engine drive time, buckets up to the 30-minute cap |
| `veredictum_run_cases_completed_total` | cases the engines reported complete |
| `veredictum_runs_running`, `_queued`, `_max_concurrent` | the queue now, beside the cap |
| `veredictum_refusals_total{guard,reason}` | the rate ledger (`probe`, `start`), the target guard (`unreadable`, `no_host`, `unresolvable`, `unchecked`, or the refused family such as `private`), the statement validator (`too_large`, `not_json`, `schema_invalid`, `unparsable`), and the one-run-per-visitor cap |
| `veredictum_server_fn_responses_total{class}` | server-function answers by status class; a refusal a server function raises answers 5xx, so subtract the refusals to read the fault rate |
| `veredictum_container_*` | the container's cgroup: memory now, peak and limit, anonymous memory, CPU seconds, process count, OOM kills |

The counters are process memory. A restart zeroes them, the same as the job
map; the container figures are the cgroup's own and also restart with it.

### The run record, past a restart

Every run writes one JSON line when its engine starts and one when it leaves
the job map, to the console's standard error:

```json
{"event":"run_ended","run":"3f2504e0-…","outcome":"finished","queued_ms":0,"driving_ms":435120,"cases_completed":1689,"cases_total":1689}
```

The compose file sends the console's output to the host's journal, which
keeps it through a container recreate and a reboot, capped at 1 GB and a year
by the drop-in `cloud-init.yaml` writes. A run that has a `run_started` line
and no `run_ended` line was in flight when the console itself stopped. On the
box:

```bash
journalctl CONTAINER_TAG=veredictum-console -o cat --since -7d \
  | jq -cR 'fromjson? | select(.event == "run_ended")'
```

### Headroom on the CAX21

The two caps above are starting values until a measured campaign on this host
confirms them. What they rest on today, and how each figure was reached:

| Figure | Value | How |
|---|---|---|
| Catalogue | 219 documents, 1,689 workflows | `veredictum validate`, 2026-10-31 |
| Gate, the first thing every run does | 244 MB peak RSS, 3.2 s | `/usr/bin/time -l veredictum validate`, release build, Apple M-series |
| Full campaign, no server answering | 494 MB peak RSS, 2.0 CPU-seconds, 43 MB recording, 22 MB results | `/usr/bin/time -l veredictum test` against a closed port, same build: every exchange fails at connect, so the recording has requests and no response bodies |
| Full campaign, live | about 435 s | 1,146 cases took 295 s against a live third-party CDR over the internet on 2026-08-31 (#467), 0.26 s a case, scaled to 1,689 |

Memory is the ceiling, as `run_job.rs` says. A live recording adds every
response body to the bodiless 494 MB, so the budget is 1.5 GB per engine,
three times the bodiless figure, plus about 250 MB for the console's own
catalogue read. Two runs come to about 3.3 GB of the 6 GB limit. CPU is not the
ceiling: a run waits on the server for nearly all of its time, and the bodiless
campaign spent 2 CPU-seconds on the whole catalogue, so two runs use well under
two of the four cores. The wall-clock cap of 30 minutes sits four times above
the estimated campaign.

The budget is an estimate, and the on-box measurement replaces it. After a
deploy, so the container's counters start clean, drive one full campaign
against a public CDR and sample the counters while it runs:

```bash
while sleep 5; do
  curl -sS -H @<(printf 'Authorization: Bearer %s\n' "$TOKEN") \
    https://console.veredictum.eu/metrics \
    | awk '/^veredictum_container_(memory_anon_bytes|memory_peak_bytes|cpu_seconds_total|oom_kills_total) /'
done
```

The largest `memory_anon_bytes` is the working set; `memory_peak_bytes`
includes page cache and is the upper bound. CPU use is the
`cpu_seconds_total` difference over the wall-clock time, divided by four. Then
drive two at once. Write both results into this table, and move the caps if
they say to.

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
