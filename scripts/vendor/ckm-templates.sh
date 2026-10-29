#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Vendor the official openEHR CKM template library as OPT 1.4 XML.
#
# Source: the public openEHR Clinical Knowledge Manager REST API
# (https://ckm.openehr.org/ckm/rest/v1). Every file is CKM's own
# Operational Template export, vendored verbatim with provenance.
#
# The CURATED journey pack -> perf/payloads/ckm/<slug>.opt
#   Hand-picked, all COMPOSITION-rooted, each mapped to a role in the
#   measured-performance hospital-simulation journeys. The slugs are
#   REFERENCED BY NAME from perf/payloads.yaml, the journey definitions
#   and scripts/generate-ckm-examples.sh — never rename or drop one.
#
# CKM REST PAGINATION GOTCHA (cost an afternoon once — do not relearn it):
# the list endpoints page with `?page=N&size=M`. `limit`, `pageSize`,
# `maxResults`, `offset`, `count`, `rows` are all silently IGNORED and you
# get a 20-row first page, which reads exactly like "CKM only publishes 20
# templates". Always page with page/size, and assert the count grew.
#
# Some CKM resources live in a private incubator and 404 without an account;
# those are recorded as unreachable in the provenance file rather than
# silently skipped.
#
# Usage:
#   scripts/vendor/ckm-templates.sh
#
# Example skeletons (`*.example.json`) for the curated pack are generated
# separately against the composed SUT by scripts/generate-ckm-examples.sh.
set -Eeuo pipefail

CKM="https://ckm.openehr.org/ckm/rest/v1"
OUT="perf/payloads/ckm"

# ── re-entrant single fetch (one OPT; not a user-facing mode) ─────────────
if [[ "${1:-}" == "--fetch-one" ]]; then
  cid=$2
  dest=$3
  for attempt in 1 2 3; do
    if curl -fsS --max-time 240 \
        "$CKM/templates/$cid/opt" -H "Accept: application/xml" -o "$dest"; then
      if head -c 2048 "$dest" | grep -q "<template"; then
        echo "OK   $cid $dest"
        exit 0
      fi
      rm -f "$dest"
      echo "BAD  $cid $dest (response is not an OPT)"
      exit 0
    fi
    sleep $((attempt * 2))
  done
  rm -f "$dest"
  echo "FAIL $cid $dest"
  exit 0
fi

if [[ $# -ne 0 ]]; then
  echo "usage: $0" >&2
  exit 2
fi

# cid | slug | journey role — every entry COMPOSITION-rooted (committable
# as a composition; ENTRY/CLUSTER-rooted CKM "item" templates cannot carry
# a commit and are deliberately absent).
SET=(
  # ── monitoring streams ────────────────────────────────────────────────
  "1013.26.380|vital-signs|vitals_round (full observation round)"
  # NOTE: 1013.26.61 (ODL Report Vital Signs) is EXCLUDED: its OPT carries an
  # AOM defect (an assumed_value outside its constrained code list — AM 1.4
  # Assumed_value_valid), rejected by conformant AOM validation.
  # ── laboratory / imaging pipelines ────────────────────────────────────
  "1013.26.408|generic-lab-test-result|lab_pipeline (result contribution)"
  "1013.26.2|ereferral|lab_pipeline / imaging_pipeline (order)"
  "1013.26.386|ccta-report|imaging_pipeline (report)"
  # ── medication (the eMAR loop) ────────────────────────────────────────
  "1013.26.80|eprescription-fhir|medication_round (order + administrations)"
  "1013.26.357|medicines-list|medicines_reconciliation (ward-seeded, updated)"
  # ── encounter documents & summaries ───────────────────────────────────
  "1013.26.191|gp-data-set|correction target (ward-seeded, amended)"
  "1013.26.376|international-patient-summary|admission / discharge summary"
  "1013.26.360|problem-list|admission (problem list)"
  # ── specialist & registry reporting ───────────────────────────────────
  "1013.26.199|bc-breast-cancer-report|specialist_report (cancer synoptic report)"
  "1013.26.40|treat-registry-report|registry_submission (registry export)"
  # ── public-health surveillance ────────────────────────────────────────
  "1013.26.377|sars-event-notification|public_health_notification (statutory notification)"
  "1013.26.282|covid19-infection-report|public_health_notification (confirmed-case follow-up)"
  "1013.26.988|poisoning-case-investigation|case_investigation"
  "1013.26.980|diphtheria-case-investigation|case_investigation"
  # ── scale probe: the largest OPT the CKM publishes (5.2 MB, #1462) ────
  "1013.26.977|congenital-syphilis-case-investigation|case_investigation (largest published form — the large-payload scale probe)"
)

mkdir -p "$OUT"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

STAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)

# ── the curated journey pack ──────────────────────────────────────────────
PROV="$OUT/PROVENANCE.md"
{
  echo "# CKM template pack — provenance"
  echo
  echo "Vendored from the official openEHR CKM (\`$CKM\`) by"
  echo "\`scripts/vendor/ckm-templates.sh\` on $STAMP."
  echo "Each file is CKM's own OPT export for the cited template, verbatim."
  echo "Example skeletons (\`*.example.json\`) are generated once against the"
  echo "composed SUT by \`scripts/generate-ckm-examples.sh\` and committed"
  echo "(byte-identical payload ground for every SUT; never fetched at run"
  echo "time). Payload entries: \`perf/payloads.yaml\`."
  echo
  echo "The **curated journey pack** below is referenced by slug from"
  echo "\`perf/payloads.yaml\`, the journey definitions and the example"
  echo "generator — the slugs are a stable contract."
  echo
  echo "## Licensing"
  echo
  echo "CKM publishes no repository-level license; each OPT embeds its source"
  echo "archetypes' \`licence\` metadata (predominantly CC-BY-SA 3.0 where"
  echo "stated — see the individual file). Vendored verbatim, so authorship"
  echo "and licence metadata ride along; root reference copy:"
  echo "\`LICENSE-CC-BY-SA-3.0\`."
  echo
  echo "| cid | slug | display name | status | modified | journey role |"
  echo "|---|---|---|---|---|---|"
} > "$PROV"

for entry in "${SET[@]}"; do
  IFS='|' read -r cid slug role <<< "$entry"
  echo "==> curated $cid ($slug)"
  meta=$(curl -fsS "$CKM/templates/$cid" -H "Accept: application/json")
  # one field per call: display names contain spaces AND pipes, so a
  # word-split read of a combined line is not safe here
  name=$(printf '%s' "$meta" | jq -r '.resourceMainDisplayName | gsub("\\|"; "/")')
  status=$(printf '%s' "$meta" | jq -r '.status')
  modified=$(printf '%s' "$meta" | jq -r '.modificationTime')
  bash "$0" --fetch-one "$cid" "$OUT/$slug.opt" | tee -a "$WORK/curated.log"
  grep -q "^OK   $cid " "$WORK/curated.log" || {
    echo "::error::$cid ($slug) did not yield an OPT — the curated pack is a contract" >&2
    exit 1
  }
  echo "| $cid | $slug | $name | $status | $modified | $role |" >> "$PROV"
done
echo "==> curated pack: $(grep -c '^OK' "$WORK/curated.log") OPTs → $OUT"
