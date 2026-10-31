`badge` draws the static badges a vendor shows in a README, from the result
document it is given. Each badge is a pure function of that document, so
drawing the same document twice writes the same bytes. The SVG is measured
with a fixed table of character widths, so no font and no network is read.

| Document | Files written |
|---|---|
| an Arazzo `results.json` | `base`, `options`, `profile` and `instrument`, each `.svg` and `.json` |
| a `perf-result.json` | `perf.svg` and `perf.json` |

```bash
veredictum badge --results conformance/results.json --out conformance/badges
```

The `.json` file is a Shields.io endpoint document, so a badge can also be
served by Shields from wherever the file is published. Every badge reports the
vendor's own run and is never a certificate. [Badges](badges.md) has what each
badge says, the colour rules, and how to publish them from GitHub, GitLab or
any other CI.
