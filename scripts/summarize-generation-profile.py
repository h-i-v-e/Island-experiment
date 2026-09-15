#!/usr/bin/env python3
"""Summarize generation-bench CSV and profiling-feature stderr without warmup."""
import argparse
from collections import defaultdict
import csv
import json
import math
from pathlib import Path
import statistics


def summarize(csv_path, stages_path):
    with csv_path.open(newline='') as source:
        runs = list(csv.DictReader(source))
    if not runs:
        raise ValueError('benchmark CSV contains no measured runs')

    generations = []
    stages = defaultdict(list)
    for line in stages_path.read_text().splitlines():
        if not line.startswith('profile,'):
            continue
        _, name, elapsed = line.split(',')
        milliseconds = float(elapsed)
        if not math.isfinite(milliseconds) or milliseconds < 0:
            raise ValueError(f'invalid stage duration: {line}')
        stages[name].append(milliseconds)
        if name == 'island.generate':
            generations.append(dict(stages))
            stages.clear()
    if stages or len(generations) != len(runs) + 1:
        raise ValueError('expected one warmup and one complete profile per CSV run')

    fixtures = defaultdict(list)
    for run, profile in zip(runs, generations[1:]):
        elapsed = float(run['milliseconds'])
        if not math.isfinite(elapsed) or elapsed <= 0:
            raise ValueError('invalid benchmark duration')
        # The outer timer excludes the benchmark caller, but should describe
        # the same generation; reject accidental pairing with unrelated logs.
        if abs(profile['island.generate'][0] - elapsed) > max(10, elapsed * .01):
            raise ValueError('CSV and stage log generation durations do not match')
        fixtures[(int(run['seed']), int(run['workers']))].append((run, profile))

    results = []
    for (seed, workers), measurements in fixtures.items():
        hashes = {run['geometry_hash'] for run, _ in measurements}
        if len(hashes) != 1:
            raise ValueError(f'geometry changed across runs for seed {seed}')
        names = set().union(*(profile for _, profile in measurements))
        ranked = []
        for name in names - {'island.generate'}:
            totals = [sum(profile.get(name, [])) for _, profile in measurements]
            ranked.append({
                'stage': name,
                'inclusive_ms_per_run': totals,
                'median_inclusive_ms': statistics.median(totals),
                'calls_per_run': [len(profile.get(name, [])) for _, profile in measurements],
            })
        ranked.sort(key=lambda stage: (-stage['median_inclusive_ms'], stage['stage']))
        first = measurements[0][0]
        results.append({
            'seed': seed,
            'workers': workers,
            'runs': len(measurements),
            'milliseconds': [float(run['milliseconds']) for run, _ in measurements],
            'median_ms': statistics.median(float(run['milliseconds']) for run, _ in measurements),
            'vertices': int(first['vertices']),
            'triangles': int(first['triangles']),
            'geometry_hash': first['geometry_hash'],
            'identical_hash_across_runs': True,
            'ranked_stages': ranked,
        })
    return {
        'scope': 'CPU Island::generate only. One initial warmup excluded. Stage times are inclusive and can overlap; do not add parent and child costs.',
        'csv': csv_path.name,
        'stage_log': stages_path.name,
        'fixtures': results,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('csv', type=Path)
    parser.add_argument('stages', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    try:
        report = summarize(args.csv, args.stages)
    except (ValueError, KeyError, OSError) as error:
        parser.exit(1, f'Invalid generation profile: {error}\n')
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(f'Wrote {args.output}: {len(report["fixtures"])} fixture(s)')


if __name__ == '__main__':
    main()
