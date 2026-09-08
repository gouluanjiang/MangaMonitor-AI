#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

state="$tmp/monitor-state"
mkdir -p "$state"

write_pristine_state() {
  rm -rf "$state"
  mkdir -p "$state"
  cat > "$state/authors.json" <<'JSON'
{"schema_version":1,"authors":[{"author_id":"A1","name":"One","enabled":true},{"author_id":"A2","name":"Two","enabled":true}]}
JSON
  cat > "$state/catalog.json" <<'JSON'
{"schema_version":1,"records":[]}
JSON
  cat > "$state/inventory_index.json" <<'JSON'
{"schema_version":1,"works":[{"work_id":"LOCAL_BASELINE"}]}
JSON
  cat > "$state/pending.json" <<'JSON'
{"schema_version":1,"tasks":[]}
JSON
  cat > "$state/review.json" <<'JSON'
{"schema_version":1,"match_review":[],"cleanup_review":[]}
JSON
  cat > "$state/decisions.json" <<'JSON'
{"schema_version":1,"positive_mappings":[],"negative_mappings":[],"ignored_source_records":[],"ignored_works":[]}
JSON
  cat > "$state/scan_state.json" <<'JSON'
{"schema_version":1,"initial_full_scan_complete":false,"current_scan_id":null,"author_source_progress":{},"failures":[],"last_complete_scan_at":null}
JSON
  cat > "$state/latest.json" <<'JSON'
{"schema_version":1,"scan_id":null,"scan_status":"NOT_STARTED","last_complete_scan_at":null,"events":[]}
JSON
  cat > "$state/scope-certificates.json" <<'JSON'
{"schema_version":1,"certificate_set_hash":"4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945","certificates":[]}
JSON
}

write_config() {
  local enabled="$1"
  local path="$2"
  local configured_state="${3:-$state}"
  cat > "$path" <<JSON
{"schema_version":1,"production_enabled":$enabled,"state_directory":"$configured_state","author_file":"$configured_state/authors.json","historical_id_threshold":5,"soft_batch_size":1,"max_requests_per_batch":20}
JSON
}

fake_runner="$tmp/fake-phase3b"
cat > "$fake_runner" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
state=''
output=''
authors=''
batch_size=''
batch_index=''
base=''
mode=''
continued=false
while (($#)); do
  case "$1" in
    --state) state="$2"; shift 2 ;;
    --output) output="$2"; shift 2 ;;
    --authors) authors="$2"; shift 2 ;;
    --batch-size) batch_size="$2"; shift 2 ;;
    --batch-index) batch_index="$2"; shift 2 ;;
    --expected-base-sha) base="$2"; shift 2 ;;
    --mode) mode="$2"; shift 2 ;;
    --continue-cycle) continued=true; shift ;;
    --threshold|--actual-base-sha|--max-requests|--author-concurrency|--repair-overlay) shift 2 ;;
    --live-source) shift ;;
    *) echo "FAKE_UNKNOWN_ARG:$1" >&2; exit 90 ;;
  esac
done
mkdir -p "$output"
cp -a "$state/." "$output/"
author_count="$(jq '[.authors[] | select(.enabled != false)] | length' "$authors")"
batch_count="$(( (author_count + batch_size - 1) / batch_size ))"
complete=true
strategy=true
if [[ "${FAKE_MODE:-}" == incomplete && "$batch_index" -eq $((batch_count - 1)) ]]; then
  complete=false
  strategy=false
fi
cat > "$output/checkpoint.json" <<JSON
{"schema_version":1,"batch_index":$batch_index}
JSON
cat > "$output/state-manifest.json" <<JSON
{"schema_version":1,"base_commit":"$base","requested_mode":"$mode","batch_index":$batch_index,"batch_count":$batch_count,"complete":$complete,"strategy_complete":$strategy}
JSON
cat > "$output/scan-report.json" <<JSON
{"complete":$complete,"strategy_complete":$strategy,"source_error_boundaries":0}
JSON
current_author="$(jq -r --argjson i "$batch_index" '[.authors[] | select(.enabled != false)] | .[$i].name' "$authors")"
previous_last_full='{}'
if jq -e '.phase3a_scan.last_full | type == "object"' "$output/scan_state.json" >/dev/null 2>&1; then
  previous_last_full="$(jq -c '.phase3a_scan.last_full' "$output/scan_state.json")"
fi
last_full="$(jq -c --arg author "$current_author" '. + {("jm|" + $author):"fixed", ("pica|" + $author):"fixed"}' <<< "$previous_last_full")"
if [[ "${FAKE_MODE:-}" == coverage-drift && "$batch_index" -eq $((batch_count - 1)) ]]; then
  last_full="$(jq -c --arg key "pica|$current_author" 'del(.[$key])' <<< "$last_full")"
fi
jq -n \
  --arg scan_id "BOOTSTRAP-$batch_index" \
  --argjson complete "$complete" \
  --argjson last_full "$last_full" '
  {
    schema_version:3,
    phase3a_scan:{
      scan_id:$scan_id,
      requested_mode:"full",
      complete:$complete,
      direct_failures:{},
      last_full:$last_full
    }
  }
' > "$output/scan_state.json"
jq -n \
  --arg scan_id "BOOTSTRAP-$batch_index" \
  --arg status "$(if [[ "$complete" == true ]]; then echo COMPLETE; else echo PARTIAL; fi)" '
  {schema_version:3,scan_id:$scan_id,scan_status:$status,events:[]}
