#!/usr/bin/env bash
set -euo pipefail

config_file="${1:-monitor-config.json}"
report_root="${2:-reports/phase3b-bootstrap}"
phase3b_bin="${PHASE3B_BIN:-./target/release/phase3b}"
commit_monitor_state="${COMMIT_MONITOR_STATE:-scripts/commit-monitor-state.sh}"
repair_overlay="${REPAIR_OVERLAY:-fixtures/matcher-m2/inventory-primary-repair.json}"

fail() {
  echo "$1" >&2
  exit "${2:-64}"
}

json_equal() {
  cmp -s <(jq -S -c . "$1") <(jq -S -c . "$2")
}

[[ -f "$config_file" ]] || fail 'BOOTSTRAP_CONFIG_MISSING'
[[ "$(jq -r '.production_enabled' "$config_file")" == "false" ]] || fail 'BOOTSTRAP_REQUIRES_PRODUCTION_DISABLED'

state_dir="$(jq -r '.state_directory // empty' "$config_file")"
authors_file="$(jq -r '.author_file // empty' "$config_file")"
batch_size="$(jq -r '.soft_batch_size // empty' "$config_file")"
max_requests="$(jq -r '.max_requests_per_batch // empty' "$config_file")"
threshold="$(jq -r '.historical_id_threshold // empty' "$config_file")"
author_concurrency=3

case "$state_dir" in
  monitor-state|*/monitor-state) ;;
  *) fail 'BOOTSTRAP_STATE_TARGET_NOT_ALLOWED' ;;
