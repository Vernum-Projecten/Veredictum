#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Builds the two images the GitHub Action's CI job runs (#770), both from
# scripts/action/test/engine.Dockerfile:
#
#   veredictum-action-stand-in:local   the stand-in for the release image: this
#                                      tree's engine, its Arazzo catalogue and
#                                      its vendored specification trees
#   veredictum-action-stub-sut:local   the fake server under test, the crate's
#                                      `stub_sut` example
#
# Both compile inside the build, so the script runs wherever Docker does.
#
# Usage: scripts/action/stand-in-image.sh
set -euo pipefail

cd "$(dirname "$0")/../.."

if [[ $# -gt 0 ]]; then
  echo "usage: $0" >&2
  exit 2
fi
docker build --file scripts/action/test/engine.Dockerfile --target engine \
  --tag veredictum-action-stand-in:local .
docker build --file scripts/action/test/engine.Dockerfile --target stub-sut \
  --tag veredictum-action-stub-sut:local .
echo "stand-in-image: built veredictum-action-stand-in:local and veredictum-action-stub-sut:local"
