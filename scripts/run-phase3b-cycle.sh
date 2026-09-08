#!/usr/bin/env bash
set -euo pipefail

config_file="${1:-monitor-config.json}"
requested_mode="${2:-$(jq -r '.default_mode' "$config_file")}"
report_root="${3:-reports/phase3b-production}"
phase3b_bin="${PHASE3B_BIN:-./target/release/phase3b}"
commit_monitor_state="${COMMIT_MONITOR_STATE:-scripts/commit-monitor-state.sh}"

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
if ! [[ "$batch_size" =~ ^[0-9]+$ ]] || (( batch_size < 1 || batch_size > 200 )); then
  echo "INVALID_PRODUCTION_BATCH_CONFIG" >&2
  exit 64
fi
author_count="$(jq 'if type == "array" then length else [.authors[] | select(.enabled != false)] | length end' "$authors_file")"
if (( author_count == 0 )); then
  echo "INVALID_PRODUCTION_BATCH_CONFIG" >&2
  exit 64
fi
batch_count="$(( (author_count + batch_size - 1) / batch_size ))"
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
recover=false
continue_cycle=false
manifest="$state_dir/state-manifest.json"
state_abs="$(realpath -e "$state_dir")"
report_abs="$(realpath -m "$report_root")"
case "$report_abs/" in
  "$state_abs/"*) echo "PRODUCTION_REPORT_ROOT_INSIDE_STATE" >&2; exit 64 ;;
esac
case "$state_abs/" in
  "$report_abs/"*) echo "PRODUCTION_REPORT_ROOT_CONTAINS_STATE" >&2; exit 64 ;;
esac
mkdir -p "$report_root"
# Separate invocation directories retain abandoned staging evidence and avoid
# reusing a stale output checkpoint from another generation.
cycle_reports="$(mktemp -d "$report_root/cycle-XXXXXXXX")"
if [[ -f "$manifest" ]]; then
  previous_batch="$(jq -r '.batch_index' "$manifest")"
  previous_count="$(jq -r '.batch_count' "$manifest")"
  if [[ "$(manifest_strategy_complete "$manifest")" != "true" ]]; then
    preflight_stage="$cycle_reports/preflight"
    mkdir -p "$preflight_stage"
    cp -a "$state_dir/." "$preflight_stage/"
    # The CLI reads current public exports and the copied checkpoint without
    # writing either. Only this enum is classified; stderr is never parsed.
    preflight="$("$phase3b_bin" \
      --resume-preflight --state "$state_dir" --output "$preflight_stage" \
      --authors "$authors_file" --mode "$requested_mode" --threshold "$threshold" \
      --batch-size "$batch_size" --batch-index "$previous_batch")"
    printf '%s\n' "$preflight" > "$cycle_reports/resume-preflight.json"
    case "$(jq -er '.classification' <<< "$preflight")" in
      RESUMABLE_EXACT)
        start_batch="$previous_batch"
        resume=true
        continue_cycle=true
        ;;
      AUTHORITY_DRIFT_REQUIRES_FULL_RECOVERY)
        start_batch=0
        recover=true
        ;;
      *) echo "PHASE3B_RESUME_PREFLIGHT_REJECTED: $preflight" >&2; exit 65 ;;
    esac
  elif [[ "$(jq -r '.requested_mode' "$manifest")" == "$requested_mode" && "$previous_count" == "$batch_count" ]]; then
      if (( previous_batch + 1 < batch_count )); then
        start_batch="$((previous_batch + 1))"
        continue_cycle=true
      fi
  elif [[ "$(jq -r '.recovery.kind // empty' "$manifest")" == "AUTHORITY_DRIFT_FULL" ]] \
       && (( previous_batch + 1 < previous_count )); then
    echo "RECOVERY_CYCLE_OPTIONS_MISMATCH" >&2
    exit 65
  fi
fi

for ((batch_index=start_batch; batch_index<batch_count; batch_index++)); do
  base_commit="$(git rev-parse HEAD)"
  stage="$cycle_reports/batch-$batch_index"
  mkdir -p "$stage"
  extra=()
  if [[ "$resume" == "true" || "$recover" == "true" ]]; then
    # The committed partial checkpoint is copied to the separate staging path;
    # the runner still never mutates monitor-state directly.
    cp -a "$state_dir/." "$stage/"
    if [[ "$recover" == "true" ]]; then
      extra+=(--recover-authority-drift-full --recovery-from-batch-index "$previous_batch")
    else
      extra+=(--resume)
    fi
  elif [[ "$continue_cycle" == "true" ]]; then
    extra+=(--continue-cycle)
  fi

  "$phase3b_bin" \
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
    --live-source \
    "${extra[@]}"

  "$commit_monitor_state" "$base_commit" "$stage" "$state_dir"
  if [[ "$(manifest_strategy_complete "$stage/state-manifest.json")" != "true" ]]; then
    echo "PARTIAL_BATCH_COMMITTED_FOR_RESUME: $batch_index" >&2
    exit 70
  fi
  resume=false
  recover=false
  continue_cycle=true
done

echo "PHASE3B_CYCLE_COMPLETE batches=$batch_count authors=$author_count batch_size=$batch_size author_concurrency=$author_concurrency max_requests=$max_requests mode=$requested_mode"
