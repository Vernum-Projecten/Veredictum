# Logging in to the CDR

<!-- toc -->

A CDR either needs no login, takes a username and password over HTTP Basic, or
takes a bearer token. `--auth` tells the instrument which one yours wants, and
the credential itself comes from an environment variable:

| Your CDR wants | Pass | Set |
|---|---|---|
| no login | `--auth none` (the default) | nothing |
| a username and password | `--auth basic` | `VEREDICTUM_CDR_USER` and `VEREDICTUM_CDR_PASSWORD` |
| a bearer token | `--auth bearer` | `VEREDICTUM_CDR_TOKEN` |

`test` and `identify` both read these. In the [GitHub Action](github-action.md)
the inputs are `auth` and `admin-auth`, with the same values.

## The administrative principal

Some workflows delete EHRs through the administrative API, which a CDR usually
allows only an administrator. `--admin-auth` names how that principal logs in, with its own
variables:

| The administrator logs in with | Pass | Set |
|---|---|---|
| no login | `--admin-auth none` (the default) | nothing |
| a username and password | `--admin-auth basic` | `VEREDICTUM_CDR_ADMIN_USER` and `VEREDICTUM_CDR_ADMIN_PASSWORD` |
| a bearer token | `--admin-auth bearer` | `VEREDICTUM_CDR_ADMIN_TOKEN` |

With `--admin-auth none`, every workflow with an administrative step is skipped
as `not-declared`. A SMART on openEHR Platform's scoped tokens are declared in
the IXIT and read from `VEREDICTUM_SCOPED_TOKEN_<NAME>` ([`test`](commands.md#test)).

## Reading a secret from a file

Every variable above has a `_FILE` form: `VEREDICTUM_CDR_PASSWORD_FILE=/run/secrets/cdr-password`
reads the password from that file, the way container secrets are mounted. Set
the variable or its `_FILE` form, never both; with both set the run stops. One
trailing line ending in the file is dropped.

An empty variable counts as unset. A CI secret that does not exist expands to
an empty string, and the run then stops with exit status `2` instead of
presenting an empty credential to your CDR. The error names the flag, the mode
and the variables it reads:

```text
--auth basic needs VEREDICTUM_CDR_USER and VEREDICTUM_CDR_PASSWORD (or their _FILE forms), and VEREDICTUM_CDR_PASSWORD is not set, nor is VEREDICTUM_CDR_PASSWORD_FILE (an empty value counts as unset)
```

## Why the environment

A command-line argument is visible to every user of the machine through the
process list, and it lands in shell history and CI logs. An environment
variable or a mounted file is not, so the instrument never takes a credential
as an argument. The credential is read once, when the run starts, and it is
never recorded: the results carry no credential, and the HAR recording shows
`[not recorded]` in place of every `Authorization` value.

## Examples

No login:

```bash
veredictum test --base-url https://cdr.example/openehr/v1 --out out/
```

A username and password:

```bash
export VEREDICTUM_CDR_USER=… VEREDICTUM_CDR_PASSWORD=…
veredictum test --base-url https://cdr.example/openehr/v1 --auth basic --out out/
```

A bearer token for the client and another for the administrator, from files:

```bash
export VEREDICTUM_CDR_TOKEN_FILE=/run/secrets/cdr-token
export VEREDICTUM_CDR_ADMIN_TOKEN_FILE=/run/secrets/cdr-admin-token
veredictum test --base-url https://cdr.example/openehr/v1 \
    --auth bearer --admin-auth bearer --out out/
```

In the container image, pass each variable by name with `-e`, so its value is
taken from your shell and never typed on the command line:

```bash
docker run --rm -v "$PWD/out:/work/out" \
    -e VEREDICTUM_CDR_USER -e VEREDICTUM_CDR_PASSWORD \
    ghcr.io/vernum-projecten/veredictum:<tag> \
    test --base-url https://cdr.example/openehr/v1 --auth basic --out /work/out
```
