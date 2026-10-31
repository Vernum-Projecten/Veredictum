#!/usr/bin/env bash
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
# The GitHub Action's one step (`action.yml` at the repository root, #770).
#
# It runs the instrument's container image twice: `test` drives the shipped
# Arazzo catalogue against the server under test and writes `results.json` and
# the HAR recording into the output directory, then `export` writes the JUnit
# XML, CTRF and Markdown views beside them, and `badge` writes the badges into
# `badges/` under it. The Markdown summary is appended to the job summary, and
# each failed, errored or warning row is printed as a workflow-command
# annotation, at most GitHub's 10 of each kind per step. The step exits with
# the run's own status: 0 when no row counts against the server and none is
# inconclusive, 1 when a row counts against the server, 3 when none does but
# a row is inconclusive (the instrument could not judge it), 2 when the run
# could not start or finish. With IN_FAIL_ON_INCONCLUSIVE=false a 3 passes
# the step, the `exit-code` output still reads 3, and the job summary lists
# the inconclusive rows.
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
#   IN_PROFILE      development | released, checked by the engine
#   IN_PLATFORM_URL the SMART Platform base URL (optional)
#   IN_OUT_DIR      the output directory, created when missing
#   IN_IMAGE        the image reference; empty means the release image of the
#                   version this action's own tree carries
#   IN_PULL         true | false: pull the image before resolving it
#   IN_NETWORK      the container network the run joins
#   IN_BADGE_BRANCH the branch of the calling repository the badges are pushed
#                   to (optional; empty pushes nothing)
#   IN_FAIL_ON_INCONCLUSIVE  true | false: whether exit status 3 fails the step
#   IN_ANNOTATIONS  true | false: whether the rows are annotated
#
# The badge push uses GITHUB_TOKEN from the step's env, by name, and happens
# only on a run that is not a pull request (GITHUB_EVENT_NAME) and whose
# GITHUB_REF is the repository's default branch, read from the event payload
# at GITHUB_EVENT_PATH. The branch holds one parentless commit with the badges
# and nothing else, replaced on every push, and no commit is made when the
# badges did not change.
#
# Credentials are read from the variables the engine reads
# (VEREDICTUM_CDR_USER, VEREDICTUM_CDR_PASSWORD, VEREDICTUM_CDR_TOKEN and
# their VEREDICTUM_CDR_ADMIN_ counterparts), or from files named by the same
# variables with `_FILE` appended. A credential is handed to the container by
# NAME, so its value never appears on a command line, and an EMPTY variable is
# not handed over at all: a secret that does not exist in the caller's
# repository expands to an empty string, and the run must then stop on a
# missing credential rather than present an empty one.
#
# The SMART scoped-token variables travel the same way: every variable named
# VEREDICTUM_SCOPED_TOKEN_<NAME>, which covers the test issuer's key
# (VEREDICTUM_SCOPED_TOKEN_KEY) and each token an IXIT's `issued` member names,
# and each of them with `_FILE` appended.
#
# Usage:
#   scripts/action/run.sh               # as the action runs it
#   scripts/action/run.sh --self-test   # against a stub `docker`, no runtime
set -euo pipefail

readonly CREDENTIALS=(
  VEREDICTUM_CDR_USER
  VEREDICTUM_CDR_PASSWORD
  VEREDICTUM_CDR_TOKEN
  VEREDICTUM_CDR_ADMIN_USER
  VEREDICTUM_CDR_ADMIN_PASSWORD
  VEREDICTUM_CDR_ADMIN_TOKEN
)
readonly SCOPED_TOKEN_PREFIX=VEREDICTUM_SCOPED_TOKEN_
readonly RELEASE_IMAGE=ghcr.io/vernum-projecten/veredictum
readonly BADGES=(base options profile instrument)
readonly BOT_NAME='github-actions[bot]'
readonly BOT_EMAIL='41898282+github-actions[bot]@users.noreply.github.com'

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

