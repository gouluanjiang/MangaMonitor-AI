#!/usr/bin/env bash
# Offline orchestrator contract. The runner, publisher and git are test doubles;
# no source traffic, remote repository, production state or local library is used.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/monitor-state"
state="$tmp/monitor-state"
export FAKE_ROOT="$tmp"
export PATH="$tmp/bin:$PATH"

cat > "$tmp/bin/git" <<'SH'
#!/usr/bin/env bash
[[ "$*" == 'rev-parse HEAD' ]] || exit 99
echo fixture-public-base
SH
cat > "$tmp/bin/runner" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
input='' output='' batch='' mode='' preflight=false recover=false resume=false continued=false
while (($#)); do
  case "$1" in
    --state) input="$2"; shift 2 ;;
    --output) output="$2"; shift 2 ;;
    --batch-index) batch="$2"; shift 2 ;;
    --mode) mode="$2"; shift 2 ;;
    --resume-preflight) preflight=true; shift ;;
    --recover-authority-drift-full) recover=true; shift ;;
    --resume) resume=true; shift ;;
    --continue-cycle) continued=true; shift ;;
    --authors|--threshold|--batch-size|--expected-base-sha|--actual-base-sha|--max-requests|--author-concurrency|--recovery-from-batch-index) shift 2 ;;
    --live-source) shift ;;
    *) echo "UNKNOWN_MOCK_ARGUMENT:$1" >&2; exit 90 ;;
  esac
done
marker="$(jq -r '.recovery.kind // empty' "$input/state-manifest.json")"
if [[ "$preflight" == true ]]; then
  # Check that orchestration copied checkpoint evidence, but left input intact.
  cmp "$input/checkpoint.json" "$output/checkpoint.json"
  classification="${FAKE_CLASSIFICATION:-AUTHORITY_DRIFT_REQUIRES_FULL_RECOVERY}"
  [[ "$marker" != AUTHORITY_DRIFT_FULL ]] || classification=RESUMABLE_EXACT
  jq -n --arg classification "$classification" '{classification:$classification,reason:"fixture"}'
  exit 0
fi
effective=incremental
recovery=null
if [[ "$recover" == true || ( "$marker" == AUTHORITY_DRIFT_FULL && ( "$resume" == true || "$continued" == true ) ) ]]; then
  effective=full
  recovery='{"kind":"AUTHORITY_DRIFT_FULL","generation_id":"fixture-generation"}'
fi
complete=true
if [[ "${FAKE_PARTIAL_ONCE:-false}" == true && "$effective" == full && "$batch" == 1 && ! -e "$FAKE_ROOT/partial-used" ]]; then
  complete=false
  touch "$FAKE_ROOT/partial-used"
fi
printf '%s|%s|%s|%s|%s\n' "$batch" "$effective" "$recover" "$resume" "$continued" >> "$FAKE_ROOT/run.log"
mkdir -p "$output"
cp -a "$input/." "$output/"
count="$(jq '[.authors[] | select(.enabled != false)] | length' "$input/authors.json")"
jq -n --arg mode "$mode" --arg effective "$effective" --argjson batch "$batch" \
  --argjson count "$count" --argjson complete "$complete" --argjson recovery "$recovery" \
  '{requested_mode:$mode,effective_requested_mode:$effective,batch_index:$batch,batch_count:$count,complete:$complete,strategy_complete:$complete,recovery:$recovery}' > "$output/state-manifest.json"
printf '%s\n' "$batch-$effective-$complete" > "$output/checkpoint.json"
SH
cat > "$tmp/bin/commit" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$1" >> "$FAKE_ROOT/commit.log"
cp -a "$2/." "$3/"
SH
chmod +x "$tmp/bin/git" "$tmp/bin/runner" "$tmp/bin/commit"

reset_state() {
  cat > "$state/authors.json" <<'JSON'
{"authors":[{"name":"Writer0","enabled":true},{"name":"Writer1","enabled":true}]}
JSON
  cat > "$state/state-manifest.json" <<'JSON'
{"requested_mode":"monthly","batch_index":1,"batch_count":2,"complete":false,"strategy_complete":false,"recovery":null}
JSON
  echo interrupted-monthly-checkpoint > "$state/checkpoint.json"
  : > "$tmp/run.log"
  : > "$tmp/commit.log"
  jq -n --arg state "$state" '{production_enabled:true,state_directory:$state,author_file:($state+"/authors.json"),default_mode:"monthly",soft_batch_size:1,max_requests_per_batch:20,historical_id_threshold:5}' > "$tmp/config.json"
}

run_cycle() {
  PHASE3B_BIN="$tmp/bin/runner" COMMIT_MONITOR_STATE="$tmp/bin/commit" \
    bash "$repo_root/scripts/run-phase3b-cycle.sh" "$tmp/config.json" monthly "$tmp/reports with spaces"
}

reset_state
set +e
FAKE_PARTIAL_ONCE=true run_cycle > "$tmp/first.out" 2>&1
status=$?
set -e
test "$status" -eq 70
grep -Fxq '0|full|true|false|false' "$tmp/run.log"
grep -Fxq '1|full|false|false|true' "$tmp/run.log"
test "$(wc -l < "$tmp/commit.log")" -eq 2
jq -e '.recovery.kind == "AUTHORITY_DRIFT_FULL" and .strategy_complete == false' "$state/state-manifest.json" >/dev/null
: > "$tmp/run.log"
: > "$tmp/commit.log"
run_cycle > "$tmp/second.out" 2>&1
test "$(cat "$tmp/run.log")" == '1|full|false|true|false'
test "$(wc -l < "$tmp/commit.log")" -eq 1
jq -e '.recovery.kind == "AUTHORITY_DRIFT_FULL" and .strategy_complete == true' "$state/state-manifest.json" >/dev/null
: > "$tmp/run.log"
run_cycle > "$tmp/third.out" 2>&1
grep -Fxq '0|incremental|false|false|false' "$tmp/run.log"
grep -Fxq '1|incremental|false|false|true' "$tmp/run.log"
jq -e '.recovery == null' "$state/state-manifest.json" >/dev/null

for classification in OPTIONS_MISMATCH CHECKPOINT_CORRUPT CURRENT_STATE_INVALID NOT_PARTIAL UNRECOGNIZED; do
  reset_state
  before="$(cat "$state/checkpoint.json")"
  if FAKE_CLASSIFICATION="$classification" run_cycle > "$tmp/rejected.out" 2>&1; then
    echo "EXPECTED_CLASSIFICATION_REJECTION:$classification" >&2; exit 1
  fi
  test ! -s "$tmp/run.log"
  test ! -s "$tmp/commit.log"
  test "$(cat "$state/checkpoint.json")" == "$before"
done

reset_state
jq '.authors += [{"name":"Writer2","enabled":true}]' "$state/authors.json" > "$tmp/authors.json"
cp "$tmp/authors.json" "$state/authors.json"
run_cycle > "$tmp/registry.out" 2>&1
test "$(wc -l < "$tmp/run.log")" -eq 3
grep -Fxq '0|full|true|false|false' "$tmp/run.log"
grep -Fxq '2|full|false|false|true' "$tmp/run.log"

reset_state
jq '.production_enabled=false' "$tmp/config.json" > "$tmp/disabled.json"
cp "$tmp/disabled.json" "$tmp/config.json"
if run_cycle > "$tmp/disabled.out" 2>&1; then exit 1; fi
test ! -s "$tmp/run.log"
test ! -s "$tmp/commit.log"
echo PHASE3B_AUTHORITY_RECOVERY_ORCHESTRATOR_TESTS_PASSED
