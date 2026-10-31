#!/usr/bin/env python3
"""Native GPU regression probe for temperature-driven terrain snow lines."""
import json
from pathlib import Path
import subprocess
import tempfile

REPO = Path(__file__).resolve().parents[2]
subprocess.run(["cargo", "build", "--locked", "-p", "planet-bevy"], cwd=REPO, check=True)
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--offline", "--no-deps", "--format-version", "1"], cwd=REPO
))
deps = Path(metadata["target_directory"]) / "debug" / "deps"


def library(name):
    candidates = list(deps.glob(f"lib{name}-*.rlib"))
    if not candidates:
        raise RuntimeError(f"Missing built dependency: {name}")
    return max(candidates, key=lambda path: path.stat().st_mtime_ns)


with tempfile.TemporaryDirectory(prefix="planet-snow-") as directory:
    executable = Path(directory) / "probe"
    subprocess.run([
        "rustc", "--edition=2024", str(Path(__file__).with_name("snow_probe.rs")),
        "-L", f"dependency={deps}", "--extern", f"wgpu={library('wgpu')}",
        "--extern", f"bevy_tasks={library('bevy_tasks')}",
        "--extern", f"half={library('half')}", "-o", str(executable),
    ], cwd=REPO, check=True)
    subprocess.run([
        str(executable), str(REPO / "planet-bevy/src/weather/material.wgsl")
    ], cwd=REPO, check=True)
