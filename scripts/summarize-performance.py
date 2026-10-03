#!/usr/bin/env python3
"""Describe measured synthetic loading conditions without inferring native timing."""
import argparse
import json
import math
from pathlib import Path
import statistics


def rows(path):
    if not path.exists():
        return []
    raw = path.read_bytes()
    result = []
    for index, line in enumerate(raw.splitlines()):
        try:
            result.append(json.loads(line))
        except json.JSONDecodeError:
            if index == len(raw.splitlines()) - 1 and not raw.endswith(b'\n'):
                continue
            raise
    return result


def distribution(values):
    values = sorted(x for x in values if isinstance(x, (int, float)))
    if not values:
        return None
    return {'samples':len(values), 'min':values[0], 'median':statistics.median(values), 'p95':values[max(0,math.ceil(.95*len(values))-1)], 'max':values[-1]}


def usage(samples):
    result = {'samples':len(samples)}
    for name in ['JSHeapUsedSize','Nodes','JSEventListeners','Documents']:
        result[name] = distribution(row['metrics'].get(name) for row in samples)
    result['summedRssKb'] = distribution(sum(p['rssKb'] for p in row['processes']) for row in samples if isinstance(row.get('processes'),list))
    result['runnerRssBytes'] = distribution(row['runnerMemory']['rss'] for row in samples)
    result['activeIPC'] = distribution(row['state']['active'] for row in samples)
    proportional = []
    for row in samples:
        processes = row.get('processes')
        if processes and all(p.get('proportionalMemory') and p['proportionalMemory'].get('PssKb') is not None for p in processes):
            proportional.append(sum(p['proportionalMemory']['PssKb'] for p in processes))
    result['summedPssKb'] = distribution(proportional)
    result['pssAvailableForAllSamples'] = len(proportional) == len(samples) and len(samples)>0
    return result


parser = argparse.ArgumentParser()
parser.add_argument('directory', type=Path)
args = parser.parse_args()
root = args.directory
summary = json.loads((root/'summary.json').read_text()) if (root/'summary.json').exists() else None
latencies = rows(root/'latency.ndjson')
metrics = rows(root/'pressure-metrics.ndjson')
result = {'summary':summary, 'matrix':[], 'pressure':[], 'continuousPressureWindows':[], 'heapDiagnostics':{}, 'limitations':[
    'Five samples per matrix condition are descriptive; p95 equals maximum at this sample count.',
    'Background conditions describe synthetic IPC traffic, not real native download/source throughput.',
    'Cold Chromium trials do not flush host OS caches.',
    'Heap collection happens outside the timed pressure interval; resource samples inside it are not forced-GC values.',
    'Pressure modes rotate in five-minute blocks; compare whole 20-minute cycles after warming.',
    'Missing PSS is unavailable and must not be interpreted as zero unique memory.'
]}
keys=['size','background','scenario','phase']
matrix=[row for row in latencies if isinstance(row.get('round'),int)]
for group in sorted({tuple(row.get(k) for k in keys) for row in matrix}):
    selected=[row for row in matrix if tuple(row.get(k) for k in keys)==group]
    result['matrix'].append({**dict(zip(keys,group)),**{name:distribution(row.get(name) for row in selected) for name in ['firstMs','completeMs','ipcCalls']}})
pressure=[row for row in latencies if row.get('round')=='pressure' and row.get('phase')=='pressure']
for background,scenario in sorted({(row['background'],row['scenario']) for row in pressure}):
    selected=[row for row in pressure if row['background']==background and row['scenario']==scenario]
    result['pressure'].append({'background':background,'scenario':scenario,**{name:distribution(row.get(name) for row in selected) for name in ['firstMs','completeMs','ipcCalls']}})
for start,end in [(0,20),(20,40),(40,60)]:
    selected=[row for row in metrics if start*60000<=row['elapsedMs']<end*60000]
    result['continuousPressureWindows'].append({'fromMinute':start,'toMinute':end,**usage(selected)})
for position in ['before','after']:
    path=root/f'{position}-measurement-heap.json'
    if not path.exists():continue
    data=json.loads(path.read_text());selected={'unavailable':data.get('unavailable'), 'heapSnapshot':data.get('heapSnapshot')}
    for stage in ['beforeGc','afterGc']:
        if stage not in data:continue
        selected[stage]={key:value for key,value in {x['name']:x['value'] for x in data[stage]['performance']['metrics']}.items() if key in ['JSHeapUsedSize','Nodes','JSEventListeners','Documents']}
    result['heapDiagnostics'][position]=selected
print(json.dumps(result,indent=2,ensure_ascii=False))
