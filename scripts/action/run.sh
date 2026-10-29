#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The GitHub Action's one step (`action.yml` at the repository root, #770).
#
# It runs the instrument's container image twice: `test` drives the shipped
# Arazzo catalogue against the server under test and writes `results.json` and
# the HAR recording into the output directory, then `export` writes the JUnit
# XML, CTRF and Markdown views beside them. The Markdown summary is appended to
# the job summary. The step exits with the run's own status: 0 when every row
# passed or was skipped, 1 when a verdict-bearing row failed or errored, 2 when
# the run could not start or finish.
#
# No openEHR specification governs this. It is our own design.
#
# Inputs arrive as environment variables, set by `action.yml` from `inputs.*`,
# so no input value is ever spliced into shell source:
#
#   IN_BASE_URL     the server's base URL (required)
#   IN_STATEMENT    path to the party's statement (optional)
#   IN_IXIT         path to the party's IXIT (optional)
#   IN_AUTH         none | basic | bearer, checked by the engine
#   IN_ADMIN_AUTH   none | basic | bearer, checked by the engine
#   IN_DOCUMENTS    document ids separated by white space (optional)
#   IN_OUT_DIR      the output directory, created when missing
#   IN_IMAGE        the image reference; empty means the release image of the
#                   version this action's own tree carries
#   IN_PULL         true | false: pull the image before resolving it
#   IN_NETWORK      the container network the run joins
#
# Credentials are read from the variables the engine reads
# (VEREDICTUM_SUT_USER, VEREDICTUM_SUT_PASSWORD, VEREDICTUM_SUT_TOKEN and the
# VEREDICTUM_ADMIN_ trio), or from files named by the same variables with
# `_FILE` appended. A credential is handed to the container by NAME, so its
# value never appears on a command line, and an EMPTY variable is not handed
# over at all: a secret that does not exist in the caller's repository expands
# to an empty string, and the run must then stop on a missing credential
# rather than present an empty one.
#
# Usage:
#   scripts/action/run.sh               # as the action runs it
#   scripts/action/run.sh --self-test   # against a stub `docker`, no runtime
set -euo pipefail

readonly CREDENTIALS=(
  VEREDICTUM_SUT_USER
  VEREDICTUM_SUT_PASSWORD
  VEREDICTUM_SUT_TOKEN
  VEREDICTUM_ADMIN_USER
  VEREDICTUM_ADMIN_PASSWORD
  VEREDICTUM_ADMIN_TOKEN
)
readonly RELEASE_IMAGE=ghcr.io/vernum-projecten/veredictum

# output <name> <value>: a step output, when the runner provides the file.
output() {
  if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
    printf '%s=%s\n' "$1" "$2" >> "$GITHUB_OUTPUT"
  fi
}

# summary <line>: one line of the job summary, when the runner provides it.
summary() {
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    printf '%s\n' "$1" >> "$GITHUB_STEP_SUMMARY"
  fi
}

# refuse <message>: the run could not start; exit status 2, as the engine's.
refuse() {
  echo "::error::$1" >&2
  summary "Veredictum could not start the run: $1"
  output exit-code 2
  exit 2
}

