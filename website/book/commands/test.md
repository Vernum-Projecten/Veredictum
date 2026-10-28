`test` drives the Arazzo catalogue under `catalogue/`, where `run` drives the
case-core catalogue under `artifacts/`. The two live side by side until the
cut-over to the Arazzo catalogue removes the case-core one.

**What runs.** Every document under `cases/<capability>/`, in path order. The
shared setups under `cases/_setup/` are not cases: a setup runs only where a
case names it in `dependsOn`. `--document <ID>` narrows the run to named
documents, which is how you re-drive one family while working on a fix; a
narrowed `results.json` is a working tool, never the record you submit.

**What the party declares.** The statement's `options` list picks one arm of
each option in `capabilities.yaml`, and the documents of every other arm are
skipped as `option-not-selected`. A token that names no arm, or two arms of
one option, stops the run. The IXIT answers the `declared` inputs a rows file
reads. A workflow with an administrative step needs the administrative
principal: without `--admin-auth`, it is skipped as `not-declared`.

**Credentials.** Each principal's secrets are read from the environment, from
the variable itself or from a file named by the variable with `_FILE`
appended, which is how container secrets are mounted. Setting both is
refused. Neither credential is recorded: the HAR carries `[not recorded]` in
place of every `Authorization` value, and records only whether the
administrative principal was supplied, so a re-judgement skips the same
workflows.

**Re-judging.** `--rejudge <RECORDING>` answers every request from the
recording and reuses every recorded input, so `results.json` comes out
byte-identical. Pass the root, statement, IXIT and `--document` list the
recorded run was given; a replay that does not line up with the recording is
refused rather than judged.

**In the container image.** The image's entry point hands any argument other
than `healthcheck` to the engine, and `VEREDICTUM_CATALOGUE` points `--root`
at the catalogue the image ships:

```bash
docker run --rm -v "$PWD/out:/work/out" \
    -e VEREDICTUM_SUT_TOKEN \
    ghcr.io/vernum-projecten/veredictum:<tag> \
    test --base-url https://cdr.example/openehr/v1 --auth bearer --out /work/out
```

The image runs as uid 65532, so the mounted directory must be writable by it.

**Other Arazzo tools.** The documents are plain Arazzo 1.1.0, so any Arazzo
tool reads them and a workflow runner can smoke-run them. The verdict is
`test`'s alone: the citations, the judges, the statement-driven selection and
the attribution of every red row live in the `x-veredictum-*` extensions and
in this pipeline, which a generic runner ignores.
