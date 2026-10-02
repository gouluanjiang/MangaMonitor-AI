#!/usr/bin/env python3
"""Summarize flushed synthetic soak evidence; never turns incomplete runs green."""
import argparse
import json
import statistics
from pathlib import Path


def rows(path):
    if not path.exists():
        return []
    result = []
    for line in path.read_text().splitlines():
        try:
            result.append(json.loads(line))
        except json.JSONDecodeError:
            # A killed writer may leave one partial final line.
            break
    return result


def stats(values):
    if not values:
        return None
    values = sorted(values)
    return {
        "samples": len(values),
        "min": values[0],
        "median": statistics.median(values),
        "p95": values[min(len(values) - 1, int(len(values) * .95))],
        "max": values[-1],
    }


parser = argparse.ArgumentParser()
parser.add_argument("directory", type=Path)
args = parser.parse_args()
root = args.directory
summary = json.loads((root / "summary.json").read_text()) if (root / "summary.json").exists() else None
samples = rows(root / "metrics.ndjson")
operations = rows(root / "operations.ndjson")
latency = rows(root / "latency.ndjson")
result = {
    "summary": summary,
    "finished": summary is not None,
    "metricSamples": len(samples),
    "operationBegins": sum(row.get("event") == "begin" for row in operations),
    "operationPasses": sum(row.get("event") == "pass" for row in operations),
    "windows": [],
    "limits": [
        "IPC counts describe the synthetic boundary, not internal native worker queues.",
        "RSS sums can double-count shared pages; /proc CPU ticks require CLK_TCK.",
        "No same-process continuity may be inferred across output directories.",
        "Timing distributions include the workload actually recorded; compare equal size/phase.",
    ],
}
for start, end in [(0, 15), (30, 60), (90, 120), (150, 180), (210, 240)]:
    window = [row for row in samples if start * 60000 <= row["elapsedMs"] < end * 60000]
    metrics = {"fromMinute": start, "toMinute": end, "samples": len(window)}
    if window and "metrics" in window[0]:
        for name in ["JSHeapUsedSize", "Nodes", "JSEventListeners", "Documents"]:
            metrics[name] = stats([row["metrics"][name] for row in window])
        metrics["summedRssKb"] = stats([
            sum(process["rssKb"] for process in row["processes"])
            for row in window if isinstance(row.get("processes"), list)
        ])
        metrics["activeIPC"] = stats([row["state"]["active"] for row in window])
        metrics["rendererCpuPercentOneCore"] = stats([
            (b["metrics"]["TaskDuration"] - a["metrics"]["TaskDuration"]) /
            ((b["elapsedMs"] - a["elapsedMs"]) / 1000) * 100
            for a, b in zip(window, window[1:])
        ])
    elif window:
        metrics["operations"] = window[-1]["operations"] - window[0]["operations"]
    result["windows"].append(metrics)
result["latency"] = {
    name: stats([row[name] for row in latency])
    for name in ["firstMs", "completeMs"]
}
print(json.dumps(result, indent=2, ensure_ascii=False))
