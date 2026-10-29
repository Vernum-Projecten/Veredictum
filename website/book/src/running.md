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
on a failed row. An advisory row, a SHOULD the server did not follow, is
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

## Measured performance, stress and speed

Those four steps cover functional conformance. Three more instruments produce
the other kinds of evidence, and all three read the performance schedule under
`perf/`, never the conformance catalogue.

- `veredictum perf` earns a volumetric class (POC, S, L or R) with an open-loop
  measured run and merges the measurement record into an existing measured
  `results.json`. The normative window is one hour; `--hours` extends it to 2,
  4, 6, 8 or 12, which is a stricter demonstration. Nothing shorter than the
  case exists. `veredictum verdicts` then re-derives each class verdict from
  the histograms in that record.
- `veredictum stress` climbs geometric load steps to find where the deployment
  breaks and writes `stress.json`. It is exploration only and is never a
  conformance record, which is why it is class-free by design.
- `veredictum bench` measures comparative speed against any reachable CDR and
  writes a `bench-result` document. It needs no catalogue, no IXIT and no
  statement, and it is a speed record and never a conformance record.

`perf` and `stress` both need the `environment` block in your IXIT file filled
in, every member of it, because a throughput number without the deployment
described says nothing. A conformance run reads only `exclusive_server` from
that block.
All three want an idle machine and a deployment whose resource limits match the
envelope you are claiming. A measured run on a laptop that is also running a
browser measures the browser.

### Benchmark a CDR in one command

The benchmark needs no clone and no declaration files: the packs are embedded
in the binary, pinned by digest, and described by `bench-packs`.

```bash
# The credential is read from the environment. It never rides argv.
export VEREDICTUM_CDR_PASSWORD=…

veredictum bench \
  --base-url https://cdr.example/openehr/v1 \
  --auth basic --user <user> \
  --pack community-vitals \
  --repetitions 3 \
  --with-baselines \
  --out ./bench \
  --label "Your CDR 1.2.3"
```

`community-vitals` reproduces the openEHR community's vital-signs harness and
then measures the same population a second way, open-loop, so a stall shows up
in the percentiles instead of quietly reducing the request count.
`--with-baselines` composes the two pinned reference CDRs, EHRbase and
FerroEHR, from image digests on your machine and drives the same pack at the
same seed against each, so the record carries one relative index per
reference, the only kind of number that means anything across machines. A
declared posture profile is checked by canaries on both sides of the measured
window, and a run whose deployment disagrees with its declaration is refused
rather than recorded. [`bench`](commands.md#bench) lists every flag.

## Where to look when something goes wrong

A red row is not presumptive evidence of a bug in your server. It is evidence
that the specification, the catalogue and the server do not all three agree, and
which one is wrong is a question that gets answered before anything is changed.
[The conformance method](methodology.md) sets out how, and what evidence an
attribution has to carry.

The [command reference](commands.md) lists every subcommand with its real flags.
