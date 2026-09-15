#!/usr/bin/env python3
"""Record provenance for an already built macOS Motu artifact."""
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
plugin = Path(sys.argv[1])
source = hashlib.sha256()
inputs = list((ROOT / 'island-rs/src').rglob('*')) + [ROOT / 'island-rs/build.rs', ROOT / 'island-rs/Cargo.toml']
for path in sorted(inputs):
    if path.is_file():
        source.update(str(path.relative_to(ROOT)).encode())
        source.update(path.read_bytes())
data = {
    'abi_version': 1, 'target': 'aarch64-apple-darwin', 'library': plugin.name,
    'sha256': hashlib.sha256(plugin.read_bytes()).hexdigest(),
    'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(),
    'source_tree_sha256': source.hexdigest(),
    'cargo_lock_sha256': hashlib.sha256((ROOT / 'island-rs/Cargo.lock').read_bytes()).hexdigest(),
    'build': 'cargo build --release --lib --locked --no-default-features',
    'supported_player_architecture': 'Apple Silicon', 'render_pipeline': 'Built-in',
    'unity_version': '6000.5.6f1'
}
plugin.with_name('native-artifact.json').write_text(json.dumps(data, indent=2) + '\n')