esac
[[ -n "$state_dir" && -d "$state_dir" ]] || fail 'BOOTSTRAP_STATE_DIR_MISSING'
[[ "$authors_file" == "$state_dir/authors.json" && -f "$authors_file" ]] || fail 'BOOTSTRAP_AUTHOR_FILE_MISMATCH'
state_abs="$(realpath -e "$state_dir")"
report_abs="$(realpath -m "$report_root")"
case "$report_abs" in
  "$state_abs"|"$state_abs"/*) fail 'BOOTSTRAP_REPORT_ROOT_INSIDE_STATE' ;;
esac
for name in authors.json catalog.json inventory_index.json pending.json review.json decisions.json scan_state.json latest.json; do
  [[ -f "$state_dir/$name" ]] || fail "BOOTSTRAP_STATE_FILE_MISSING:$name"
done
[[ ! -e "$state_dir/state-manifest.json" && ! -e "$state_dir/checkpoint.json" ]] || fail 'BOOTSTRAP_STATE_NOT_PRISTINE'

jq -e '
  .scan_status == "NOT_STARTED" and
  .scan_id == null and
  .last_complete_scan_at == null and
  ((.events // []) | length) == 0
' "$state_dir/latest.json" >/dev/null || fail 'BOOTSTRAP_STATE_NOT_PRISTINE'
jq -e '
  .initial_full_scan_complete == false and
  .current_scan_id == null and
  ((.author_source_progress // {}) | length) == 0 and
  ((.failures // []) | length) == 0 and
  .last_complete_scan_at == null
' "$state_dir/scan_state.json" >/dev/null || fail 'BOOTSTRAP_STATE_NOT_PRISTINE'
jq -e '((.records // []) | length) == 0' "$state_dir/catalog.json" >/dev/null || fail 'BOOTSTRAP_STATE_NOT_PRISTINE'
jq -e '((.tasks // []) | length) == 0' "$state_dir/pending.json" >/dev/null || fail 'BOOTSTRAP_STATE_NOT_PRISTINE'
jq -e '((.match_review // []) | length) == 0' "$state_dir/review.json" >/dev/null || fail 'BOOTSTRAP_STATE_NOT_PRISTINE'
if [[ -f "$state_dir/assistant-task-gates.json" ]]; then
  jq -e '.schema_version == 1 and ((.records // []) | length) == 0' "$state_dir/assistant-task-gates.json" >/dev/null \
    || fail 'BOOTSTRAP_STATE_NOT_PRISTINE'
fi

[[ "$batch_size" =~ ^[0-9]+$ ]] && (( batch_size >= 1 && batch_size <= 200 )) \
  || fail 'BOOTSTRAP_INVALID_BATCH_SIZE'
[[ "$max_requests" =~ ^[0-9]+$ ]] && (( max_requests >= 1 )) \
  || fail 'BOOTSTRAP_INVALID_REQUEST_BUDGET'
[[ "$threshold" =~ ^[0-9]+$ ]] && (( threshold >= 1 )) \
  || fail 'BOOTSTRAP_INVALID_THRESHOLD'

author_count="$(jq '[.authors[] | select(.enabled != false)] | length' "$authors_file")"
unique_author_count="$(jq '[.authors[] | select(.enabled != false) | .name | select(type == "string" and length > 0)] | unique | length' "$authors_file")"
test_author_count="$(jq '[.authors[] | select(.enabled != false) | select((.author_id // "") | startswith("AUTHOR_TEST_"))] | length' "$authors_file")"
[[ "$author_count" =~ ^[0-9]+$ ]] && (( author_count >= 1 )) || fail 'BOOTSTRAP_NO_ENABLED_AUTHORS'
[[ "$unique_author_count" == "$author_count" ]] || fail 'BOOTSTRAP_AUTHOR_REGISTRY_INVALID'
[[ "$test_author_count" == "0" ]] || fail 'BOOTSTRAP_TEST_AUTHOR_ENABLED'
batch_count="$(( (author_count + batch_size - 1) / batch_size ))"

[[ ! -e "$report_root" ]] || fail 'BOOTSTRAP_REPORT_ROOT_EXISTS'
mkdir -p "$report_root"

base_commit="$(git rev-parse HEAD)"
input_state="$state_dir"
final_state=''

for ((batch_index=0; batch_index<batch_count; batch_index++)); do
  stage="$report_root/batch-$batch_index"
  mkdir "$stage"
  extra=()
  if (( batch_index > 0 )); then
    extra+=(--continue-cycle)
  fi

  "$phase3b_bin" \
    --state "$input_state" \
    --output "$stage" \
    --authors "$authors_file" \
    --mode full \
    --threshold "$threshold" \
    --batch-size "$batch_size" \
    --batch-index "$batch_index" \
    --expected-base-sha "$base_commit" \
    --actual-base-sha "$base_commit" \
    --max-requests "$max_requests" \
    --author-concurrency "$author_concurrency" \
    --repair-overlay "$repair_overlay" \
    --live-source \
    "${extra[@]}"

  [[ -f "$stage/scan-report.json" && -f "$stage/state-manifest.json" ]] || fail 'BOOTSTRAP_BATCH_EVIDENCE_MISSING'
  jq -e '
    .complete == true and
    (.strategy_complete // .complete) == true and
    ((.source_error_boundaries // 0) == 0)
  ' "$stage/scan-report.json" >/dev/null || fail 'BOOTSTRAP_BATCH_INCOMPLETE' 70
  jq -e \
    --arg base "$base_commit" \
    --argjson batch "$batch_index" \
    --argjson count "$batch_count" '
      .base_commit == $base and
      .requested_mode == "full" and
      .batch_index == $batch and
      .batch_count == $count and
      .complete == true and
      (.strategy_complete // .complete) == true
    ' "$stage/state-manifest.json" >/dev/null || fail 'BOOTSTRAP_MANIFEST_MISMATCH' 70

  input_state="$stage"
  final_state="$stage"
done

[[ -n "$final_state" ]] || fail 'BOOTSTRAP_NO_FINAL_STATE'
for name in checkpoint.json authors.json catalog.json inventory_index.json pending.json review.json decisions.json scan_state.json latest.json state-manifest.json; do
  [[ -f "$final_state/$name" ]] || fail "BOOTSTRAP_FINAL_STATE_MISSING:$name" 70
done

json_equal "$state_dir/authors.json" "$final_state/authors.json" || fail 'BOOTSTRAP_AUTHORS_DRIFT' 70
json_equal "$state_dir/inventory_index.json" "$final_state/inventory_index.json" || fail 'BOOTSTRAP_INVENTORY_DRIFT' 70
json_equal "$state_dir/decisions.json" "$final_state/decisions.json" || fail 'BOOTSTRAP_DECISIONS_DRIFT' 70
jq -e '
  .schema_version == 3 and
  .phase3a_scan.scan_id != "" and
  .phase3a_scan.requested_mode == "full" and
  .phase3a_scan.complete == true and
  ((.phase3a_scan.direct_failures // {}) | length) == 0
' "$final_state/scan_state.json" >/dev/null || fail 'BOOTSTRAP_FINAL_SCAN_NOT_COMPLETE' 70
expected_full_keys="$(( author_count * 2 ))"
actual_full_keys="$(jq '(.phase3a_scan.last_full // {}) | length' "$final_state/scan_state.json")"
[[ "$actual_full_keys" == "$expected_full_keys" ]] || fail 'BOOTSTRAP_FULL_COVERAGE_MISMATCH' 70
while IFS= read -r author; do
  for source in jm pica; do
    jq -e --arg key "$source|$author" '
      (.phase3a_scan.last_full[$key] // "") | type == "string" and length > 0
    ' "$final_state/scan_state.json" >/dev/null || fail 'BOOTSTRAP_FULL_COVERAGE_MISMATCH' 70
  done
done < <(jq -r '.authors[] | select(.enabled != false) | .name' "$authors_file")
jq -e '
  .schema_version == 3 and
  (.scan_id | type == "string" and length > 0) and
  .scan_status == "COMPLETE"
' "$final_state/latest.json" >/dev/null || fail 'BOOTSTRAP_FINAL_SCAN_NOT_COMPLETE' 70

# Nothing above mutates durable monitor-state. Only a fully accumulated and
# verified full-scan state reaches the existing race-protected committer once.
"$commit_monitor_state" "$base_commit" "$final_state" "$state_dir"

echo "PHASE3B_BOOTSTRAP_COMPLETE batches=$batch_count authors=$author_count batch_size=$batch_size author_concurrency=$author_concurrency max_requests=$max_requests base=$base_commit"