# absolute <path>: the path made absolute against the working directory.
absolute() {
  case "$1" in
    /*) printf '%s\n' "$1" ;;
    *) printf '%s/%s\n' "$PWD" "$1" ;;
  esac
}

# The bare version the action's own tree carries: `[package] version` of the
# engine manifest, which the release pipeline holds equal to the tag. The
# image's semver tag is that bare version (no leading `v`).
action_version() {
  awk -F'"' '/^\[package\]/{p=1} p && /^version = /{print $2; exit}' \
    "${GITHUB_ACTION_PATH:?}/app/veredictum/Cargo.toml"
}

main() {
  local base_url="${IN_BASE_URL:-}" out_dir="${IN_OUT_DIR:-veredictum-results}"
  local image="${IN_IMAGE:-}" pull="${IN_PULL:-true}" network="${IN_NETWORK:-host}"
  local auth="${IN_AUTH:-none}" admin_auth="${IN_ADMIN_AUTH:-none}"
  local version="" image_id repo_digests
  local -a run_args=() test_args=()

  [[ -n "$base_url" ]] || refuse "the base-url input is required"
  case "$pull" in
    true | false) ;;
    *) refuse "the pull input is \"${pull}\" (accepted: true, false)" ;;
  esac
  [[ -n "$network" ]] || refuse "the network input is empty"

  if [[ -z "$image" ]]; then
    version="$(action_version)"
    [[ -n "$version" ]] || refuse "could not read the version this action carries"
    image="${RELEASE_IMAGE}:${version}"
  fi
  if [[ "$pull" == true ]]; then
    docker pull --quiet "$image" >/dev/null || refuse "could not pull ${image}"
  fi
  # Both runs use the image ID, so they run the same bytes even if the tag
  # moves between them.
  image_id="$(docker image inspect --format '{{.Id}}' "$image")" \
    || refuse "the image ${image} is not present"
  repo_digests="$(docker image inspect --format '{{join .RepoDigests " "}}' "$image")"
  echo "image ${image}: id ${image_id}, repository digests: ${repo_digests:-none}"
  output image-id "$image_id"
  output image-digests "$repo_digests"

  # The default image must carry the engine this action's tree describes; a
  # caller who names an image takes that responsibility.
  if [[ -n "$version" ]]; then
    local reported
    reported="$(docker run --rm --entrypoint /usr/local/bin/veredictum "$image_id" --version)" \
      || refuse "the engine in ${image} did not report a version"
    [[ "$reported" == "veredictum ${version} ("* ]] \
      || refuse "the engine in ${image} reports '${reported}', not veredictum ${version}"
  fi

  mkdir -p "$out_dir"
  out_dir="$(cd "$out_dir" && pwd -P)"
  # A run that cannot start writes nothing, so a results document left by an
  # earlier run in the same directory would otherwise be exported as this one.
  rm -f "${out_dir}/results.json" "${out_dir}/recording.har" "${out_dir}/junit.xml" \
    "${out_dir}/ctrf.json" "${out_dir}/summary.md"
  output results "${out_dir}/results.json"

  # The image runs as uid 65532; running as the caller's uid instead lets the
  # run write into a directory the caller created, and lets the caller read the
  # files afterwards without a permission change.
  run_args=(--rm --network "$network" --user "$(id -u):$(id -g)" -v "${out_dir}:/work/out")

  if [[ -n "${IN_STATEMENT:-}" ]]; then
    [[ -f "$IN_STATEMENT" ]] || refuse "the statement ${IN_STATEMENT} is not a file"
    run_args+=(-v "$(absolute "$IN_STATEMENT"):/work/input/statement.json:ro")
    test_args+=(--statement /work/input/statement.json)
  fi
  if [[ -n "${IN_IXIT:-}" ]]; then
    [[ -f "$IN_IXIT" ]] || refuse "the IXIT ${IN_IXIT} is not a file"
    run_args+=(-v "$(absolute "$IN_IXIT"):/work/input/ixit.json:ro")
    test_args+=(--ixit /work/input/ixit.json)
  fi
  local document
  local -a documents=()
  read -r -a documents <<< "${IN_DOCUMENTS:-}"
  for document in ${documents[@]+"${documents[@]}"}; do
    test_args+=(--document "$document")
  done

  local name file_var
  for name in "${CREDENTIALS[@]}"; do
    if [[ -n "${!name:-}" ]]; then
      run_args+=(-e "$name")
    fi
    file_var="${name}_FILE"
    if [[ -n "${!file_var:-}" ]]; then
      [[ -f "${!file_var}" ]] || refuse "${file_var} names ${!file_var}, which is not a file"
      run_args+=(-v "$(absolute "${!file_var}"):/work/secrets/${name}:ro" -e "${file_var}=/work/secrets/${name}")
    fi
  done

  local code=0
  docker run "${run_args[@]}" "$image_id" test --base-url "$base_url" \
    --auth "$auth" --admin-auth "$admin_auth" ${test_args[@]+"${test_args[@]}"} --out /work/out || code=$?
  case "$code" in
    0 | 1 | 2) ;;
    # Any other status is the container runtime's own, never the engine's.
    *)
      echo "::error::the container exited ${code}, which is not an engine status" >&2
      code=2
      ;;
  esac

  if [[ ! -f "${out_dir}/results.json" ]]; then
    summary "Veredictum could not start the run (exit status ${code}); the step log says why."
    output exit-code "$code"
    exit "$code"
  fi

  local export_code=0
  docker run --rm --network none --user "$(id -u):$(id -g)" -v "${out_dir}:/work/out" \
    "$image_id" export --results /work/out/results.json --junit /work/out/junit.xml \
    --ctrf /work/out/ctrf.json --markdown /work/out/summary.md || export_code=$?
  if [[ "$export_code" -eq 0 ]]; then
    output junit "${out_dir}/junit.xml"
    output ctrf "${out_dir}/ctrf.json"
    output summary "${out_dir}/summary.md"
    if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
      cat "${out_dir}/summary.md" >> "$GITHUB_STEP_SUMMARY"
    fi
    summary ""
    summary "Image \`${image}\`, id \`${image_id}\`."
  else
    echo "::error::the export of ${out_dir}/results.json exited ${export_code}" >&2
    summary "Veredictum could not export the results (exit status ${export_code})."
    if [[ "$code" -eq 0 ]]; then code=2; fi
  fi

  output exit-code "$code"
  exit "$code"
}

# ── The self-test: the wiring, against a stub `docker` ────────────────────────
# A stub records every invocation and plays the engine: `test` writes a
# results document and exits with STUB_TEST_EXIT, `export` writes the three
# views. What it proves is the part only this script decides: credentials by
# name and never by value, an empty credential dropped, a `_FILE` credential
# mounted, the exit status passed through, and the summary appended.
self_test() {
  local dir fails=0
  dir="$(mktemp -d)"
  trap 'rm -rf "$dir"' RETURN
  mkdir -p "$dir/bin" "$dir/action/app/veredictum" "$dir/work"
  printf '[package]\nname = "veredictum"\nversion = "9.8.7"\n' > "$dir/action/app/veredictum/Cargo.toml"
  printf '{"claims":{"capabilities":["system"]}}\n' > "$dir/work/statement.json"
  printf 'from-a-file\n' > "$dir/work/token"
  cat > "$dir/bin/docker" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$STUB_LOG"
case "$1" in
  pull) exit 0 ;;
  image) printf 'sha256:stub\n'; exit 0 ;;
esac
for arg in "$@"; do
  case "$arg" in
    --version) printf 'veredictum %s (pre-approval: stub)\n' "$STUB_VERSION"; exit 0 ;;
  esac
done
out=""
prev=""
for arg in "$@"; do
  if [[ "$prev" == "-v" && "$arg" == *":/work/out" ]]; then out="${arg%:/work/out}"; fi
  prev="$arg"
done
for arg in "$@"; do
  case "$arg" in
    test)
      [[ "$STUB_TEST_EXIT" == 2 ]] && exit 2
      printf '{}\n' > "$out/results.json"
      exit "$STUB_TEST_EXIT" ;;
    export)
      printf '<testsuites/>\n' > "$out/junit.xml"
      printf '{}\n' > "$out/ctrf.json"
      printf '# Veredictum results: stub\n' > "$out/summary.md"
      exit 0 ;;
  esac
done
exit 125
STUB
  chmod +x "$dir/bin/docker"

  # check <label> <exit> <pattern that must match> <pattern the docker log must not match> [env…]
  check() {
    local label="$1" want="$2" must="$3" mustnt="$4" got=0
    shift 4
    : > "$dir/log"
    : > "$dir/summary"
    : > "$dir/output"
    (
      cd "$dir/work" || exit 99
      env -i PATH="$dir/bin:$PATH" STUB_LOG="$dir/log" STUB_VERSION=9.8.7 \
        GITHUB_ACTION_PATH="$dir/action" GITHUB_STEP_SUMMARY="$dir/summary" \
        GITHUB_OUTPUT="$dir/output" IN_BASE_URL=http://127.0.0.1:9/openehr/v1 \
        STUB_TEST_EXIT=0 "$@" bash "$SELF" >/dev/null 2>&1
    ) || got=$?
    if [[ "$got" != "$want" ]]; then
      echo "self-test FAILED: ${label}: exit ${got}, expected ${want}" >&2
      fails=$((fails + 1))
    elif [[ -n "$must" ]] && ! grep -qE -- "$must" "$dir/log" "$dir/summary" "$dir/output"; then
      echo "self-test FAILED: ${label}: nothing matched ${must}" >&2
      fails=$((fails + 1))
    elif [[ -n "$mustnt" ]] && grep -qE -- "$mustnt" "$dir/log"; then
      echo "self-test FAILED: ${label}: the docker command line matched ${mustnt}" >&2
      fails=$((fails + 1))
    elif ! grep -qx "exit-code=${want}" "$dir/output"; then
      echo "self-test FAILED: ${label}: no exit-code=${want} output" >&2
      fails=$((fails + 1))
    else
      echo "self-test ok: ${label}"
    fi
  }

  check "a passing run exits 0 and appends the summary" 0 '^# Veredictum results: stub$' ''
  check "the default image is the release image of the action's version" 0 \
    "pull --quiet ${RELEASE_IMAGE}:9\\.8\\.7" ''
  check "a failed row exits 1" 1 '^exit-code=1$' '' STUB_TEST_EXIT=1
  check "a run that cannot start exits 2" 2 'could not start' '' STUB_TEST_EXIT=2
  check "a token is passed by name, never by value" 0 '-e VEREDICTUM_SUT_TOKEN ' 's3cr3t' \
    VEREDICTUM_SUT_TOKEN=s3cr3t
  check "an empty credential is not passed at all" 0 '' 'VEREDICTUM_SUT_TOKEN' \
    VEREDICTUM_SUT_TOKEN=
  check "a _FILE credential is mounted and renamed" 0 \
    'VEREDICTUM_ADMIN_TOKEN_FILE=/work/secrets/VEREDICTUM_ADMIN_TOKEN' 'from-a-file' \
    VEREDICTUM_ADMIN_TOKEN_FILE=token
  check "a missing _FILE credential is refused" 2 'not a file' '' \
    VEREDICTUM_ADMIN_TOKEN_FILE=missing
  check "the statement is mounted read-only" 0 \
    'statement\.json:/work/input/statement\.json:ro .*--statement /work/input/statement\.json' '' \
    IN_STATEMENT=statement.json
  check "a named image is not version-checked" 0 'pull --quiet example\.org/x:1' '--version' \
    IN_IMAGE=example.org/x:1 STUB_VERSION=0.0.0
  check "the default image must carry the action's version" 2 'not veredictum 9\.8\.7' '' \
    STUB_VERSION=0.0.0
  check "an unknown pull value is refused" 2 'accepted: true, false' '' IN_PULL=yes
  check "pull false does not pull" 0 '' '^pull ' IN_PULL=false
  check "documents become repeated --document flags" 0 '--document ehr --document query' '' \
    'IN_DOCUMENTS=ehr query'

  if [[ "$fails" -gt 0 ]]; then
    echo "run.sh self-test: ${fails} case(s) failed" >&2
    return 1
  fi
  echo "run.sh self-test: every case passed"
}

SELF="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
readonly SELF

case "${1:-}" in
  --self-test) self_test ;;
  "") main ;;
  *) echo "usage: $0 [--self-test]" >&2; exit 2 ;;
esac
