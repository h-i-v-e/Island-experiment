#!/bin/sh
set -eu

crate_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_dir=$(dirname -- "$crate_dir")
dist_dir="$crate_dir/dist"
bundle_name="Procedural Material Studio.app"
bundle_path="$dist_dir/$bundle_name"

if [ "$(uname -s)" != "Darwin" ]; then
    echo "package-macos.sh requires macOS" >&2
    exit 1
fi
if [ -e "$bundle_path" ]; then
    echo "$bundle_path already exists; move it aside before packaging again" >&2
    exit 1
fi

cargo build --release --manifest-path "$crate_dir/Cargo.toml"
mkdir -p "$dist_dir"
stage_dir=$(mktemp -d "$dist_dir/.studio-package.XXXXXX")
trap 'rm -rf "$stage_dir"' EXIT HUP INT TERM
stage_bundle="$stage_dir/$bundle_name"
mkdir -p "$stage_bundle/Contents/MacOS"
install -m 755 \
    "$crate_dir/target/release/island-material-studio" \
    "$stage_bundle/Contents/MacOS/island-material-studio"

plist="$stage_bundle/Contents/Info.plist"
plutil -create xml1 "$plist"
plutil -insert CFBundleDisplayName -string "Procedural Material Studio" "$plist"
plutil -insert CFBundleExecutable -string "island-material-studio" "$plist"
plutil -insert CFBundleIdentifier -string "nz.co.motu.procedural-material-studio" "$plist"
plutil -insert CFBundleInfoDictionaryVersion -string "6.0" "$plist"
plutil -insert CFBundleName -string "Procedural Material Studio" "$plist"
plutil -insert CFBundlePackageType -string "APPL" "$plist"
plutil -insert CFBundleShortVersionString -string "0.1.0" "$plist"
plutil -insert NSHighResolutionCapable -bool true "$plist"

mv "$stage_bundle" "$bundle_path"
rmdir "$stage_dir"
trap - EXIT HUP INT TERM
codesign --force --sign - "$bundle_path"
echo "Packaged $bundle_path"
echo "Repository: $repository_dir"
