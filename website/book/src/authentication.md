# Logging in to the CDR

<!-- toc -->

| Your CDR wants | Pass | Set |
|---|---|---|
| no login | `--auth none` (the default) | nothing |
| a username and password | `--auth basic` | `VEREDICTUM_CDR_USER` and `VEREDICTUM_CDR_PASSWORD` |
| a bearer token | `--auth bearer` | `VEREDICTUM_CDR_TOKEN` |

`test` and `identify` read these; `perf` and `perf-compare` read the variables
the IXIT names. The [GitHub Action](github-action.md) takes the same values as
`auth` and `admin-auth`.

## The administrative principal

Workflows that delete EHRs through the administrative API need:

| The administrator logs in with | Pass | Set |
|---|---|---|
| no login | `--admin-auth none` (the default) | nothing |
| a username and password | `--admin-auth basic` | `VEREDICTUM_CDR_ADMIN_USER` and `VEREDICTUM_CDR_ADMIN_PASSWORD` |
| a bearer token | `--admin-auth bearer` | `VEREDICTUM_CDR_ADMIN_TOKEN` |

With `none`, those workflows are skipped as `not-declared`. SMART scoped
tokens come from `VEREDICTUM_SCOPED_TOKEN_<NAME>` ([`test`](commands.md#test)).

## Rules

- **Files:** every variable has a `_FILE` form, such as
  `VEREDICTUM_CDR_PASSWORD_FILE=/run/secrets/cdr-password`. Setting both stops
  the run. One trailing line ending is dropped.
- **Empty means unset:** a missing CI secret stops the run with exit `2`:

  ```text
  --auth basic needs VEREDICTUM_CDR_USER and VEREDICTUM_CDR_PASSWORD (or their _FILE forms), and VEREDICTUM_CDR_PASSWORD is not set, nor is VEREDICTUM_CDR_PASSWORD_FILE (an empty value counts as unset)
  ```

- **Never an argument, never recorded:** arguments leak into the process list
  and logs. Results carry no credential, and the HAR shows `[not recorded]`
  for every `Authorization` value.

## Examples

```bash
# no login
veredictum test --base-url https://cdr.example/openehr/v1 --out out/

# username and password
export VEREDICTUM_CDR_USER=… VEREDICTUM_CDR_PASSWORD=…
veredictum test --base-url https://cdr.example/openehr/v1 --auth basic --out out/

# bearer tokens for client and administrator, from files
export VEREDICTUM_CDR_TOKEN_FILE=/run/secrets/cdr-token
export VEREDICTUM_CDR_ADMIN_TOKEN_FILE=/run/secrets/cdr-admin-token
veredictum test --base-url https://cdr.example/openehr/v1 \
    --auth bearer --admin-auth bearer --out out/
```

In the container, pass each variable by name with `-e`:

```bash
docker run --rm -v "$PWD/out:/work/out" \
    -e VEREDICTUM_CDR_USER -e VEREDICTUM_CDR_PASSWORD \
    ghcr.io/vernum-projecten/veredictum:<tag> \
    test --base-url https://cdr.example/openehr/v1 --auth basic --out /work/out
```
