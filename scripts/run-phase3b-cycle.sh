#!/usr/bin/env bash
set -euo pipefail

config_file="${1:-monitor-config.json}"
requested_mode="${2:-$(jq -r '.default_mode' "$config_file")}"
report_root="${3:-reports/phase3b-production}"

if [[ "$(jq -r '.production_enabled' "$config_file")" != "true" ]]; then
  echo "PRODUCTION_MONITOR_DISABLED" >&2
  exit 69
fi

state_dir="$(jq -r '.state_directory' "$config_file")"
authors_file="$(jq -r '.author_file' "$config_file")"
batch_size="$(jq -r '.soft_batch_size' "$config_file")"
max_requests="$(jq -r '.max_requests_per_batch // 400' "$config_file")"
threshold="$(jq -r '.historical_id_threshold' "$config_file")"
# V1 deliberately caps simultaneous author lanes at three. This is execution
# concurrency inside a soft batch, not a request-rate or batch-size multiplier.
author_concurrency=3
author_count="$(jq 'if type == "array" then length else [.authors[] | select(.enabled != false)] | length end' "$authors_file")"
batch_count="$(( (author_count + batch_size - 1) / batch_size ))"

if (( author_count == 0 || batch_size < 1 || batch_size > 200 )); then
  echo "INVALID_PRODUCTION_BATCH_CONFIG" >&2
  exit 64
fi
if ! [[ "$max_requests" =~ ^[0-9]+$ ]] || (( max_requests < 1 )); then
  echo "INVALID_PRODUCTION_REQUEST_BUDGET" >&2
  exit 64
fi
if (( author_concurrency < 1 || author_concurrency > 3 )); then
  echo "INVALID_PRODUCTION_AUTHOR_CONCURRENCY" >&2
  exit 64
fi

# `complete` continues to mean full source coverage. Incremental scans are allowed to
# advance after a verified EARLY_STOP_HEURISTIC, exposed separately as strategy_complete.
# Older manifests fall back to their historical `complete` field.
manifest_strategy_complete() {
  jq -r '(.strategy_complete // .complete // false)' "$1"
}

start_batch=0
resume=false
continue_cycle=false
manifest="$state_dir/state-manifest.json"
if [[ -f "$manifest" && "$(jq -r '.requested_mode' "$manifest")" == "$requested_mode" ]]; then
  previous_batch="$(jq -r '.batch_index' "$manifest")"
  previous_count="$(jq -r '.batch_count' "$manifest")"
  if [[ "$previous_count" == "$batch_count" ]]; then
    if [[ "$(manifest_strategy_complete "$manifest")" == "true" ]]; then
      if (( previous_batch + 1 < batch_count )); then
        start_batch="$((previous_batch + 1))"
        continue_cycle=true
      fi
    else
      start_batch="$previous_batch"
      resume=true
      continue_cycle=true
    fi
  fi
fi

mkdir -p "$report_root"
for ((batch_index=start_batch; batch_index<batch_count; batch_index++)); do
  base_commit="$(git rev-parse HEAD)"
  stage="$report_root/batch-$batch_index"
  mkdir -p "$stage"
  extra=()
  if [[ "$resume" == "true" ]]; then
    # The committed partial checkpoint is copied to the separate staging path;
    # the runner still never mutates monitor-state directly.
    cp -a "$state_dir/." "$stage/"
    extra+=(--resume)
  elif [[ "$continue_cycle" == "true" ]]; then
    extra+=(--continue-cycle)
  fi

  ./target/release/phase3b \
    --state "$state_dir" \
    --output "$stage" \
    --authors "$authors_file" \
    --mode "$requested_mode" \
    --threshold "$threshold" \
    --batch-size "$batch_size" \
    --batch-index "$batch_index" \
    --expected-base-sha "$base_commit" \
    --actual-base-sha "$base_commit" \
    --max-requests "$max_requests" \
    --author-concurrency "$author_concurrency" \
    --repair-overlay fixtures/matcher-m2/inventory-primary-repair.json \
    --live-source \
    "${extra[@]}"

  scripts/commit-monitor-state.sh "$base_commit" "$stage" "$state_dir"
  if [[ "$(manifest_strategy_complete "$stage/state-manifest.json")" != "true" ]]; then
    echo "PARTIAL_BATCH_COMMITTED_FOR_RESUME: $batch_index" >&2
    exit 70
  fi
  resume=false
  continue_cycle=true
done

echo "PHASE3B_CYCLE_COMPLETE batches=$batch_count authors=$author_count batch_size=$batch_size author_concurrency=$author_concurrency max_requests=$max_requests mode=$requested_mode"