' > "$output/latest.json"
if [[ "${FAKE_MODE:-}" == inventory-drift && "$batch_index" -eq $((batch_count - 1)) ]]; then
  cat > "$output/inventory_index.json" <<'JSON'
{"schema_version":1,"works":[{"work_id":"UNAUTHORIZED_DRIFT"}]}
JSON
fi
printf '%s|%s|%s|%s\n' "$batch_index" "$state" "$output" "$continued" >> "$FAKE_RUN_LOG"
SH
chmod +x "$fake_runner"

fake_commit="$tmp/fake-commit-monitor-state"
cat > "$fake_commit" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$FAKE_COMMIT_LOG"
SH
chmod +x "$fake_commit"

run_bootstrap() {
  local config="$1"
  local reports="$2"
  shift 2
  PHASE3B_BIN="$fake_runner" \
  COMMIT_MONITOR_STATE="$fake_commit" \
  FAKE_RUN_LOG="$tmp/run.log" \
  FAKE_COMMIT_LOG="$tmp/commit.log" \
  "$@" \
  bash "$repo_root/scripts/run-phase3b-bootstrap.sh" "$config" "$reports"
}

expect_failure() {
  local expected="$1"
  shift
  set +e
  output="$($@ 2>&1)"
  status=$?
  set -e
  test "$status" -ne 0
  grep -q "$expected" <<< "$output"
}

write_pristine_state
config="$tmp/config.json"
write_config false "$config"
: > "$tmp/run.log"
: > "$tmp/commit.log"
run_bootstrap "$config" "$tmp/reports-success"
test "$(wc -l < "$tmp/run.log")" -eq 2
test "$(wc -l < "$tmp/commit.log")" -eq 1
first="$(sed -n '1p' "$tmp/run.log")"
second="$(sed -n '2p' "$tmp/run.log")"
grep -Fq "0|$state|$tmp/reports-success/batch-0|false" <<< "$first"
grep -Fq "1|$tmp/reports-success/batch-0|$tmp/reports-success/batch-1|true" <<< "$second"
# The fake committer intentionally does nothing: bootstrap accumulation itself must not mutate durable state.
jq -e '.scan_status == "NOT_STARTED"' "$state/latest.json" >/dev/null

write_pristine_state
write_config true "$config"
: > "$tmp/run.log"
: > "$tmp/commit.log"
expect_failure 'BOOTSTRAP_REQUIRES_PRODUCTION_DISABLED' run_bootstrap "$config" "$tmp/reports-production"
test ! -s "$tmp/run.log"
test ! -s "$tmp/commit.log"

write_pristine_state
write_config false "$config" "$tmp/not-monitor-state"
: > "$tmp/run.log"
: > "$tmp/commit.log"
expect_failure 'BOOTSTRAP_STATE_TARGET_NOT_ALLOWED' run_bootstrap "$config" "$tmp/reports-invalid-target"
test ! -s "$tmp/run.log"
test ! -s "$tmp/commit.log"

write_pristine_state
write_config false "$config"
: > "$tmp/run.log"
: > "$tmp/commit.log"
expect_failure 'BOOTSTRAP_REPORT_ROOT_INSIDE_STATE' run_bootstrap "$config" "$state/bootstrap-reports"
test ! -e "$state/bootstrap-reports"
test ! -s "$tmp/run.log"
test ! -s "$tmp/commit.log"

write_pristine_state
jq '.authors[0].author_id="AUTHOR_TEST_0001"' "$state/authors.json" > "$tmp/authors" && mv "$tmp/authors" "$state/authors.json"
write_config false "$config"
: > "$tmp/run.log"
: > "$tmp/commit.log"
expect_failure 'BOOTSTRAP_TEST_AUTHOR_ENABLED' run_bootstrap "$config" "$tmp/reports-test-author"
test ! -s "$tmp/run.log"
test ! -s "$tmp/commit.log"

write_pristine_state
write_config false "$config"
jq '.scan_status="COMPLETE"' "$state/latest.json" > "$tmp/latest" && mv "$tmp/latest" "$state/latest.json"
: > "$tmp/run.log"
: > "$tmp/commit.log"
expect_failure 'BOOTSTRAP_STATE_NOT_PRISTINE' run_bootstrap "$config" "$tmp/reports-nonpristine"
test ! -s "$tmp/run.log"
test ! -s "$tmp/commit.log"

write_pristine_state
write_config false "$config"
: > "$tmp/run.log"
: > "$tmp/commit.log"
expect_failure 'BOOTSTRAP_BATCH_INCOMPLETE' run_bootstrap "$config" "$tmp/reports-incomplete" env FAKE_MODE=incomplete
test ! -s "$tmp/commit.log"

write_pristine_state
write_config false "$config"
: > "$tmp/run.log"
: > "$tmp/commit.log"
expect_failure 'BOOTSTRAP_INVENTORY_DRIFT' run_bootstrap "$config" "$tmp/reports-drift" env FAKE_MODE=inventory-drift
test ! -s "$tmp/commit.log"

write_pristine_state
write_config false "$config"
: > "$tmp/run.log"
: > "$tmp/commit.log"
expect_failure 'BOOTSTRAP_FULL_COVERAGE_MISMATCH' run_bootstrap "$config" "$tmp/reports-coverage" env FAKE_MODE=coverage-drift
test ! -s "$tmp/commit.log"

write_pristine_state
write_config false "$config"
touch "$state/state-manifest.json"
: > "$tmp/run.log"
: > "$tmp/commit.log"
expect_failure 'BOOTSTRAP_STATE_NOT_PRISTINE' run_bootstrap "$config" "$tmp/reports-manifest"
test ! -s "$tmp/run.log"
test ! -s "$tmp/commit.log"

echo 'PHASE3B_BOOTSTRAP_REGRESSIONS_OK'
