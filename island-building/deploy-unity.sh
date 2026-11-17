#!/bin/sh
# Build the engine-neutral town library and install its thin C ABI for Unity.
set -eu
crate_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_dir=$(dirname -- "$crate_dir")
plugin_dir=${MOTU_PLUGIN_DIR:-"$repository_dir/packages/com.motu.runtime/Runtime/Plugins/macOS"}
task_target_dir=${CARGO_TARGET_DIR:-"$repository_dir/target"}
case "$task_target_dir" in /*) ;; *) task_target_dir="$PWD/$task_target_dir" ;; esac
if [ "$(uname -s)" != Darwin ] || [ "$(uname -m)" != arm64 ]; then
    echo "This deployment currently supports macOS Apple Silicon." >&2
    exit 1
fi
CARGO_TARGET_DIR="$task_target_dir" cargo build --manifest-path "$crate_dir/Cargo.toml" --release --lib --locked --no-default-features
library="$task_target_dir/release/libisland_building.dylib"
destination="$plugin_dir/libmotu_building.dylib"
mkdir -p "$plugin_dir"
temporary=$(mktemp "$plugin_dir/.libmotu_building.dylib.XXXXXX")
trap 'rm -f "$temporary"' EXIT HUP INT TERM
install -m 755 "$library" "$temporary"
mv -f "$temporary" "$destination"
trap - EXIT HUP INT TERM
cmp "$library" "$destination"
python3 - "$destination" "$repository_dir" <<'PY'
import hashlib, json, pathlib, subprocess, sys
plugin, root = map(pathlib.Path, sys.argv[1:])
files = sorted((root / "island-building/src").rglob("*.rs"))
files += sorted((root / "island-building/recipes").rglob("*.json"))
files += [root / "island-building/Cargo.toml", root / "island-building/Cargo.lock"]
files += sorted((root / "island-rs/src").rglob("*.rs"))
files += sorted((root / "island-rs/texture-recipes").rglob("*.json"))
files += sorted((root / "island-rs/texture-recipes").rglob("*.json"))
files += [root / "island-rs/Cargo.toml", root / "island-rs/build.rs"]
digest = hashlib.sha256()
for path in files:
    digest.update(str(path.relative_to(root)).encode())
    digest.update(path.read_bytes())
manifest = {"api_version": 1, "sha256": hashlib.sha256(plugin.read_bytes()).hexdigest(),
            "building_source_sha256": digest.hexdigest(),
            "git_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()}
plugin.with_suffix(".dylib.manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
print("Installed town native library:", plugin)
print("SHA-256:", manifest["sha256"])
PY
echo "Restart Unity to load a replaced native library."
