#!/usr/bin/env python3
"""Summarize flushed synthetic soak evidence without claiming unfinished passes."""
import argparse
import datetime as dt
import json
import math
import re
import statistics
from pathlib import Path


warnings = []


def rows(path):
    if not path.exists():
        return []
    raw = path.read_bytes()
    lines = raw.splitlines()
    result = []
    for index, line in enumerate(lines):
        try:
            result.append(json.loads(line))
        except (json.JSONDecodeError, UnicodeDecodeError):
            if index == len(lines) - 1 and not raw.endswith(b"\n"):
                warnings.append(f"Ignored one unflushed final line: {path.name}")
            else:
                raise ValueError(f"Malformed evidence at {path}:{index + 1}") from None
    return result


def stats(values):
    values = sorted(value for value in values if value is not None)
    if not values:
        return None
    return {
        "samples": len(values), "min": values[0],
        "median": statistics.median(values),
        "p95": values[max(0, math.ceil(len(values) * .95) - 1)],
        "max": values[-1],
    }


def unix_ms(value):
    if isinstance(value, (int, float)):
        return value
    return dt.datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp() * 1000


def native_status(row, field):
    match = re.search(rf"^{field}:\s+(\d+)", row["resources"]["status"], re.M)
    return int(match[1]) if match else None


def native_ticks(row):
    raw = row["resources"]["stat"]
    fields = raw[raw.rindex(")") + 2:].split()
    return int(fields[11]) + int(fields[12])


def cpu_samples(window, ticks_per_second):
    samples = []
    for a, b in zip(window, window[1:]):
        seconds = (b["elapsedMs"] - a["elapsedMs"]) / 1000
        if seconds <= 0:
            continue
        if "processes" in a:
            previous = {p["pid"]: p["cpuTicks"] for p in a.get("processes") or []}
            delta = sum(p["cpuTicks"] - previous[p["pid"]]
                        for p in b.get("processes") or [] if p["pid"] in previous)
        else:
            delta = native_ticks(b) - native_ticks(a)
        samples.append(delta / ticks_per_second / seconds * 100)
    return samples


def measured_resources(window, ticks_per_second, logical_cpus, quota_cores):
    if not window:
        return {}
    result = {}
    if "metrics" in window[0]:
        for name in ["JSHeapUsedSize", "Nodes", "JSEventListeners", "Documents"]:
            result[name] = stats(row["metrics"].get(name) for row in window)
        result["summedRssKb"] = stats(sum(p["rssKb"] for p in row["processes"])
                                      for row in window if isinstance(row.get("processes"), list))
        result["summedFds"] = stats(sum(p["fds"] for p in row["processes"] if p["fds"] is not None)
                                    for row in window if isinstance(row.get("processes"), list))
        result["runnerRssBytes"] = stats(row["runnerMemory"]["rss"] for row in window)
        result["activeIPC"] = stats(row["state"]["active"] for row in window)
        result["rendererMainThreadCpuPercentOneCore"] = stats(
            (b["metrics"]["TaskDuration"] - a["metrics"]["TaskDuration"]) /
            ((b["elapsedMs"] - a["elapsedMs"]) / 1000) * 100
            for a, b in zip(window, window[1:]) if b["elapsedMs"] > a["elapsedMs"])
        a, b = window[0], window[-1]
        counts = {key: b["state"].get(key, 0) - a["state"].get(key, 0)
                  for key in ["requests", "failures", "injected", "retryAttempts"]}
        seconds = (b["elapsedMs"] - a["elapsedMs"]) / 1000
        counts["secondsBetweenSamples"] = seconds
        counts["requestsPerSecond"] = counts["requests"] / seconds if seconds else None
        counts["boundaryErrorFraction"] = counts["failures"] / counts["requests"] if counts["requests"] else None
        result["boundaryCounterDeltas"] = counts
        result["catalogSizes"] = sorted({row["state"]["size"] for row in window})
    else:
        result["rssKb"] = stats(native_status(row, "VmRSS") for row in window)
        result["threads"] = stats(native_status(row, "Threads") for row in window)
        result["fds"] = stats(row["resources"]["fds"] for row in window)
    if ticks_per_second:
        cpu = cpu_samples(window, ticks_per_second)
        result["sampledProcessCpuPercentOneCore"] = stats(cpu)
        if logical_cpus:
            result["sampledProcessCpuPercentOfReportedLogicalCpus"] = stats(x / logical_cpus for x in cpu)
        if quota_cores:
            result["sampledProcessCpuPercentOfCgroupQuota"] = stats(x / quota_cores for x in cpu)
    return result


parser = argparse.ArgumentParser()
parser.add_argument("directory", type=Path)
parser.add_argument("--clock-ticks", type=int, help="recorded CLK_TCK on the measured host")
parser.add_argument("--logical-cpus", type=int, help="recorded logical CPU count on measured host")
parser.add_argument("--cpu-quota-cores", type=float, help="recorded cgroup cpu.max quota divided by period")
args = parser.parse_args()
if args.clock_ticks is not None and args.clock_ticks <= 0:
    parser.error("--clock-ticks must be positive")
if args.logical_cpus is not None and args.logical_cpus <= 0:
    parser.error("--logical-cpus must be positive")
