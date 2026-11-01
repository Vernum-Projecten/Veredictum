#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# Every canonical-XML request body of an accepting XML-write arm validates
# against the vendored ITS-XML schemas.
#
# ITS-REST `Resources.md` §XML Format: "both request payloads and responses
# MUST conform to the published XSDs". The documents an `*-xml-write-accepted`
# arm of a catalogue's `capabilities.yaml` names send XML a service is expected
# to process, so each body is held to a schema here, chosen by its root's
# namespace. Every Arazzo catalogue root of the tree is read
# (`catalogue-roots.sh`, #983), and an arm naming a document its own root does
# not hold is a finding:
#
# - `http://schemas.openehr.org/v1`, the 1.0.2 lineage: a `composition` root
#   against `ALL/Composition.xsd`, an `items` root against `ALL/Structure.xsd`.
# - `http://schemas.openehr.org/v2`, the 2.0.0 lineage: an `items` root
#   against `RM/Release-1.1.0/documents/Structure.xsd` composed with
#   `Ehr.xsd` and `Demographic.xsd`, because the published document schema
#   reaches neither EHR_STATUS nor the PARTY types (upstream report #759).
#   That lineage writes its archetypeNodeId facet in the Perl dialect, with
#   `(?:` groups and a `\w` that holds the underscore, which XML Schema Part 2
#   Appendix F does not define; the check reads that one facet as written, in
#   a copy, with `(?:` as `(` and `\w` as `[\w_]`. The vendored files are
#   never changed.
#
# Any other root, an accepting-arm document that sends no XML body, and a body
# that does not validate are findings. xmllint (libxml2) does the validating
# and yq reads the documents. No openEHR spec governs the check itself — our
# own design.
#
# Usage: scripts/checks/xml-write-bodies.sh               # every catalogue root
#        scripts/checks/xml-write-bodies.sh --self-test   # seeded violations
# Env:   XML_WRITE_BODIES_TREE the git tree whose catalogue roots are scanned
#                              (default: the repository)
set -euo pipefail
cd "$(dirname "$0")/../.."
# shellcheck source=scripts/checks/catalogue-roots.sh
source scripts/checks/catalogue-roots.sh

ROOT="$PWD"
NSV1="$ROOT/specs/its-xml-schemas/its-xml-1.0.2-nsv1/ALL"
NSV2="$ROOT/specs/its-xml-schemas/its-xml-2.0.0-nsv2"

for tool in xmllint yq perl; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "xml-write-bodies: $tool is not installed" >&2
    exit 2
  fi
done

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# The 2.0.0 schema set, composed once, with the Perl-dialect facet read.
compose_nsv2() {
  local dir="$WORK/nsv2"
  mkdir -p "$dir/RM" "$dir/BASE"
  cp -R "$NSV2/RM/Release-1.1.0" "$dir/RM/Release-1.1.0"
  cp -R "$NSV2/BASE/latest" "$dir/BASE/latest"
  perl -i -pe 'if (/\(\?:/) { s/\(\?:/(/g; s/\[\\w-\]/[\\w_\\-]/g; s/\\w\+/[\\w_]+/g; }' \
    "$dir/BASE/latest/BaseTypes.xsd"
  cat > "$dir/RM/Release-1.1.0/composed.xsd" <<'XSD'
<?xml version="1.0" encoding="utf-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns="http://schemas.openehr.org/v2"
           targetNamespace="http://schemas.openehr.org/v2" elementFormDefault="qualified">
  <xs:include schemaLocation="documents/Structure.xsd"/>
  <xs:include schemaLocation="Ehr.xsd"/>
  <xs:include schemaLocation="Demographic.xsd"/>
</xs:schema>
XSD
  echo "$dir/RM/Release-1.1.0/composed.xsd"
}
NSV2_COMPOSED="$(compose_nsv2)"