# badge_git <args…>: git without the runner's global or system configuration,
# so no signing key, hook or credential helper of the runner applies.
badge_git() {
  GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git "$@"
}

# publish_badges <dir> <branch>: push the badges in <dir> to <branch> of the
# calling repository when this run may, and print why when it may not. Returns
# non-zero only when a push was due and failed.
publish_badges() {
  local dir="$1" branch="$2" event="${GITHUB_EVENT_NAME:-}" ref="${GITHUB_REF:-}"
  local default="" server="${GITHUB_SERVER_URL:-https://github.com}" remote work
  case "$event" in
    pull_request | pull_request_target)
      echo "badges not pushed: a ${event} run never updates the badge branch"
      return 0
      ;;
  esac
  if [[ -n "${GITHUB_EVENT_PATH:-}" && -f "$GITHUB_EVENT_PATH" ]]; then
    default="$(jq -r '.repository.default_branch // empty' "$GITHUB_EVENT_PATH")" || default=""
  fi
  if [[ -z "$default" ]]; then
    echo "badges not pushed: the event payload names no default branch"
    return 0
  fi
  if [[ "$ref" != "refs/heads/${default}" ]]; then
    echo "badges not pushed: ${ref:-no ref} is not the default branch refs/heads/${default}"
    return 0
  fi
  if [[ -z "${GITHUB_TOKEN:-}" ]]; then
    echo "::error::badge-branch is set, but GITHUB_TOKEN is not in the step's env" >&2
    return 1
  fi
  if [[ -z "${GITHUB_REPOSITORY:-}" ]]; then
    echo "::error::GITHUB_REPOSITORY is not set" >&2
    return 1
  fi
  remote="${server}/${GITHUB_REPOSITORY}.git"

  local status=0 header
  work="$(mktemp -d)"
  # The token reaches git through its environment, never a command line.
  header="AUTHORIZATION: basic $(printf 'x-access-token:%s' "$GITHUB_TOKEN" | base64 | tr -d '\n')"
  export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0="http.${server}/.extraheader" GIT_CONFIG_VALUE_0="$header"
  push_badges "$dir" "$branch" "$remote" "$work" || status=$?
  unset GIT_CONFIG_COUNT GIT_CONFIG_KEY_0 GIT_CONFIG_VALUE_0
  rm -rf -- "$work"
  return "$status"
}

# push_badges <dir> <branch> <remote> <work>: commit the badges in <dir> as the
# one commit of <branch> at <remote>, using the scratch repository <work>,
# unless that branch already holds exactly these badges.
push_badges() {
  local dir="$1" branch="$2" remote="$3" work="$4" old tree commit name
  badge_git init --quiet "$work"
  for name in "${BADGES[@]}"; do
    cp "${dir}/${name}.svg" "${dir}/${name}.json" "$work/"
  done
  badge_git -C "$work" add --all
  tree="$(badge_git -C "$work" write-tree)"
  if ! old="$(badge_git ls-remote "$remote" "refs/heads/${branch}")"; then
    echo "::error::could not read ${branch} of ${GITHUB_REPOSITORY}" >&2
    return 1
  fi
  old="${old%%[[:space:]]*}"
  if [[ -n "$old" ]]; then
    if ! badge_git -C "$work" fetch --quiet --depth 1 "$remote" "refs/heads/${branch}"; then
      echo "::error::could not fetch ${branch} of ${GITHUB_REPOSITORY}" >&2
      return 1
    fi
    old="$(badge_git -C "$work" rev-parse FETCH_HEAD)"
    if [[ "$(badge_git -C "$work" rev-parse "FETCH_HEAD^{tree}")" == "$tree" ]]; then
      echo "badges unchanged: nothing pushed to ${branch}"
      return 0
    fi
  fi
  commit="$(GIT_AUTHOR_NAME="$BOT_NAME" GIT_AUTHOR_EMAIL="$BOT_EMAIL" \
    GIT_COMMITTER_NAME="$BOT_NAME" GIT_COMMITTER_EMAIL="$BOT_EMAIL" \
    badge_git -C "$work" commit-tree "$tree" -m "Veredictum badges for ${GITHUB_SHA:-an unknown commit}")"
  if ! badge_git -C "$work" push --quiet --force-with-lease="refs/heads/${branch}:${old}" \
    "$remote" "${commit}:refs/heads/${branch}"; then
    echo "::error::could not push the badges to ${branch} of ${GITHUB_REPOSITORY}" >&2
    return 1
  fi
  echo "badges pushed to ${branch} as ${commit}"
}

