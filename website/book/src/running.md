# Running the instrument

<!-- toc -->

A conformance campaign is four steps. Each one either checks a file or writes
one the next step reads, so you can stop after any of them, inspect what was
produced, and resume.

## 1. Check the catalogue

Before a server is involved, check the catalogue you are about to grade with:

```bash
veredictum validate --root catalogue --specs specs/openehr
```

Zero findings is the only passing result, and the command exits `1` when there
is even one. [The command reference](commands.md#validate) lists the gates it
runs. `veredictum test` runs the same gate before it drives anything, and
refuses to start while it has a finding.

## 2. Declare your deployment

This repository commits no declaration about anybody's product. ISO/IEC 9646-7
splits an ICS proforma in one place: every cell belongs to the proforma
specifier except the support and supported-values columns, which belong to the
supplier of the implementation. The proforma is the catalogue itself: the
capability folders under `catalogue/cases/` and the options in
`catalogue/capabilities.yaml`. The answers are yours.

Put your answers in a directory of your own beside the clone, `mine/` in the
commands below. `fixtures/declaration/` carries both files filled in for a
product that does not exist, which is a template to read and never a claim to
copy.

- **`statement.json`** is your declaration of claims, against
  `schemas/statement.schema.json`. It names the product and version, the
  capability folders it claims in `claims.capabilities`, and in `options` one
  arm of each option in `capabilities.yaml`.
- **`ixit.json`** describes the deployment, against
  `schemas/ixit.schema.json`. A rows file's `declared` inputs read their
  values from it, such as the system id your server stamps, and a measured run
  reads its `environment` block, naming the hardware and topology the numbers
  were produced on.

**Credentials are never in either file.** `veredictum test` reads them from the
environment variables its `--auth` and `--admin-auth` modes name, or from the
file a `_FILE` variable points at, so no secret ever enters an artifact you
might publish.

## 3. Review your claim

```bash
veredictum validate --root catalogue --specs specs/openehr \
    --statement mine/statement.json
```

`--statement` adds the static review to the same command that checks the
catalogue: the statement must validate against its published schema, every
claimed capability must be a capability folder, and every option needs exactly
one arm. A claim that fails here would otherwise stop the run.

## 4. Drive the catalogue

```bash
export VEREDICTUM_CDR_USER=… VEREDICTUM_CDR_PASSWORD=…
veredictum test --base-url https://cdr.example/openehr/v1 --auth basic \
    --statement mine/statement.json --ixit mine/ixit.json --out out/
```

This CDR wants a username and password. One that needs no login takes no
`--auth` at all, and one that wants a bearer token takes `--auth bearer` with
`VEREDICTUM_CDR_TOKEN`; [Logging in to the CDR](authentication.md) has the
table, the administrator's variables and the `_FILE` forms.

Every workflow of a document in a folder your statement does not claim is
skipped as `not-claimed`, and the documents of every option arm you did not
pick are skipped as `option-not-selected`. Without a statement, every
capability runs and no arm is selected, which is what you want when you are
exploring an unfamiliar server rather than grading a declared one. A workflow
with an administrative step needs `--admin-auth`; without it, it is skipped as
`not-declared`. A SMART on openEHR Platform also passes `--platform-url` and
declares its scoped tokens in the IXIT ([`test`](commands.md#test)).

The run writes `out/results.json`, one row per workflow row, each passed,
failed, errored or skipped, and `out/recording.har`, every exchange as HAR 1.2.
A failed or errored row carries four parts: what was checked, why (the quoted
sentence), what was expected and what came back. The recording holds the
server's response bodies verbatim, so it stays where you keep the run.

The exit status is `0` when every row passed or was skipped, `1` when a row
failed or errored, and `2` when the run could not start, so a CI job goes red
on a failed row. A warning row, a SHOULD the server did not follow, is
reported and leaves the status at `0`. [`export`](commands.md#export) turns
`results.json` into JUnit XML, CTRF and a Markdown summary for that job.

`--document <ID>` narrows the run to named documents, which is how you re-drive
one family while working on a fix. A narrowed `results.json` is a working tool
and never the record you submit.

`veredictum identify --base-url <URL>` asks a server who it is before you drive
it: the ITS-REST System operation's `Options` document carries `solution`,
`solution_version`, `vendor` and `restapi_specs_version`.

## Re-judging a recording

```bash
veredictum test --rejudge out/recording.har \
    --statement mine/statement.json --ixit mine/ixit.json --out rejudged/
```

Every request is answered from the recording and every recorded input is
reused, so `rejudged/results.json` comes out byte-identical to the run's own.
That is what lets anyone holding the recording check the judgement without a
server. Pass the statement, IXIT and `--document` list the recorded run was
given; a replay that does not line up with the recording is refused rather
than judged.

The command re-judges the recording of a whole run. The recordings in the
verification pack (`verification-pack/arazzo/`) are slices of one or two
workflows each, so the command refuses them. They are re-judged by the test
suite instead (`cargo nextest run -p veredictum arazzo_pack`), which plans every
workflow outside a slice as skipped and requires every adjudicated verdict back.

## The container image

The image carries the catalogue and runs the same subcommand. Mount a
directory the image's user (uid 65532) can write to, and name it as `--out`:

```bash
docker run --rm -v "$PWD/out:/work/out" \
    -e VEREDICTUM_CDR_USER -e VEREDICTUM_CDR_PASSWORD \
    ghcr.io/vernum-projecten/veredictum:<tag> \
    test --base-url https://cdr.example/openehr/v1 --auth basic --out /work/out
```

**What any Arazzo tool can do with the catalogue, and what only Veredictum
does.** The documents are plain [Arazzo 1.1.0]. Any Arazzo tool can read them,
and one that runs workflows can smoke-run them against a server: the steps,
parameters, request bodies and success criteria are standard. A conformance
verdict comes only from `veredictum`. The citation behind each criterion, the
judges a plain condition cannot express, the selection of documents by the
party's statement, and the attribution of each red row to the server or to the
instrument live in the `x-veredictum-*` extensions and in this pipeline, and a
generic runner ignores both. Its pass or fail is a second opinion, never a
verdict.

[Arazzo 1.1.0]: https://spec.openapis.org/arazzo/v1.1.0.html

## Measured performance

Those four steps cover functional conformance. The second kind of evidence is
a performance record: how a deployment holds up under the traffic a clinical
front end sends, measured as latency and throughput while the offered load
rises. It reads the workloads under `perf/`, never the conformance catalogue,
and it earns no class and enters no conformance verdict.

No openEHR specification governs performance. The CNF guide says so in as
many words: "Non-functional conformance (performance, etc) is not addressed by
this guide". Everything below is the instrument's own design, and every
method claim names the source it rests on.

### What a run does, phase by phase

`veredictum perf` offers one workload document: a declared mix of operations,
AQL shapes and payloads, sent as Poisson arrivals, with chart reviews run as
short sessions on one EHR. Every latency is measured from the instant the
request was due, so a request that waited 40 ms for the generator or the
connection pool carries those 40 ms (wrk2's correction for coordinated
omission).

1. **Seeding**, before the clock: a closed-loop bulk load of the workload's
   corpus recipe, reported apart and labelled closed-loop. `--corpus reuse`
   reuses a stored corpus instead, and refuses one whose read-back counts or
   digest moved.
2. **Posture canaries**, first bracket: the declared posture profile is
   checked against the server (below).
3. **Settle**, 15 s: `vacuumdb --all --analyze` through the declared database
   container, then idle.
4. **Warm-up**, 20 s at the start rate, discarded.
5. **Scout**, 60 s: an exponential ramp judged in 2 s windows. The first
   window that breaches the qualification gives the knee estimate K̂.
6. **Drain**, at most 10 s.
7. **Ladder**: four plateaus at 0.5, 0.75, 0.9 and 1.05 × K̂, each a 5 s
   ramp-in and a hold of `clamp(5,000 / rate, 30 s, 90 s)`, so a hold is sized
   for 5,000 samples. A fifth plateau at 0.25 × K̂ runs only when the lowest
   one fails.
8. **Closing**: the second posture bracket, after the last plateau.

The run after seeding takes at most ten minutes, and about five when the
server's knee is above 150 operations per second. The planned range is
printed before the first measured request, and `perf-workloads` prints it for
every workload without a server.

A plateau qualifies when every operation holds p99 ≤ 1 s (≤ 5 s for a
population AQL query) at the quantile it is judged at, errors stay at or below
0.1 %, the achieved throughput is at least 95 % of the offered rate, and the
generator kept up. A server that answers any exchange `429` gets no record at
all, because the run would have measured its rate limiter.

### From IXIT to `show`

The run needs the `environment` block of your IXIT filled in, every member,
because a throughput number without the deployment described says nothing.
The optional `containers` block names the server and database containers and
the `database` the server-side readings query; without it those readings are
recorded as absent, with that reason, and the settle phase only idles.

```bash
veredictum perf \
  --root perf \
  --ixit mine/ixit.json \
  --workload perf/workloads/clinical-mix.yaml \
  --posture minimal \
  --host-prescription reference-8cpu-8gb \
  --out runs/perf-result.json

veredictum perf-judge --result runs/perf-result.json
veredictum show runs/perf-result.json
veredictum export --results runs/perf-result.json --markdown runs/perf.md
veredictum perf-assets --results runs/perf-result.json --out runs/charts
```

`perf-judge` recomputes every derived number from the recorded histograms and
exits `1` naming each field that differs, which is how anyone holding the
record checks it without trusting whoever ran it. The committed example record,
`app/veredictum/examples/perf-result.example.json`, is synthetic and labelled
so, and its phases ran as follows:

| Phase | Planned | Took | Exchanges |
|---|---:|---:|---:|
| settle | 15.0 s | 15.0 s | 0 |
| warm-up | 20.0 s | 20.0 s | 160 |
| scout | 60.0 s | 34.0 s, halted at the knee | 3,870 |
| drain | 10.0 s | 1.2 s | 0 |
| ladder | 174.0 s | 165.9 s | 25,310 |
| closing | 10.0 s | 0.4 s | 1 |

That is 236.6 s after seeding, under four minutes, because the scout halted
once the server fell behind. `veredictum show` prints it like this (abridged):

```text
Plateaus (operations/s; latency ms)
    offered  achieved     p50     p95     p99   p99.9     max  errors
✓     100.4     100.4     7.9    41.7     100     n/r     615   0.00%
✓     150.6     150.6     8.1    44.1     125     n/r     750   0.00%
✓     180.8     180.7    15.7     232     795     n/r  12,091   0.00% unsteady
✕     210.9     210.3    16.0   2,370   7,246     n/r  30,015   0.22%

Knee: between 180.8/s offered, the highest plateau that qualifies with every one
below it (achieved 180.7/s), and 210.9/s, the first plateau that fails.
Peak: 246.0/s achieved, in scout window 16, whatever its latency.
Population: 241,024 to 1,390,524 people at their peak, at 13 to 75 peak
arrivals/s per 100,000, read only at corpus scale S (100,000 EHRs) on example
8-vCPU VM, 8 cores, 8 GiB, NVMe storage, single node.
```

### How to read a record

- **Percentiles.** Per plateau and per operation the record states p50, p90,
  p95, p99, p99.9 and max, each with its 95 % interval from the binomial order
  statistics (Le Boudec, *Performance Evaluation of Computer and
  Communication Systems*, 2010, §2.2). Whole-plateau numbers come from summed
  histograms, never from an average of percentiles.
- **Resolvability.** A quantile with fewer than 10 samples above it is
  `not_resolvable` and prints as `n/r`. At 5,000 samples a p99 has 50 above
  it; at 500 it would have 5, which is mostly noise.
- **`judged_at`.** Each operation is judged at the highest of p99, p95, p90
  and p50 that resolves. An operation with a 10 % share has about 500 samples
  on a plateau and is judged at p95, and the record says so.
- **The knee.** The highest offered rate whose plateau and every lower one
  qualify, with its achieved throughput (Jain, *The Art of Computer Systems
  Performance Analysis*, 1991, ch. 3, usable capacity). It is a bracket up to
  the first failing plateau, a lower bound when every plateau qualifies, or
  below the ladder when the lowest fails. The ladder never bisects further.
- **The peak.** The highest achieved throughput anywhere in the run, whatever
  its latency.
- **The USL fit.** The Universal Scalability Law fitted to throughput against
  mean in flight (Gunther, *Guerrilla Capacity Planning*, 2007), with α, β,
  the predicted maximum and R². It explains the shape of the curve and is
  never a verdict.
- **The population reading.** The knee divided by 13 to 75 peak API arrivals
  per second per 100,000 people, the population-anchored model of the
  [methodology](methodology.md), printed as a band beside the corpus scale
  (POC, S, L or R: 10,000 to 10,000,000 EHRs) and the environment it ran on.
- **Server-side readings.** CPU, memory, block and network I/O per declared
  container, database sizes, the costliest statements from
  `pg_stat_statements` and the PostgreSQL settings. They are context and never
  bear on qualification. The sampler is read-only: it creates no extension and
  resets no statistic.
- **The scope sentence.** Every record carries it: a run of at most ten
  minutes measures steady-state capacity and says nothing about memory growth,
  data growth or behaviour over hours.

### What one pass does not measure

One run gives sampling intervals and nothing else. Two GitHub-hosted runs of
the same code have read up to 156 % apart, so the record says
`between_run_noise: unmeasured` unless an A/A floor is attached. The floor
comes from `perf-compare --aa`, which runs one target against itself with the
same interleaving an A/B comparison uses and records, per metric, the largest
ratio between the two sides. Attach it to an A/B run with `--floor`, and a
ratio counts as a difference only when it clears both the declared
`--tolerance` and the floor, and the two sides' 95 % intervals do not overlap.

A single pass also gives no p99 for an operation too rare to resolve one (it is
judged lower, and says so), no knee finer than the ladder's bracket, and
nothing about behaviour beyond ten minutes.

### Posture

A record is comparable with another only when the same features were switched
on behind both. `--posture` declares one profile: `minimal` (the bare
spec-conformant surface, commits validated against the template),
`minimal-signed-digest`, `minimal-signed-pgp` or `clinical-default` (with an
audit trail). Black-box canaries check it after seeding and again after the
last plateau: version signing on versions the run itself committed, commit
validation by offering the workload's invalid twin, authentication with one
uncredentialed read, compression with one `Accept-Encoding` read, and TLS from
the base URL. Audit and tenancy have no released read surface and stay
`declared-only`. A reading that contradicts the declaration, or two readings
that disagree, refuse the run.

### Declaring a workload of your own

A workload is a YAML document against `schemas/perf-workload.schema.json`:
an id and version, a seed, a corpus recipe (scale, compositions per EHR and a
template mix of payload keys from `perf/payloads.yaml`), the interactions with
their shares and a `derivation` each, and the AQL query classes. Check it
without a server:

```bash
veredictum perf-workloads --root perf --catalogue catalogue --workload mine/workload.yaml
```

Loading is the gate. Shares at every level sum to exactly 100 %, every
operation is a released ITS-REST operation, every payload is in the manifest,
every query parses with the released AQL grammar, the expanded mix reads
between 10 and 50 times per write unless the document names the published
harness it `reproduces`, no qualification limit is looser than the ruled
default, and the planned run stays within ten minutes after seeding. A
document that matches none of the pinned digests runs and is labelled
`custom` in every record and every view. `--statement` drops the interactions
your claims do not cover, and the record carries the drop.

The pinned workloads, with their expanded mixes and payloads, are listed in
[Performance workloads](perf-workloads.md), which is generated from them.
`community-vitals` reproduces the openEHR community's vital-signs harness:
its population, its closed-loop read walk (recorded apart and labelled
closed-loop, because that is the figure the community thread quotes), and then
the same reads on the ladder. `aql-mix` offers six AQL classes over the same
population.

### Comparing two CDRs on one host

```bash
veredictum perf-compare --ab mine/a.ixit.json mine/b.ixit.json \
  --root perf --workload perf/workloads/clinical-mix.yaml \
  --floor runs/aa/perf-compare.json --out runs/ab
```

Both targets are seeded from the same recipe and seed and stay resident on one
host. Each gets its settle, warm-up and scout; the shared ladder is built from
the larger knee estimate, so both sides are offered the same rates and holds;
and on each rung the two plateaus run in ABBA order, A then B, then B then A.
The run writes both records, `perf-compare.json` and `perf-compare.md`, with
every metric's ratio beside both values, in about twice a single run.
`--with-baselines <IXIT>` runs your target against the pinned EHRbase and
FerroEHR deployments the same way, each composed on your machine from its
digest-pinned images and torn down afterwards, so the ratios are an index
relative to each reference.

`perf-compare --result a.json --result b.json` aligns records you already
have into one table and names every mismatch above it: the workload digest,
the corpus recipe, the posture, the host prescription, the generator's
fingerprint and the network leg. It exits `1` when any applies.

### Why a hosted CI runner never publishes an absolute figure

An absolute millisecond describes a system and the machine it ran on
together. A hosted CI runner is shared hardware no run can pin, and two of
them have read the same code 156 % apart. On a runner, compare: run A/B
interleaved and read the ratio. A figure meant to be published names the host
prescription it claims, `--host-prescription reference-8cpu-8gb` for the idle
8-CPU, 8 GB reference host under `perf/hosts/`, and the record reads the
generator's measured CPU count, memory and network leg against it. A record on
a host that does not match says `unmatched` and names each difference.
## Where to look when something goes wrong

A red row is not presumptive evidence of a bug in your server. It is evidence
that the specification, the catalogue and the server do not all three agree, and
which one is wrong is a question that gets answered before anything is changed.
[The conformance method](methodology.md) sets out how, and what evidence an
attribution has to carry.

The [command reference](commands.md) lists every subcommand with its real flags.
