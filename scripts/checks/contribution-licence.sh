#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Every pull request from a person records acceptance of the contribution
# licensing terms (CONTRIBUTING.md § Licensing of contributions).
#
# The terms turn a contribution into part of one work under one Licensor; a
# merge without a recorded acceptance is a line whose relicensing right was
# never granted. A checkbox in the pull request template is the record, and
# this gate is what makes it binding: a body without the ticked line fails,
# whether the box was left empty or the section deleted.
#
# Bots cannot accept terms, and their changes carry no copyrightable expression
# of their own (dependency and pin bumps); the workflow skips them by author
# type before this script runs.
#
# Usage: PR_BODY="$body" scripts/checks/contribution-licence.sh   (no arguments)
#
# With PR_BODY unset there is no pull request to judge, so the script checks
# itself instead: a ticked line passes, an unticked line and a body without the
# line fail. That is what lets the local gate battery run this gate rather than
# skip it, and what a CI run never does, because CI always sets PR_BODY.
set -euo pipefail

if [ "$#" -ne 0 ]; then
  echo "usage: $0   (reads the pull request body from PR_BODY; takes no arguments)" >&2
  exit 2
fi

if [ -z "${PR_BODY+set}" ]; then
  self="$0"
  PR_BODY='- [x] I accept the terms in [CONTRIBUTING.md § Licensing of contributions](x)' "$self" >/dev/null
  if PR_BODY='- [ ] I accept the terms in [CONTRIBUTING.md § Licensing of contributions](x)' "$self" 2>/dev/null; then
    echo "self-test: an unticked line passed" >&2; exit 1
  fi
  if PR_BODY='Closes #1' "$self" 2>/dev/null; then
    echo "self-test: a body without the line passed" >&2; exit 1
  fi
  echo "ok: contribution-licence self-test (a ticked line passes, an unticked or missing line fails)"
  exit 0
fi

readonly ACCEPTED='^[[:space:]]*[-*] \[[xX]\] I accept the terms in \[CONTRIBUTING\.md § Licensing of contributions\]'
readonly UNTICKED='^[[:space:]]*[-*] \[ \] I accept the terms in \[CONTRIBUTING\.md § Licensing of contributions\]'

body="${PR_BODY:-}"
if printf '%s\n' "$body" | grep -qE "$ACCEPTED"; then
  echo "ok: the contribution licensing terms are accepted in the pull request body"
  exit 0
fi
if printf '%s\n' "$body" | grep -qE "$UNTICKED"; then
  echo "error: the licensing checkbox in the pull request body is not ticked" >&2
else
  echo "error: the pull request body carries no licensing acceptance line" >&2
  echo "       (the pull request template's 'Licensing of contributions' section was removed)" >&2
fi
echo >&2
echo "Tick the box under 'Licensing of contributions' in the pull request" >&2
echo "description (edit the description; the check re-runs on the edit). The" >&2
echo "terms are CONTRIBUTING.md § Licensing of contributions." >&2
exit 1
