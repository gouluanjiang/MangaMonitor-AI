#!/usr/bin/env bash
set -euo pipefail

source_stage="${1:?staged Phase 3B state is required}"
helper="$(cd "$(dirname "$0")" && pwd)/commit-monitor-state.sh"
test_root="$(mktemp -d)"
origin="$test_root/origin.git"

git init --bare --initial-branch=main "$origin" >/dev/null
git clone -q "$origin" "$test_root/seed"
(
  cd "$test_root/seed"
  echo seed > README.md
  git -c user.name=test -c user.email=test@example.invalid add README.md
  git -c user.name=test -c user.email=test@example.invalid commit -m seed >/dev/null
  git push -q origin main
)

git clone -q "$origin" "$test_root/worker"
base="$(git -C "$test_root/worker" rev-parse HEAD)"
cp -a "$source_stage" "$test_root/stage-success"
jq --arg base "$base" '.base_commit=$base' "$test_root/stage-success/state-manifest.json" > "$test_root/manifest.tmp"
mv "$test_root/manifest.tmp" "$test_root/stage-success/state-manifest.json"
(
  cd "$test_root/worker"
  "$helper" "$base" "$test_root/stage-success" monitor-state origin main >/dev/null
)
successful_commit="$(git -C "$test_root/worker" rev-parse HEAD)"

git clone -q "$origin" "$test_root/concurrent"
(
  cd "$test_root/concurrent"
  echo concurrent > concurrent.txt
  git -c user.name=test -c user.email=test@example.invalid add concurrent.txt
  git -c user.name=test -c user.email=test@example.invalid commit -m concurrent >/dev/null
  git push -q origin main
)

cp -a "$source_stage" "$test_root/stage-conflict"
jq --arg base "$successful_commit" '.base_commit=$base' "$test_root/stage-conflict/state-manifest.json" > "$test_root/manifest.tmp"
mv "$test_root/manifest.tmp" "$test_root/stage-conflict/state-manifest.json"
set +e
(
  cd "$test_root/worker"
  "$helper" "$successful_commit" "$test_root/stage-conflict" monitor-state origin main >/dev/null
)
conflict_exit=$?
set -e
if [[ "$conflict_exit" != "75" ]]; then
  echo "EXPECTED_COMMIT_CONFLICT, got $conflict_exit" >&2
  exit 1
fi

git clone -q "$origin" "$test_root/recovery"
recovery_base="$(git -C "$test_root/recovery" rev-parse HEAD)"
cp -a "$source_stage" "$test_root/stage-recovery"
jq --arg base "$recovery_base" '.base_commit=$base' "$test_root/stage-recovery/state-manifest.json" > "$test_root/manifest.tmp"
mv "$test_root/manifest.tmp" "$test_root/stage-recovery/state-manifest.json"
(
  cd "$test_root/recovery"
  "$helper" "$recovery_base" "$test_root/stage-recovery" monitor-state origin main >/dev/null
)
recovery_commit="$(git -C "$test_root/recovery" rev-parse HEAD)"

jq -n \
  --arg successful_commit "$successful_commit" \
  --argjson conflict_exit "$conflict_exit" \
  --arg recovery_commit "$recovery_commit" \
  --arg test_root "$test_root" \
  '{successful_commit:$successful_commit, conflict_exit:$conflict_exit, recovery_commit:$recovery_commit, test_root:$test_root}'