main() {
  local base_url="${IN_BASE_URL:-}" out_dir="${IN_OUT_DIR:-veredictum-results}"
  local image="${IN_IMAGE:-}" pull="${IN_PULL:-true}" network="${IN_NETWORK:-host}"
  local auth="${IN_AUTH:-none}" admin_auth="${IN_ADMIN_AUTH:-none}"
  local profile="${IN_PROFILE:-development}"
  local fail_on_inconclusive="${IN_FAIL_ON_INCONCLUSIVE:-true}" annotations="${IN_ANNOTATIONS:-true}"
  local version="" image_id repo_digests
  local -a run_args=() test_args=()

  [[ -n "$base_url" ]] || refuse "the base-url input is required"
  case "$pull" in
    true | false) ;;
    *) refuse "the pull input is \"${pull}\" (accepted: true, false)" ;;
  esac
  case "$fail_on_inconclusive" in
    true | false) ;;
    *) refuse "the fail-on-inconclusive input is \"${fail_on_inconclusive}\" (accepted: true, false)" ;;
  esac
  case "$annotations" in
    true | false) ;;
    *) refuse "the annotations input is \"${annotations}\" (accepted: true, false)" ;;
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
  local badge
  for badge in "${BADGES[@]}"; do
    rm -f "${out_dir}/badges/${badge}.svg" "${out_dir}/badges/${badge}.json"
  done
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
  if [[ -n "${IN_PLATFORM_URL:-}" ]]; then
    test_args+=(--platform-url "$IN_PLATFORM_URL")
  fi

  # The scoped-token names are whatever the caller set under the prefix, each
  # reduced to its base name so a `_FILE` variable and its value variable are
  # one secret.
  local name file_var
  local -a secrets=("${CREDENTIALS[@]}")
  while IFS= read -r name; do
    [[ -n "$name" ]] && secrets+=("$name")
  done < <(compgen -e | grep -E "^${SCOPED_TOKEN_PREFIX}[A-Z0-9_]+\$" | sed -E 's/_FILE$//' | LC_ALL=C sort -u)
  for name in "${secrets[@]}"; do
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
    --auth "$auth" --admin-auth "$admin_auth" --profile "$profile" \
    ${test_args[@]+"${test_args[@]}"} --out /work/out || code=$?
  case "$code" in
    0 | 1 | 2 | 3) ;;
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
    --ctrf /work/out/ctrf.json --markdown /work/out/summary.md \
    --annotations /work/out/annotations.txt || export_code=$?
  if [[ "$export_code" -eq 0 ]]; then
    output junit "${out_dir}/junit.xml"
    output ctrf "${out_dir}/ctrf.json"
    output summary "${out_dir}/summary.md"
    if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
      cat "${out_dir}/summary.md" >> "$GITHUB_STEP_SUMMARY"
    fi
    summary ""
    summary "Image \`${image}\`, id \`${image_id}\`."
    # A workflow command is read from the step's standard output.
    if [[ "$annotations" == true ]]; then
      cat "${out_dir}/annotations.txt"
    fi
  else
    echo "::error::the export of ${out_dir}/results.json exited ${export_code}" >&2
    summary "Veredictum could not export the results (exit status ${export_code})."
    if [[ "$code" -eq 0 || "$code" -eq 3 ]]; then code=2; fi
  fi
  rm -f "${out_dir}/annotations.txt"

  local badge_code=0
  docker run --rm --network none --user "$(id -u):$(id -g)" -v "${out_dir}:/work/out" \
    "$image_id" badge --results /work/out/results.json --out /work/out/badges || badge_code=$?
  if [[ "$badge_code" -eq 0 ]]; then
    output badges "${out_dir}/badges"
    if [[ -z "${IN_BADGE_BRANCH:-}" ]]; then
      echo "badges not pushed: the badge-branch input is empty"
    elif ! publish_badges "${out_dir}/badges" "$IN_BADGE_BRANCH"; then
      summary "Veredictum could not push the badges to \`${IN_BADGE_BRANCH}\`; the step log says why."
      if [[ "$code" -eq 0 || "$code" -eq 3 ]]; then code=2; fi
    fi
  else
    echo "::error::the badges of ${out_dir}/results.json exited ${badge_code}" >&2
    summary "Veredictum could not draw the badges (exit status ${badge_code})."
    if [[ "$code" -eq 0 || "$code" -eq 3 ]]; then code=2; fi
  fi

  output exit-code "$code"
  if [[ "$code" -eq 3 && "$fail_on_inconclusive" == false ]]; then
    summary ""
    summary "Exit status 3 passes this step (fail-on-inconclusive: false). The inconclusive rows, which the instrument could not judge and which do not count against the server:"
    summary ""
    jq -r '.rows[] | select((.status == "failed" or .status == "errored") and .explanation.whose == "instrument")
      | "- `\(.case)` row \(.row): \(.explanation.received)"' "${out_dir}/results.json" \
      | while IFS= read -r line; do summary "$line"; done
    echo "exit status 3 (inconclusive rows only) passes the step: fail-on-inconclusive is false"
    exit 0
  fi
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
      if [[ "$STUB_TEST_EXIT" == 3 ]]; then
        printf '{"rows":[{"case":"stub/unjudged","row":0,"status":"errored","explanation":{"whose":"instrument","received":"not judged: stub"}},{"case":"stub/fine","row":0,"status":"passed"}]}\n' > "$out/results.json"
      else
        printf '{}\n' > "$out/results.json"
      fi
      exit "$STUB_TEST_EXIT" ;;
    export)
      printf '<testsuites/>\n' > "$out/junit.xml"
      printf '{}\n' > "$out/ctrf.json"
      printf '# Veredictum results: stub\n' > "$out/summary.md"
      printf '::error title=stub/case row 0::stub annotation\n' > "$out/annotations.txt"
      exit 0 ;;
    badge)
      mkdir -p "$out/badges"
      for name in base options profile instrument; do
        printf '<svg>%s %s</svg>\n' "$name" "${STUB_BADGE:-first}" > "$out/badges/$name.svg"
        printf '{"schemaVersion":1,"label":"%s"}\n' "$name" > "$out/badges/$name.json"
      done
      exit 0 ;;
  esac
