#!/usr/bin/env python3
"""Run explicitly ignored native probes with ENOSPC in an owned Linux subtree.

Build the workbench-downloads test executable separately using locked deps.
This driver never builds Rust, opens a real profile, or fills the host disk.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


parser = argparse.ArgumentParser()
parser.add_argument("--binary", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--shim-source", type=Path, default=Path(__file__).with_name("synthetic_enospc.c"))
args = parser.parse_args()
if sys.platform != "linux":
    parser.error("this diagnostic requires Linux; it does not validate Windows disk behavior")
if os.environ.get("LD_PRELOAD"):
    parser.error("an existing LD_PRELOAD is present; refusing to replace or bypass it")
binary = args.binary.resolve(strict=True)
out = args.output.resolve()
out.mkdir(parents=True, exist_ok=False)
root = Path(tempfile.mkdtemp(prefix="synthetic-fault-", dir=out))
(root / ".synthetic-fault-root").write_text("isolated synthetic ENOSPC probe\n")
control = out / "fault-control.txt"
trace = out / "fault-trace.txt"
shim = out / "synthetic-enospc.so"
subprocess.run(["cc", "-shared", "-fPIC", "-O2", "-Wall", "-Wextra", "-o", str(shim), str(args.shim_source.resolve(strict=True))], check=True)
env = dict(os.environ)
env.update({
    "LD_PRELOAD": str(shim),
    "TMPDIR": str(root),
    "MANGAMONITOR_FAULT_ROOT": str(root),
    "MANGAMONITOR_FAULT_CONTROL": str(control),
    "MANGAMONITOR_FAULT_TRACE": str(trace),
    "CI": "true",
    "GITHUB_ACTIONS": "true",
    "RUST_BACKTRACE": "1",
})
self_probe = r'''
import errno,os
from pathlib import Path
root=Path(os.environ['MANGAMONITOR_FAULT_ROOT'])
control=Path(os.environ['MANGAMONITOR_FAULT_CONTROL'])
scope=root/'shim-self-check'; scope.mkdir()
target=scope/'partial.bin'
outside=root.parent/'outside-injection-sentinel.txt'
outside.write_bytes(b'unrelated synthetic bytes')
control.write_text(str(scope)+'/\n3\n')
try:
    target.write_bytes(b'0123456789')
    raise AssertionError('fault did not fire')
except OSError as error:
    assert error.errno==errno.ENOSPC, error
finally:
    control.unlink()
assert target.read_bytes()==b'012'
assert outside.read_bytes()==b'unrelated synthetic bytes'
with target.open('ab') as stream: stream.write(b'3456789')
assert target.read_bytes()==b'0123456789'
# Even an armed prefix outside the marked root must be ignored.
control.write_text(str(root.parent)+'/\n0\n')
try: outside.write_bytes(b'still outside the injection scope')
finally: control.unlink()
assert outside.read_bytes()==b'still outside the injection scope'
print('SYNTHETIC_ENOSPC_SHIM_SCOPE_AND_RECOVERY_VERIFIED')
'''
with (out / "shim-self-check.log").open("w") as log:
    subprocess.run([sys.executable, "-c", self_probe], env=env, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=20)
started = datetime.datetime.now(datetime.timezone.utc).isoformat()
command = [str(binary), "synthetic_enospc", "--ignored", "--test-threads=1", "--nocapture"]
timed_out = False
with (out / "native-probes.log").open("w") as log:
    try:
        completed = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=180)
        exit_code = completed.returncode
    except subprocess.TimeoutExpired:
        timed_out = True
        exit_code = 124
ended = datetime.datetime.now(datetime.timezone.utc).isoformat()
control.unlink(missing_ok=True)
log_text = (out / "native-probes.log").read_text(errors="replace")
summaries = re.findall(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored", log_text)
result_count = sum(int(p)+int(f) for p,f,_ in summaries)
if result_count != 5 or not summaries:
    exit_code = exit_code or 2
result = {
    "kind": "linux-generated-media-and-isolated-storage-ENOSPC",
    "started": started, "ended": ended,
    "harnessRevision": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
    "workspaceStatus": subprocess.check_output(["git", "status", "--porcelain=v1"], text=True).strip(),
    "driverSha256": sha(Path(__file__)),
    "shimSourceSha256": sha(args.shim_source.resolve(strict=True)),
    "binary": str(binary), "binarySha256": sha(binary), "shimSha256": sha(shim),
    "nativeProbeExitCode": exit_code, "timedOut": timed_out, "expectedProbeCount": 5, "reportedProbeCount": result_count,
    "nativeSummaries": summaries,
    "passed": exit_code == 0,
    "ownedRoot": str(root),
    "preservedFailureRoots": re.findall(r"PRESERVED_SYNTHETIC_FAULT_ROOT=(.+)", log_text),
    "injectedWriteFailures": len(trace.read_text().splitlines()),
    "limits": ["Synthetic saved staging only; no source clients or real media.", "ENOSPC applies to the selected synthetic path prefix, not an actual full filesystem.", "Linux syscall boundary; Windows full-volume behavior remains a separate acceptance item."],
}
(out / "summary.json").write_text(json.dumps(result, indent=2)+"\n")
print(json.dumps(result, indent=2))
raise SystemExit(exit_code)
