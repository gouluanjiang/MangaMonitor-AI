#!/usr/bin/env bash
set -euo pipefail

expected_base="${1:?expected base commit is required}"
staging_dir="${2:?staging directory is required}"
state_dir="${3:-monitor-state}"
remote_name="${4:-origin}"
branch_name="${5:-main}"

case "$state_dir" in
  monitor-state|*/monitor-state) ;;
  *) echo "STATE_TARGET_NOT_ALLOWED: $state_dir" >&2; exit 64 ;;
esac

required=(
  checkpoint.json authors.json catalog.json inventory_index.json pending.json
  review.json decisions.json scan_state.json latest.json state-manifest.json
)
optional=(
  observations.json scan-report.json state-diff.json review-export.json
  review-export.csv review-reason-summary.json review-reason-summary.csv
  sanitized-state-sample.json
)

for name in "${required[@]}"; do
  test -f "$staging_dir/$name" || {
    echo "STAGED_STATE_MISSING: $name" >&2
    exit 65
  }
done

manifest_base="$(jq -r '.base_commit // empty' "$staging_dir/state-manifest.json")"
if [[ "$manifest_base" != "$expected_base" ]]; then
  echo "MANIFEST_BASE_MISMATCH" >&2
  exit 66
fi

local_head="$(git rev-parse HEAD)"
if [[ "$local_head" != "$expected_base" ]]; then
  echo "LOCAL_BASE_MISMATCH: expected $expected_base, found $local_head" >&2
  exit 75
fi

git fetch --quiet "$remote_name" "$branch_name"
remote_head="$(git rev-parse "$remote_name/$branch_name")"
if [[ "$remote_head" != "$expected_base" ]]; then
  echo "REMOTE_BASE_CHANGED: expected $expected_base, found $remote_head" >&2
  exit 75
fi

mkdir -p "$state_dir"
for name in "${required[@]}" "${optional[@]}"; do
  if [[ -f "$staging_dir/$name" ]]; then
    install -m 0644 "$staging_dir/$name" "$state_dir/$name"
  fi
done

git add -- "$state_dir"
if git diff --cached --quiet; then
  echo "STATE_UNCHANGED"
  exit 0
fi

git -c user.name='MangaMonitor Action' \
    -c user.email='mangamonitor-action@users.noreply.github.com' \
    commit -m "Update monitor state" -- "$state_dir"

# Check again after preparing the commit. Never force-push over a decision or
# another scan that landed while this job was running.
git fetch --quiet "$remote_name" "$branch_name"
remote_head="$(git rev-parse "$remote_name/$branch_name")"
if [[ "$remote_head" != "$expected_base" ]]; then
  echo "REMOTE_CHANGED_DURING_COMMIT: expected $expected_base, found $remote_head" >&2
  exit 75
fi
git push "$remote_name" "HEAD:$branch_name"