if args.cpu_quota_cores is not None and args.cpu_quota_cores <= 0:
    parser.error("--cpu-quota-cores must be positive")
root = args.directory
summary = json.loads((root / "summary.json").read_text()) if (root / "summary.json").exists() else None
manifest = json.loads((root / "manifest.json").read_text())
samples = rows(root / "metrics.ndjson")
operations = rows(root / "operations.ndjson")
latency = rows(root / "latency.ndjson")
failures = rows(root / "failures.ndjson")
begins = {row.get("operation", row.get("step")): row for row in operations if row.get("event") == "begin"}
started = unix_ms(samples[0]["at"]) - samples[0]["elapsedMs"] if samples else None
passed = []
for row in operations:
    if row.get("event") != "pass":
        continue
    begin = begins.get(row.get("operation", row.get("step")))
    if begin is None:
        raise ValueError("Passing operation has no preceding begin evidence")
    effective = {**begin, **row, "elapsedMs": unix_ms(begin["at"]) - started}
    # Native modes 5 and 6 always build a separate 32-record fault fixture.
    # The original begin record names the unrelated selected main catalog.
    if effective.get("mode") in (5, 6):
        effective["rootSize"] = 32
    passed.append(effective)
for row in latency:
    begin = begins.get(row["operation"])
    if begin is None:
        raise ValueError("Latency row has no corresponding operation")
    row["elapsedMs"] = unix_ms(begin["at"]) - started

summary_duration = summary.get("continuousMs", summary.get("elapsedMs")) if summary else None
result = {
    "manifest": manifest, "summary": summary,
    "finished": summary is not None,
    "durationComplete": bool(summary and summary.get("complete")),
    "metricSamples": len(samples),
    "observedMetricDurationMs": samples[-1]["elapsedMs"] if samples else 0,
    "summaryDurationMs": summary_duration,
    "sampleIntervalMs": stats(b["elapsedMs"] - a["elapsedMs"] for a, b in zip(samples, samples[1:])),
    "operationBegins": len(begins), "operationPasses": len(passed),
    "operationFailures": len(failures) + sum(row.get("event") == "fail" or str(row.get("event", "")).startswith("FAIL") for row in operations),
    "clockTicksPerSecond": args.clock_ticks, "logicalCpus": args.logical_cpus, "cpuQuotaCores": args.cpu_quota_cores,
    "windows": [], "warnings": warnings,
    "limits": [
        "A completed duration is not a claim of passed coverage; inspect failures, exclusions and summary.",
        "IPC error counts include deliberate faults and cancellations, not a product defect rate.",
        "IPC counts describe the synthetic boundary, not hidden native worker queues.",
        "Summed RSS can double-count shared pages; process CPU uses supplied measured-host CLK_TCK.",
        "CPU observations omit processes that start and exit between samples; native CPU is parent-only.",
        "Renderer TaskDuration is main-thread work, not total Chromium CPU.",
        "No same-process continuity may be inferred across output directories.",
        "Warm windows compare one fixed catalog size; random scenario proportions can still differ.",
        "Timing is a synthetic diagnostic and is not actual source/network or Windows startup latency.",
        "Native fault modes 5 and 6 use fresh 32-record fixtures; selected main-catalog rootSize is corrected for grouping only.",
    ],
}
if samples and "pid" in samples[0]:
    result["observedNativePids"] = sorted({row["pid"] for row in samples})
if samples and "processes" in samples[0]:
    groups = [{p["pid"] for p in row.get("processes") or []} for row in samples]
    result["pidsPresentInEverySample"] = sorted(set.intersection(*groups))
for start, end in [(0, 15), (30, 60), (90, 120), (150, 180), (210, 240)]:
    in_window = lambda row: start * 60000 <= row["elapsedMs"] < end * 60000
    window = [row for row in samples if in_window(row)]
    window_ops = [row for row in passed if in_window(row)]
    measured = {
        "fromMinute": start, "toMinute": end, "samples": len(window),
        "windowFullyObserved": bool(samples and samples[-1]["elapsedMs"] >= end * 60000),
        "workloadReachedWindowEnd": (summary_duration or (samples[-1]["elapsedMs"] if samples else 0)) >= end * 60000,
        "operationPasses": len(window_ops), **measured_resources(window, args.clock_ticks, args.logical_cpus, args.cpu_quota_cores),
        "operationDurationMsByScenarioAndSize": {},
    }
    groups = sorted({f"{row.get('scenario', row.get('mode'))}:{row.get('rootSize', 'not-recorded')}" for row in window_ops})
    for group in groups:
        measured["operationDurationMsByScenarioAndSize"][group] = stats(
            row["ms"] for row in window_ops
            if f"{row.get('scenario', row.get('mode'))}:{row.get('rootSize', 'not-recorded')}" == group)
    timed = [row for row in latency if in_window(row)]
    measured["authorLatencyMs"] = {key: stats(row[key] for row in timed) for key in ["firstMs", "completeMs"]}
    result["windows"].append(measured)
result["latencyMs"] = {key: stats(row[key] for row in latency) for key in ["firstMs", "completeMs"]}
print(json.dumps(result, indent=2, ensure_ascii=False))
