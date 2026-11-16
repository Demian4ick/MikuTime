"""Measure the whole process tree, not just the Rust host. Standard library only."""
import argparse
import json
import os
from pathlib import Path
import statistics
import time

parser = argparse.ArgumentParser()
parser.add_argument('pid', type=int)
parser.add_argument('--seconds', type=int, default=60)
parser.add_argument('--label', default='idle')
parser.add_argument('--warmup', type=int, default=10)
parser.add_argument('--out', type=Path, required=True)
args = parser.parse_args()
hz = os.sysconf('SC_CLK_TCK')

def sample():
    all_processes = {}
    for proc in Path('/proc').iterdir():
        if not proc.name.isdigit():
            continue
        try:
            fields = (proc / 'stat').read_text().rsplit(')', 1)[1].split()
            all_processes[int(proc.name)] = (int(fields[1]), (int(fields[11]) + int(fields[12])) / hz)
        except (OSError, ValueError, IndexError):
            pass
    tree = {args.pid}
    while True:
        children = {pid for pid, (ppid, _) in all_processes.items() if ppid in tree}
        if children.issubset(tree): break
        tree |= children
    rss = pss = 0
    for pid in tree:
        try:
            status = Path(f'/proc/{pid}/status').read_text()
            rss += next(int(line.split()[1]) for line in status.splitlines() if line.startswith('VmRSS:'))
            try:
                pss += next(int(line.split()[1]) for line in Path(f'/proc/{pid}/smaps_rollup').read_text().splitlines() if line.startswith('Pss:'))
            except (OSError, StopIteration): pass
        except (OSError, StopIteration): pass
    return {'rssMiB': rss / 1024, 'pssMiB': pss / 1024,
            'ticks': {pid: all_processes[pid][1] for pid in tree if pid in all_processes}, 'processes': len(tree)}

time.sleep(args.warmup)
first = sample()
previous = first['ticks']
rows = []
start = time.monotonic()
last = start
for _ in range(args.seconds):
    time.sleep(1)
    current = sample()
    if args.pid not in current['ticks']: raise SystemExit('Root process exited; measurement aborted')
    now = time.monotonic()
    cpu = sum(max(0, value - previous.get(pid, value)) for pid, value in current['ticks'].items()) / (now - last) * 100
    rows.append({'elapsed': now - start, 'cpuPercentOneCore': cpu, **{k:v for k,v in current.items() if k != 'ticks'}})
    previous = current['ticks']; last = now
result = {'label': args.label, 'pid': args.pid, 'seconds': time.monotonic() - start,
          'cpuMeanPercentOneCore': statistics.mean(r['cpuPercentOneCore'] for r in rows),
          'rssMeanMiB': statistics.mean(r['rssMiB'] for r in rows), 'rssPeakMiB': max(r['rssMiB'] for r in rows),
          'pssMeanMiB': statistics.mean(r['pssMiB'] for r in rows), 'samples': rows}
args.out.parent.mkdir(parents=True, exist_ok=True)
args.out.write_text(json.dumps(result, indent=2))
print(json.dumps({k:v for k,v in result.items() if k != 'samples'}, indent=2))
