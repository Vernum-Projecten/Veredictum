# Running the instrument

<!-- toc -->

## 1. Check the catalogue

```bash
veredictum validate --root catalogue --specs specs/openehr
```

Zero findings is the only pass; any finding exits `1`, and `test` refuses to
start. [Gates](commands.md#validate).

## 2. Declare your deployment

The repository commits no declaration about any product: under ISO/IEC
9646-7 the supplier answers the ICS proforma (the capability folders and
`catalogue/capabilities.yaml`). Put your answers in `mine/` beside the clone;
`fixtures/declaration/` is a filled-in template for a fictional product.

- **`statement.json`** (`schemas/statement.schema.json`): product, version,
  claimed folders in `claims.capabilities`, and one arm per option in
  `options`.
- **`ixit.json`** (`schemas/ixit.schema.json`): values for rows' `declared`
  inputs, and the `environment` block a measured run reads.

Neither file holds credentials ([Logging in to the CDR](authentication.md)).

## 3. Review your claim

```bash
veredictum validate --root catalogue --specs specs/openehr \
    --statement mine/statement.json
```

Checks the statement against its schema, every claimed capability against the
folders, and one arm per option.

## 4. Drive the catalogue

```bash
export VEREDICTUM_CDR_USER=… VEREDICTUM_CDR_PASSWORD=…
veredictum test --base-url https://cdr.example/openehr/v1 --auth basic \
    --statement mine/statement.json --ixit mine/ixit.json --out out/
```

| Skip reason | When |
|---|---|
| `not-claimed` | the document's folder is not in your statement |
| `option-not-selected` | the document belongs to an option arm you did not pick |
| `not-declared` | the workflow has an administrative step and no `--admin-auth` was given |

Without a statement, everything runs and no arm is selected. A SMART on
openEHR Platform also needs `--platform-url` and scoped tokens in the IXIT
([`test`](commands.md#test)).

Outputs:

- **`out/results.json`:** one row per workflow row, passed, failed, errored or
  skipped; a red row states what was checked, the quoted sentence, the
  expectation and what came back ([Reading a result](reading-results.md)).
- **`out/recording.har`:** every exchange as HAR 1.2, response bodies
  verbatim. Keep it private.

| Exit | Meaning |
|---|---|
| `0` | every row passed or was skipped (warning rows, an unfollowed SHOULD, included) |
| `1` | a row counts against the server |
| `3` | none does, but at least one is inconclusive: the instrument could not judge it, and the row says so |
| `2` | the run could not start |

[`export`](commands.md#export) writes JUnit XML, CTRF and a Markdown summary.
`--document <ID>` narrows a run for debugging; never submit a narrowed
result. `veredictum identify --base-url <URL>` reads the server's `solution`,
`solution_version`, `vendor` and `restapi_specs_version`.

## Re-judging a recording

```bash
veredictum test --rejudge out/recording.har \
    --statement mine/statement.json --ixit mine/ixit.json --out rejudged/
```

Answers every request from the recording and writes a byte-identical
`results.json`. Pass the run's own statement, IXIT and `--document` list, or
the replay is refused. The verification pack's partial recordings are
re-judged by `cargo nextest run -p veredictum arazzo_pack` instead.

## The container image

Mount a directory uid 65532 can write as `--out`:

```bash
docker run --rm -v "$PWD/out:/work/out" \
    -e VEREDICTUM_CDR_USER -e VEREDICTUM_CDR_PASSWORD \
    ghcr.io/vernum-projecten/veredictum:<tag> \
    test --base-url https://cdr.example/openehr/v1 --auth basic --out /work/out
```

Any [Arazzo 1.1.0] runner can smoke-run the documents. Only `veredictum`
gives a verdict: citations, judges, statement selection and attribution live
in the `x-veredictum-*` extensions.

[Arazzo 1.1.0]: https://spec.openapis.org/arazzo/v1.1.0.html

## Measured performance

A performance record reads the workloads under `perf/`, earns no class and
enters no conformance verdict. No openEHR specification governs performance
(the CNF guide: "Non-functional conformance (performance, etc) is not
addressed by this guide"), so this is the instrument's own design.

### What a run does, phase by phase

`veredictum perf` offers one workload as Poisson arrivals, chart reviews as
short sessions on one EHR. Latency counts from the instant a request was due
(wrk2's correction for coordinated omission).

1. **Seeding**, before the clock: closed-loop bulk load, reported apart.
   `--corpus reuse` reuses a stored corpus, refusing one whose counts or digest
   moved.
2. **Posture canaries**, first bracket.
3. **Settle**, 15 s: `vacuumdb --all --analyze` in the database container,
   then idle.
4. **Warm-up**, 20 s, discarded.
5. **Scout**, 60 s: exponential ramp in 2 s windows; the first breach gives the
   knee estimate K̂.
6. **Drain**, at most 10 s.
7. **Ladder**: plateaus at 0.5, 0.75, 0.9 and 1.05 × K̂, each a 5 s ramp-in and
   a hold of `clamp(5,000 / rate, 30 s, 90 s)`; 0.25 × K̂ only if the lowest
   fails.
8. **Closing**: the second posture bracket.

At most ten minutes after seeding, about five when the knee is above 150
operations per second; `perf-workloads` prints the plan offline.

A plateau qualifies when every operation holds p99 ≤ 1 s (≤ 5 s for a
population AQL query) at its judged quantile, errors ≤ 0.1 %, achieved ≥ 95 %
of offered, and the generator kept up. Any `429` voids the record.

### From IXIT to `show`

The IXIT's `environment` block is required in full. The optional `containers`
block names the server and database containers and `database`; without it
server-side readings are recorded as absent.

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

`perf-judge` recomputes every number from the histograms and exits `1` naming
each differing field. The synthetic example
`app/veredictum/examples/perf-result.example.json`:

| Phase | Planned | Took | Exchanges |
|---|---:|---:|---:|
| settle | 15.0 s | 15.0 s | 0 |
| warm-up | 20.0 s | 20.0 s | 160 |
| scout | 60.0 s | 34.0 s, halted at the knee | 3,870 |
| drain | 10.0 s | 1.2 s | 0 |
| ladder | 174.0 s | 165.9 s | 25,310 |
| closing | 10.0 s | 0.4 s | 1 |

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

- **Percentiles:** p50, p90, p95, p99, p99.9 and max with 95 % binomial
  order-statistic intervals (Le Boudec, *Performance Evaluation of Computer
  and Communication Systems*, 2010, §2.2), from summed histograms.
- **`n/r`:** fewer than 10 samples above the quantile (`not_resolvable`).
- **`judged_at`:** the highest of p99, p95, p90, p50 that resolves.
- **Knee:** the highest offered rate whose plateau and all below qualify
  (Jain, *The Art of Computer Systems Performance Analysis*, 1991, ch. 3): a
  bracket, a lower bound if all qualify, or below the ladder.
- **Peak:** the highest achieved throughput, whatever its latency.
- **USL fit:** throughput against mean in flight (Gunther, *Guerrilla Capacity
  Planning*, 2007): α, β, predicted maximum, R². Never a verdict.
- **Population:** the knee over 13 to 75 peak arrivals per second per 100,000
  people ([methodology](methodology.md)), beside the corpus scale (POC, S, L
  or R: 10,000 to 10,000,000 EHRs).
- **Server-side readings:** per-container CPU, memory and I/O, database sizes,
  top `pg_stat_statements` entries, PostgreSQL settings. Context only;
  read-only sampler.
- **Scope:** steady-state capacity only; nothing about growth or behaviour
  over hours.

### What one pass does not measure

Between-run noise: two hosted runs of the same code have read 156 % apart, so
a record says `between_run_noise: unmeasured` until an A/A floor from
`perf-compare --aa` is attached with `--floor`. A ratio then counts only when
it clears `--tolerance` and the floor and the 95 % intervals do not overlap.
Nor: a p99 for a rare operation, a knee finer than the ladder, anything past
ten minutes.

### Posture

`--posture` declares `minimal` (spec surface, commits validated against the
template), `minimal-signed-digest`, `minimal-signed-pgp` or
`clinical-default` (with an audit trail). Canaries check it after seeding and
after the last plateau: version signing, commit validation via the
workload's invalid twin, authentication, `Accept-Encoding` compression, and
TLS. Audit and tenancy stay `declared-only`. A contradicting or inconsistent
reading refuses the run.

### Declaring a workload of your own

YAML against `schemas/perf-workload.schema.json`: id, version, seed, corpus
recipe (scale, compositions per EHR, template mix from `perf/payloads.yaml`),
interactions with shares and a `derivation`, and AQL query classes.

```bash
veredictum perf-workloads --root perf --catalogue catalogue --workload mine/workload.yaml
```

Loading refuses a document unless:

- shares at every level sum to exactly 100 %;
- operations are released ITS-REST operations, payloads are in the manifest,
  queries parse with the released AQL grammar;
- the mix reads 10 to 50 times per write, unless it `reproduces` a published
  harness;
- no qualification limit is looser than the ruled default;
- the run stays within ten minutes after seeding.

An unpinned document runs labelled `custom`. `--statement` drops unclaimed
interactions, and the record says so. The pinned workloads are listed in
[Performance workloads](perf-workloads.md); `community-vitals` reproduces the
openEHR community's vital-signs harness, and `aql-mix` offers six AQL classes.

### Comparing two CDRs on one host

```bash
veredictum perf-compare --ab mine/a.ixit.json mine/b.ixit.json \
  --root perf --workload perf/workloads/clinical-mix.yaml \
  --floor runs/aa/perf-compare.json --out runs/ab
```

Both targets are seeded alike and stay resident. The shared ladder comes from
the larger knee estimate, and each rung runs in ABBA order. Writes both
records, `perf-compare.json` and `perf-compare.md`, in about twice a single
run's time. `--with-baselines <IXIT>` compares against pinned EHRbase and
FerroEHR images composed locally.

`perf-compare --result a.json --result b.json` aligns existing records and
names every mismatch (workload digest, corpus recipe, posture, host
prescription, generator fingerprint, network leg), exiting `1` if any.

### Why a hosted CI runner never publishes an absolute figure

Shared runners vary too much; on one, compare A/B and read the ratio. A
published figure names a host prescription, such as
`--host-prescription reference-8cpu-8gb` (`perf/hosts/`); a host that does not
match reads `unmatched`, with each difference.

## Where to look when something goes wrong

A red row means the specification, the catalogue and the server disagree;
which one is wrong is settled before anything changes. See
[the conformance method](methodology.md).
