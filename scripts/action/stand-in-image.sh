#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Builds the stand-in for the release image that the GitHub Action's CI job
# runs (#770): this tree's engine, its Arazzo catalogue and its vendored
# specification trees, packaged by scripts/action/test/engine.Dockerfile. The
# engine compiles inside the build, so the script runs wherever Docker does.
#
# Usage: scripts/action/stand-in-image.sh [tag]
#   The tag defaults to veredictum-action-stand-in:local.
set -euo pipefail

cd "$(dirname "$0")/../.."

tag="${1:-veredictum-action-stand-in:local}"
docker build --file scripts/action/test/engine.Dockerfile --tag "$tag" .
echo "stand-in-image: built ${tag}"