# validate <label> <body file>: prints a finding and returns 1 when the body
# has no known root or does not validate.
validate() {
  local label="$1" body="$2" ns name schema
  if ! ns="$(xmllint --xpath 'namespace-uri(/*)' "$body" 2>/dev/null)" ||
    ! name="$(xmllint --xpath 'local-name(/*)' "$body" 2>/dev/null)"; then
    echo "xml-write-bodies: $label: the body is not well-formed XML" >&2
    return 1
  fi
  case "$ns|$name" in
    "http://schemas.openehr.org/v1|composition") schema="$NSV1/Composition.xsd" ;;
    "http://schemas.openehr.org/v1|items") schema="$NSV1/Structure.xsd" ;;
    "http://schemas.openehr.org/v2|items") schema="$NSV2_COMPOSED" ;;
    *)
      echo "xml-write-bodies: $label: no vendored ITS-XML schema declares the root {$ns}$name" >&2
      return 1
      ;;
  esac
  if ! xmllint --noout --schema "$schema" "$body" 2>"$WORK/err"; then
    sed "s|^|xml-write-bodies: $label: |" "$WORK/err" >&2
    return 1
  fi
}

# The XML request bodies of one Arazzo document, one file each under $2;
# prints how many.
bodies() {
  local doc="$1" out="$2" count i
  local select='explode(.) | [.workflows[].steps[] | select(.requestBody.contentType == "application/xml") | .requestBody.payload]'
  count="$(yq e "$select | length" "$doc")"
  for ((i = 0; i < count; i++)); do
    yq e "$select | .[$i]" "$doc" > "$out/$i.xml"
  done
  echo "$count"
}

# check_document <arazzo document>: every XML body validates, and there is one.
check_document() {
  local doc="$1" dir count i found=0
  dir="$(mktemp -d "$WORK/doc.XXXXXX")"
  count="$(bodies "$doc" "$dir")"
  if [[ "$count" -eq 0 ]]; then
    echo "xml-write-bodies: $doc: an accepting XML-write arm names it, and it sends no XML body" >&2
    return 1
  fi
  for ((i = 0; i < count; i++)); do
    validate "$doc body $((i + 1))" "$dir/$i.xml" || found=1
  done
  return "$found"
}

# The documents every `*-xml-write-accepted` arm of the catalogue root $1
# names, as files of that root.
accepted_documents() {
  local root="$1" id file
  yq e '.options[].arms | to_entries[] | select(.key | test("-xml-write-accepted$")) | .value.documents[]' \
    "$root/capabilities.yaml" |
    while read -r id; do
      file="$(find "$root/cases" -name "$id.arazzo.yaml" | head -n 1)"
      if [[ -z "$file" ]]; then
        echo "xml-write-bodies: the arm document $id is in no file of $root" >&2
        echo "MISSING"
      else
        echo "$file"
      fi
    done
}

# scan <git tree>: every accepting-arm document of every catalogue root in the
# tree validates, and the tree holds a root.
scan() {
  local tree="$1" roots root doc found=0 checked=0
  roots="$(catalogue_roots "$tree")"
  if [[ -z "$roots" ]]; then
    echo "xml-write-bodies: no catalogue root (capabilities.yaml beside cases/) in $tree" >&2
    return 1
  fi
  while IFS= read -r root; do
    while read -r doc; do
      if [[ "$doc" == "MISSING" ]]; then
        found=1
        continue
      fi
      check_document "$doc" || found=1
      checked=$((checked + 1))
    done < <(accepted_documents "$tree/$root")
  done <<< "$roots"
  if [[ "$found" -ne 0 ]]; then
    return 1
  fi
  echo "xml-write-bodies: the XML bodies of $checked accepting-arm documents validate"
}

