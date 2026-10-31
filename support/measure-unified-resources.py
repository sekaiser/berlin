#!/usr/bin/env python3
"""Run isolated, offline release-mode synthetic publication measurements."""
import argparse
import json
from pathlib import Path
import platform
import subprocess
import sys

REPO = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, default=REPO / 'target/release/examples/unified-scale')
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
workloads = [(n, p, 'cards') for n in (10, 100, 1000, 10000) for p in (10, 100, 1000)]
workloads += [(1000, 1000, 'repeated-resource'), (1000, 1000, 'repeated-excerpt')]
args.output.parent.mkdir(parents=True, exist_ok=True)
with args.output.open('w') as output:
    for count, placements, kind in workloads:
        command = [str(args.binary.resolve()), str(count), str(placements), kind, '3']
        # RSS belongs to this child workload process, not Cargo or this driver.
        timed = ['/usr/bin/time', '-l' if sys.platform == 'darwin' else '-v', *command]
        result = subprocess.run(timed, capture_output=True, text=True, check=True)
        rss = None
        for line in result.stderr.splitlines():
            if 'maximum resident set size' in line:
                rss = int(line.split()[0])  # Darwin reports bytes.
            elif 'Maximum resident set size (kbytes):' in line:
                rss = int(line.rsplit(':', 1)[1]) * 1024
        for line in result.stdout.splitlines():
            row = json.loads(line)
            row['workload_process_peak_rss_bytes'] = rss
            row['platform'] = platform.system() + '-' + platform.machine()
            output.write(json.dumps(row, sort_keys=True) + '\n')
        output.flush()
        print(f'{kind}: catalog={count}, placements={placements}, repetitions=3', flush=True)
