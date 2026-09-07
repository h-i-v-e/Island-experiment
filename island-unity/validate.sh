#!/usr/bin/env bash
# Validate an isolated copy; never imports into the working project's Library.
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
unity_version="$(sed -n 's/^m_EditorVersion: //p' "$project_root/ProjectSettings/ProjectVersion.txt")"
unity_editor="${UNITY_EDITOR_EXE:-/Applications/Unity/Hub/Editor/$unity_version/Unity.app/Contents/MacOS/Unity}"
if [[ ! -x "$unity_editor" ]]; then
    echo "Set UNITY_EDITOR_EXE to the Unity $unity_version executable." >&2
    exit 1
fi
if [[ $# -gt 1 || (${1:-} != "" && ${1:-} != "--native-exports") ]]; then
    echo "Usage: $0 [--native-exports]" >&2
    exit 1
fi
validation_root="$(mktemp -d "${TMPDIR:-/tmp}/motu-unity-validation.XXXXXX")"
rsync -a --exclude '_Recovery' --exclude '_Recovery.meta' \
    "$project_root/Assets" "$project_root/Packages" "$project_root/ProjectSettings" "$validation_root/"
echo "Validation project and retained logs: $validation_root"
"$unity_editor" -batchmode -force-metal -projectPath "$validation_root" \
    -runTests -testPlatform EditMode -testFilter Motu.Editor.RuntimeContractsTests \
    -testResults "$validation_root/tests.xml" -logFile "$validation_root/tests.log"
"$unity_editor" -batchmode -force-metal -projectPath "$validation_root" \
    -executeMethod Motu.Editor.UnityProjectBuild.BuildPlayer \
    -logFile "$validation_root/player-build.log" -quit
if [[ ${1:-} == "--native-exports" ]]; then
    "$unity_editor" -batchmode -force-metal -projectPath "$validation_root" \
        -executeMethod Motu.Editor.RiverGeometryValidation.Run \
        -logFile "$validation_root/river-fixture.log" -quit
    "$unity_editor" -batchmode -force-metal -projectPath "$validation_root" \
        -executeMethod Motu.Editor.UnityValidation.NativeExports \
        -logFile "$validation_root/native-exports.log" -quit
fi
echo "Unity validation and player build passed: $validation_root"