if [[ "${1:-}" == "--self-test" ]]; then
  seeds="$WORK/seeds"
  mkdir -p "$seeds"
  ok=1
  expect() { # expect <pass|fail> <name> <xml>
    printf '%s\n' "$3" > "$seeds/$2.xml"
    if validate "seed $2" "$seeds/$2.xml" 2>/dev/null; then got=pass; else got=fail; fi
    if [[ "$got" != "$1" ]]; then
      echo "xml-write-bodies self-test: seed $2 should $1 and did not" >&2
      ok=0
    fi
  }
  V1='xmlns="http://schemas.openehr.org/v1" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"'
  V2='xmlns="http://schemas.openehr.org/v2" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"'
  AD='<archetype_details><archetype_id><value>openEHR-EHR-EHR_STATUS.generic.v1</value></archetype_id><rm_version>1.1.0</rm_version></archetype_details>'
  expect pass folder "<items $V1 xsi:type=\"FOLDER\" archetype_node_id=\"openEHR-EHR-FOLDER.generic.v1\"><name><value>root</value></name></items>"
  expect fail folder_no_name "<items $V1 xsi:type=\"FOLDER\" archetype_node_id=\"openEHR-EHR-FOLDER.generic.v1\"></items>"
  expect pass status "<items $V2 xsi:type=\"EHR_STATUS\" archetype_node_id=\"openEHR-EHR-EHR_STATUS.generic.v1\"><name><value>s</value></name>$AD<subject/><is_queryable>true</is_queryable><is_modifiable>true</is_modifiable></items>"
  expect fail status_no_flag "<items $V2 xsi:type=\"EHR_STATUS\" archetype_node_id=\"openEHR-EHR-EHR_STATUS.generic.v1\"><name><value>s</value></name>$AD<subject/><is_queryable>true</is_queryable></items>"
  expect fail status_in_v1 "<items $V1 xsi:type=\"EHR_STATUS\" archetype_node_id=\"openEHR-EHR-EHR_STATUS.generic.v1\"><name><value>s</value></name><subject/><is_queryable>true</is_queryable><is_modifiable>true</is_modifiable></items>"
  expect fail class_root "<ehr_status $V1 archetype_node_id=\"openEHR-EHR-EHR_STATUS.generic.v1\"><name><value>s</value></name></ehr_status>"
  expect fail not_xml "<items $V2"
  printf 'arazzo: 1.1.0\nworkflows:\n  - workflowId: w\n    steps:\n      - stepId: s\n        requestBody:\n          contentType: application/json\n          payload: {}\n' > "$seeds/no_xml.arazzo.yaml"
  if check_document "$seeds/no_xml.arazzo.yaml" 2>/dev/null; then
    echo "xml-write-bodies self-test: a document with no XML body passed" >&2
    ok=0
  fi
  # Two roots: the first's arm names a document it holds, whose body
  # validates; the second's, three levels down, names one it does not hold.
  tree="$WORK/tree"
  mkdir -p "$tree/catalogue/cases/ehr" "$tree/fixtures/console/catalogue/cases/ehr"
  git -C "$tree" init -q
  for root in catalogue fixtures/console/catalogue; do
    printf 'options:\n  xml-write:\n    arms:\n      ehr-xml-write-accepted:\n        documents: [good]\n' > "$tree/$root/capabilities.yaml"
  done
  printf 'options:\n  xml-write:\n    arms:\n      ehr-xml-write-accepted:\n        documents: [gone]\n' > "$tree/fixtures/console/catalogue/capabilities.yaml"
  {
    printf 'arazzo: 1.1.0\nworkflows:\n  - workflowId: w\n    steps:\n      - stepId: s\n        requestBody:\n          contentType: application/xml\n          payload: |\n'
    printf '            <items %s xsi:type="FOLDER" archetype_node_id="openEHR-EHR-FOLDER.generic.v1"><name><value>root</value></name></items>\n' "$V1"
  } > "$tree/catalogue/cases/ehr/good.arazzo.yaml"
  cp "$tree/catalogue/cases/ehr/good.arazzo.yaml" "$tree/fixtures/console/catalogue/cases/ehr/good.arazzo.yaml"
  out="$(scan "$tree" 2>&1 || true)"
  if [[ "$out" != *"the arm document gone is in no file of $tree/fixtures/console/catalogue"* || "$out" == *"good.arazzo.yaml body"* ]]; then
    echo "xml-write-bodies self-test: the second root's stale arm document was not refused alone: $out" >&2
    ok=0
  fi
  mkdir -p "$WORK/empty"
  git -C "$WORK/empty" init -q
  if scan "$WORK/empty" >/dev/null 2>&1; then
    echo "xml-write-bodies self-test: a tree with no catalogue root passed" >&2
    ok=0
  fi
  if [[ "$ok" -eq 1 ]]; then
    echo "xml-write-bodies self-test: every seeded violation is refused, in every catalogue root"
    exit 0
  fi
  exit 1
fi

scan "${XML_WRITE_BODIES_TREE:-.}"
