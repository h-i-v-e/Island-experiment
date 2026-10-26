"""Independent acceptance check; requires NumPy/SciPy only for this script.

Run after cargo build -p planet-bevy:
    python compare_qhull.py --viewer target/debug/planet-bevy
"""

import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import time

import numpy as np
from scipy.spatial import ConvexHull


def read_obj(path):
    points, faces = [], set()
    for line in path.read_text().splitlines():
        fields = line.split()
        if fields and fields[0] == "v":
            points.append([float(value) for value in fields[1:]])
        elif fields and fields[0] == "f":
            faces.add(tuple(sorted(int(value) - 1 for value in fields[1:])))
    return np.asarray(points), faces


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--viewer", type=Path, required=True)
    args = parser.parse_args()
    viewer = args.viewer.resolve()
    results = []
    with tempfile.TemporaryDirectory(prefix="planet-qhull-") as scratch:
        output = Path(scratch) / "planet.obj"
        for count in [32, 256, 1024]:
            for seed in [0, 42, 123]:
                for axis in ["x", "y", "z"]:
                    results.append(check(viewer, output, count, seed, axis))
        # Exercise the supported upper bound and physical-radius export too.
        results.append(check(viewer, output, 8192, 42, "y", radius=6371000))
    print(json.dumps({"matched_cases": len(results), "cases": results}, indent=2))


def check(viewer, output, count, seed, axis, radius=1):
    started = time.perf_counter()
    run = subprocess.run(
        [str(viewer), "--sphere-only", "--validate-only", "--vertices", str(count),
         "--seed", str(seed), "--sweep-axis", axis, "--radius", str(radius),
         "--export", str(output)], capture_output=True, text=True, check=True,
    )
    elapsed = time.perf_counter() - started
    points, actual = read_obj(output)
    assert points.shape == (count, 3), run.stdout
    assert np.allclose(np.linalg.norm(points, axis=1), radius, rtol=1e-13)
    hull = ConvexHull(points)
    expected = {tuple(sorted(face)) for face in hull.simplices}
    assert actual == expected, (
        count, seed, axis, "missing", len(expected - actual),
        "extra", len(actual - expected), run.stdout,
    )
    assert len(hull.vertices) == count
    return {"vertices": count, "seed": seed, "axis": axis,
            "radius": radius, "triangles": len(actual), "seconds": round(elapsed, 4)}


if __name__ == "__main__":
    main()
