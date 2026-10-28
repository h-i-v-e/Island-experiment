#!/usr/bin/env python3
"""Run shipping spherical-weather WGSL helpers on the native GPU."""
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


with tempfile.TemporaryDirectory(prefix="planet-environment-") as directory:
    executable = Path(directory) / "probe"
    subprocess.run([
        "rustc", "--edition=2024", str(Path(__file__).with_name("environment_probe.rs")),
        "-L", f"dependency={deps}", "--extern", f"wgpu={library('wgpu')}",
        "--extern", f"bevy_tasks={library('bevy_tasks')}", "-o", str(executable),
    ], cwd=REPO, check=True)
    subprocess.run([
        str(executable), str(REPO / "planet-bevy/src/environment/common.wgsl")
    ], cwd=REPO, check=True)
