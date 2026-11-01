# Command reference

<!-- toc -->

Each code block is the binary's own `veredictum <command> --help`, rendered by
`scripts/render/commands-md.sh`, in the order `veredictum --help` lists them.
The notes under each are hand-written in `website/book/commands/`.

| Command | Does |
|---|---|
| `validate` | checks the catalogue |
| `test` | drives the catalogue, or re-judges its recording |
| `export` | renders a results document as JUnit XML, CTRF and a Markdown summary |
| `perf` | measures a workload against a live server |
| `perf-compare` | compares performance records, or runs two servers interleaved |
| `perf-judge` | re-derives a performance record |
| `verify` | checks a conformance certificate offline |

The rest render or explore.
