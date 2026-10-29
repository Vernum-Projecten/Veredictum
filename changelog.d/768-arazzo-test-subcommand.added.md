- `veredictum test` drives the Arazzo catalogue against a server and writes
  `results.json` and the HAR recording, or re-judges a recording offline into
  the same bytes (#768). It gates the whole catalogue first and runs every
  document except the shared setups. A statement must claim at least one
  capability folder in `claims.capabilities`, and every workflow of a
  document in an unclaimed folder is skipped `not-claimed`, citing the
  statement schema's claims clause, which now describes the rule; setups a
  claimed document depends on still run. The statement's `options` select
  option arms. An unknown folder or arm, or two arms of one option, stops the
  run. Without a statement everything runs. The administrative workflows are
  skipped when `--admin-auth` supplies no principal. Credentials come from
  environment variables or from files named by the same variables with
  `_FILE` appended, and none is recorded. The exit status is `0` when every
  row passed or was skipped (advisory rows included), `1` when a row failed
  or errored, and `2` when the run could not start. The `arazzo_spike`
  example it replaces is deleted.
- The container image runs engine subcommands through its entry point:
  `docker run <image> test …` drives the catalogue the image ships, which
  `VEREDICTUM_CATALOGUE` names, into a mounted output directory, and the
  image smoke test and the CI image build both run it. A catalogue run's
  results carry `document: catalogue`, which the Arazzo results schema now
  describes, and the HAR recording records whether the administrative
  principal was supplied.
