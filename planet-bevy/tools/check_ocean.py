#!/usr/bin/env python3
"""Run the actual ocean WGSL wave and cube-map functions on a native GPU."""
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
    return max(deps.glob(f"lib{name}-*.rlib"), key=lambda path: path.stat().st_mtime_ns)


with tempfile.TemporaryDirectory(prefix="planet-ocean-") as directory:
    executable = Path(directory) / "probe"
    subprocess.run([
        "rustc", "--edition=2024", str(Path(__file__).with_name("ocean_probe.rs")),
        "-L", f"dependency={deps}", "--extern", f"wgpu={library('wgpu')}",
        "--extern", f"bevy_tasks={library('bevy_tasks')}", "-o", str(executable),
    ], cwd=REPO, check=True)
    subprocess.run([
        str(executable), str(REPO / "planet-bevy/src/ocean/waves.wgsl")
    ], cwd=REPO, check=True)
    foam_executable = Path(directory) / "foam_probe"
    subprocess.run([
        "rustc", "--edition=2024", str(Path(__file__).with_name("foam_probe.rs")),
        "-L", f"dependency={deps}", "--extern", f"wgpu={library('wgpu')}",
        "--extern", f"bevy_tasks={library('bevy_tasks')}", "--extern", f"half={library('half')}",
        "-o", str(foam_executable),
    ], cwd=REPO, check=True)
    subprocess.run([
        str(foam_executable), str(REPO / "planet-bevy/src/ocean/waves.wgsl"),
        str(REPO / "planet-bevy/src/ocean/foam.wgsl"),
    ], cwd=REPO, check=True)
