#!/bin/sh
set -eu

crate_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_dir=$(dirname -- "$crate_dir")
dist_dir="$crate_dir/dist"
bundle_name="Procedural Material Studio.app"
bundle_path="$dist_dir/$bundle_name"
profile=release
build=true

for option in "$@"; do
    case "$option" in
        --debug) profile=debug ;;
        --no-build) build=false ;;
        *) echo "Usage: $0 [--debug] [--no-build]" >&2; exit 2 ;;
    esac
done

if [ "$(uname -s)" != "Darwin" ]; then
    echo "package-macos.sh requires macOS" >&2
    exit 1
fi
if [ -e "$bundle_path" ]; then
    echo "$bundle_path already exists; move it aside before packaging again" >&2
    exit 1
fi

task_target_dir=${CARGO_TARGET_DIR:-"$repository_dir/target"}
case "$task_target_dir" in
    /*) ;;
    *) task_target_dir="$PWD/$task_target_dir" ;;
esac
if [ "$build" = true ]; then
    set -- --manifest-path "$crate_dir/Cargo.toml"
    if [ "$profile" = release ]; then set -- "$@" --release; fi
    CARGO_TARGET_DIR="$task_target_dir" cargo build "$@"
fi
binary="$task_target_dir/$profile/island-material-studio"
if [ ! -x "$binary" ]; then
    echo "Missing executable: $binary" >&2
    exit 1
fi
mkdir -p "$dist_dir"
stage_dir=$(mktemp -d "$dist_dir/.studio-package.XXXXXX")
trap 'rm -rf "$stage_dir"' EXIT HUP INT TERM
stage_bundle="$stage_dir/$bundle_name"
mkdir -p "$stage_bundle/Contents/MacOS" "$stage_bundle/Contents/Resources/recipes"
install -m 755 \
    "$binary" \
    "$stage_bundle/Contents/MacOS/island-material-studio"
cp "$crate_dir/../island-rs/texture-recipes/"*.json "$stage_bundle/Contents/Resources/recipes/"
cp "$crate_dir/README.md" "$stage_bundle/Contents/Resources/README.md"

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

codesign --force --sign - "$stage_bundle"
codesign --verify --deep --strict "$stage_bundle"
if [ -e "$bundle_path" ]; then
    echo "$bundle_path appeared during packaging; nothing was replaced" >&2
    exit 1
fi
mv "$stage_bundle" "$bundle_path"
rmdir "$stage_dir"
trap - EXIT HUP INT TERM
echo "Packaged $bundle_path ($profile)"
echo "Repository: $repository_dir"
