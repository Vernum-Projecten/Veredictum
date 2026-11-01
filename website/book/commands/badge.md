`badge` draws a vendor's static README badges from a result document. Each
badge is a pure function of that document, so the same input writes the same
bytes. Text is measured with a fixed width table, so no font or network is
read.

| Document | Files written |
|---|---|
| an Arazzo `results.json` | `base`, `options`, `profile` and `instrument`, each `.svg` and `.json` |
| a `perf-result.json` | `perf`, `noise` and `instrument`, each `.svg` and `.json` |

```bash
veredictum badge --results conformance/results.json --out conformance/badges
```

The `.json` file is a Shields.io endpoint document, so Shields can serve the
badge from wherever the file is published. Every badge reports the vendor's
own run and is never a certificate. [Badges](badges.md) covers what each says,
the colour rules, and publishing from GitHub, GitLab or any other CI.
