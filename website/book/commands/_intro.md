# Command reference

<!-- toc -->

Every code block below is `veredictum <command> --help` from the binary itself:
this page is rendered by `scripts/render/commands-md.sh` out of clap's own
output, so a flag cannot land in the binary and go missing here. The sections
follow the order `veredictum --help` lists them in, and the prose under each
one is written by hand and lives in `website/book/commands/`.

Two commands make the conformance record: `validate` checks the catalogue,
and `test` drives it or re-judges its recording. `export`
renders a results document as JUnit XML, CTRF and a Markdown summary for a CI
system. `perf` measures a workload against a live server, `perf-compare`
compares performance records or runs two servers interleaved, `perf-judge`
re-derives a performance record, `verify` checks a conformance certificate
offline, and the rest render or explore.
