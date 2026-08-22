#!/bin/sh

set -eu

crate_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
unity_dir="$crate_dir/../island-unity"
plugin_dir="$unity_dir/Assets/Plugins/macOS"

if [ "$(uname -s)" != "Darwin" ]; then
    echo "error: this script currently deploys only the macOS Unity plugin" >&2
    exit 1
fi

if [ ! -f "$unity_dir/ProjectSettings/ProjectVersion.txt" ]; then
    echo "error: Unity project not found at $unity_dir" >&2
    exit 1
fi

cd "$crate_dir"
cargo build --release --lib

target_dir=${CARGO_TARGET_DIR:-target}
case "$target_dir" in
    /*) ;;
    *) target_dir="$crate_dir/$target_dir" ;;
esac

library="$target_dir/release/libmotu.dylib"
destination="$plugin_dir/libmotu.dylib"

if [ ! -f "$library" ]; then
    echo "error: built library not found at $library" >&2
    exit 1
fi

mkdir -p "$plugin_dir"
install -m 755 "$library" "$destination"

if ! cmp -s "$library" "$destination"; then
    echo "error: deployed library does not match the build artifact" >&2
    exit 1
fi

checksum=$(shasum -a 256 "$destination" | awk '{print $1}')
echo "Deployed $destination"
echo "SHA-256: $checksum"
echo "Restart Unity to load the new native library."