done
exit 125
STUB
  chmod +x "$dir/bin/docker"

  # check <label> <exit> <pattern that must match> <pattern the docker log must not match> [env…]
  # <exit> is the step's exit status, or <step>/<exit-code output> when the two differ.
  # The must-match pattern is searched in the docker log, the job summary, the
  # step outputs and the step's own standard output.
  check() {
    local label="$1" want="${2%%/*}" want_out="${2##*/}" must="$3" mustnt="$4" got=0
    shift 4
    : > "$dir/log"
    : > "$dir/summary"
    : > "$dir/output"
    (
      cd "$dir/work" || exit 99
      env -i PATH="$dir/bin:$PATH" STUB_LOG="$dir/log" STUB_VERSION=9.8.7 \
        GITHUB_ACTION_PATH="$dir/action" GITHUB_STEP_SUMMARY="$dir/summary" \
        GITHUB_OUTPUT="$dir/output" IN_BASE_URL=http://127.0.0.1:9/openehr/v1 \
        STUB_TEST_EXIT=0 "$@" bash "$SELF" >"$dir/stdout" 2>/dev/null
    ) || got=$?
    if [[ "$got" != "$want" ]]; then
      echo "self-test FAILED: ${label}: exit ${got}, expected ${want}" >&2
      fails=$((fails + 1))
    elif [[ -n "$must" ]] && ! grep -qE -- "$must" "$dir/log" "$dir/summary" "$dir/output" "$dir/stdout"; then
      echo "self-test FAILED: ${label}: nothing matched ${must}" >&2
      fails=$((fails + 1))
    elif [[ -n "$mustnt" ]] && grep -qE -- "$mustnt" "$dir/log" "$dir/stdout" "$dir/summary"; then
      echo "self-test FAILED: ${label}: the docker command line or the step output matched ${mustnt}" >&2
      fails=$((fails + 1))
    elif ! grep -qx "exit-code=${want_out}" "$dir/output"; then
      echo "self-test FAILED: ${label}: no exit-code=${want_out} output" >&2
      fails=$((fails + 1))
    else
      echo "self-test ok: ${label}"
    fi
  }

  check "a passing run exits 0 and appends the summary" 0 '^# Veredictum results: stub$' ''
  check "the default image is the release image of the action's version" 0 \
    "pull --quiet ${RELEASE_IMAGE}:9\\.8\\.7" ''
  check "a failed row exits 1" 1 '^exit-code=1$' '' STUB_TEST_EXIT=1
  check "a failed row fails the step whatever fail-on-inconclusive says" 1 '^exit-code=1$' '' \
    STUB_TEST_EXIT=1 IN_FAIL_ON_INCONCLUSIVE=false
  check "an inconclusive run exits 3 and fails the step by default" 3 '^exit-code=3$' '' \
    STUB_TEST_EXIT=3
  check "fail-on-inconclusive false passes exit 3 and lists the inconclusive rows" 0/3 \
    '^- `stub/unjudged` row 0: not judged: stub$' '' \
    STUB_TEST_EXIT=3 IN_FAIL_ON_INCONCLUSIVE=false
  check "fail-on-inconclusive false lists no conclusive row" 0/3 '' 'stub/fine' \
    STUB_TEST_EXIT=3 IN_FAIL_ON_INCONCLUSIVE=false
  check "a run that cannot start fails the step whatever fail-on-inconclusive says" 2 \
    'could not start' '' STUB_TEST_EXIT=2 IN_FAIL_ON_INCONCLUSIVE=false
  check "an unknown fail-on-inconclusive value is refused" 2 'accepted: true, false' '' \
    IN_FAIL_ON_INCONCLUSIVE=no
  check "the rows are annotated on the step's output" 0 \
    '^::error title=stub/case row 0::stub annotation$' ''
  check "the export writes the annotations" 0 '--annotations /work/out/annotations\.txt' ''
  check "annotations false prints none" 0 '' '^::error title=stub' IN_ANNOTATIONS=false
  check "an unknown annotations value is refused" 2 'accepted: true, false' '' IN_ANNOTATIONS=maybe
  if [[ -e "$dir/work/veredictum-results/annotations.txt" ]]; then
    echo "self-test FAILED: the annotations file was left in the output directory" >&2
    fails=$((fails + 1))
  else
    echo "self-test ok: the annotations file is not left in the output directory"
  fi
  check "a run that cannot start exits 2" 2 'could not start' '' STUB_TEST_EXIT=2
  check "a token is passed by name, never by value" 0 '-e VEREDICTUM_CDR_TOKEN ' 's3cr3t' \
    VEREDICTUM_CDR_TOKEN=s3cr3t
  check "an empty credential is not passed at all" 0 '' 'VEREDICTUM_CDR_TOKEN' \
    VEREDICTUM_CDR_TOKEN=
  check "a _FILE credential is mounted and renamed" 0 \
    'VEREDICTUM_CDR_ADMIN_TOKEN_FILE=/work/secrets/VEREDICTUM_CDR_ADMIN_TOKEN' 'from-a-file' \
    VEREDICTUM_CDR_ADMIN_TOKEN_FILE=token
  check "a missing _FILE credential is refused" 2 'not a file' '' \
    VEREDICTUM_CDR_ADMIN_TOKEN_FILE=missing
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
  check "no profile input runs the development profile" 0 '--profile development ' ''
  check "the profile input reaches the engine" 0 '--profile released ' '' IN_PROFILE=released
  check "no platform-url input passes no --platform-url" 0 '' '--platform-url'
  check "the platform-url input reaches the engine" 0 \
    '--platform-url https://platform\.example --out' '' IN_PLATFORM_URL=https://platform.example
  check "the scoped-token key is passed by name, never by value" 0 \
    '-e VEREDICTUM_SCOPED_TOKEN_KEY ' 'PRIVATE KEY' \
    'VEREDICTUM_SCOPED_TOKEN_KEY=-----BEGIN PRIVATE KEY-----'
  check "a scoped-token key _FILE is mounted and renamed" 0 \
    'VEREDICTUM_SCOPED_TOKEN_KEY_FILE=/work/secrets/VEREDICTUM_SCOPED_TOKEN_KEY' 'from-a-file' \
    VEREDICTUM_SCOPED_TOKEN_KEY_FILE=token
  check "a missing scoped-token key _FILE is refused" 2 'not a file' '' \
    VEREDICTUM_SCOPED_TOKEN_KEY_FILE=missing
  check "an empty scoped-token key is not passed at all" 0 '' 'VEREDICTUM_SCOPED_TOKEN_KEY' \
    VEREDICTUM_SCOPED_TOKEN_KEY=
  check "an issued scoped token is passed by name" 0 \
    '-e VEREDICTUM_SCOPED_TOKEN_TEMPLATE_READ ' 'issued-s3cr3t' \
    VEREDICTUM_SCOPED_TOKEN_TEMPLATE_READ=issued-s3cr3t
  check "a variable outside the credential names is not passed" 0 '' 'UNRELATED_SECRET' \
    UNRELATED_SECRET=x
  check "the badges are drawn into the output directory" 0 \
    '^badges=.*/veredictum-results/badges$' ''
  if [[ -f "$dir/work/veredictum-results/badges/base.svg" \
    && -f "$dir/work/veredictum-results/badges/instrument.json" ]]; then
    echo "self-test ok: the badge files sit beside the views"
  else
    echo "self-test FAILED: the badge files are not in the output directory" >&2
    fails=$((fails + 1))
  fi

  # The badge push, against a local bare repository standing in for GitHub.
  local remote="$dir/remote/owner/repo.git"
  printf '{"repository":{"default_branch":"main"}}\n' > "$dir/event.json"
  fresh_remote() {
    rm -rf -- "$dir/remote"
    git init --quiet --bare "$remote"
  }
  # branch_head: the commit the badge branch points at, or nothing.
  branch_head() {
    git --git-dir="$remote" rev-parse --verify --quiet refs/heads/badges || true
  }
  # push_case <label> <exit> <pattern the step log must match> <commits on the branch> [env…]
  push_case() {
    local label="$1" want="$2" must="$3" commits="$4" got=0 count=0
    shift 4
    : > "$dir/output"
    (
      cd "$dir/work" || exit 99
      env -i PATH="$dir/bin:$PATH" STUB_LOG="$dir/log" STUB_VERSION=9.8.7 \
        GITHUB_ACTION_PATH="$dir/action" GITHUB_STEP_SUMMARY="$dir/summary" \
        GITHUB_OUTPUT="$dir/output" IN_BASE_URL=http://127.0.0.1:9/openehr/v1 \
        STUB_TEST_EXIT=0 IN_BADGE_BRANCH=badges GITHUB_EVENT_NAME=push \
        GITHUB_REF=refs/heads/main GITHUB_EVENT_PATH="$dir/event.json" \
        GITHUB_SERVER_URL="file://$dir/remote" GITHUB_REPOSITORY=owner/repo \
        GITHUB_TOKEN=t0k3n GITHUB_SHA=abc123 "$@" bash "$SELF" > "$dir/step.log" 2>&1
    ) || got=$?
    if [[ -n "$(branch_head)" ]]; then
      count="$(git --git-dir="$remote" rev-list --count refs/heads/badges)"
    fi
    if [[ "$got" != "$want" ]]; then
      echo "self-test FAILED: ${label}: exit ${got}, expected ${want}" >&2
      fails=$((fails + 1))
    elif ! grep -qE -- "$must" "$dir/step.log"; then
      echo "self-test FAILED: ${label}: the step log did not match ${must}" >&2
      fails=$((fails + 1))
    elif [[ "$count" != "$commits" ]]; then
      echo "self-test FAILED: ${label}: ${count} commit(s) on the badge branch, expected ${commits}" >&2
      fails=$((fails + 1))
    else
      echo "self-test ok: ${label}"
    fi
  }

  fresh_remote
  push_case "no badge-branch input pushes nothing" 0 'badge-branch input is empty' 0 \
    IN_BADGE_BRANCH=
  push_case "a pull request never pushes" 0 'a pull_request run never updates' 0 \
    GITHUB_EVENT_NAME=pull_request
  push_case "a pull_request_target run never pushes" 0 'a pull_request_target run never' 0 \
    GITHUB_EVENT_NAME=pull_request_target
  push_case "a branch other than the default never pushes" 0 \
    'refs/heads/feature is not the default branch refs/heads/main' 0 GITHUB_REF=refs/heads/feature
  push_case "a missing token fails the step and pushes nothing" 2 'GITHUB_TOKEN is not in' 0 \
    GITHUB_TOKEN=
  push_case "the default branch creates the badge branch" 0 '^badges pushed to badges as ' 1
  local first
  first="$(branch_head)"
  if [[ "$(git --git-dir="$remote" log -1 --format='%an <%ae>|%cn <%ce>' refs/heads/badges)" \
    == "${BOT_NAME} <${BOT_EMAIL}>|${BOT_NAME} <${BOT_EMAIL}>" ]] \
    && [[ "$(git --git-dir="$remote" ls-tree --name-only refs/heads/badges | LC_ALL=C sort | tr '\n' ' ')" \
      == "base.json base.svg instrument.json instrument.svg options.json options.svg profile.json profile.svg " ]]; then
    echo "self-test ok: the badge commit is the bot's and holds only the badges"
  else
    echo "self-test FAILED: the badge commit's author or tree is not as expected" >&2
    fails=$((fails + 1))
  fi
  if grep -q 't0k3n' "$dir/log" "$dir/step.log"; then
    echo "self-test FAILED: the token appeared in a command line or the step log" >&2
    fails=$((fails + 1))
  fi
  push_case "unchanged badges make no commit" 0 '^badges unchanged: nothing pushed' 1
  if [[ "$(branch_head)" != "$first" ]]; then
    echo "self-test FAILED: unchanged badges moved the badge branch" >&2
    fails=$((fails + 1))
  fi
  push_case "changed badges replace the one commit" 0 '^badges pushed to badges as ' 1 \
    STUB_BADGE=second
  if [[ "$(branch_head)" == "$first" ]]; then
    echo "self-test FAILED: changed badges did not move the badge branch" >&2
    fails=$((fails + 1))
  fi

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
