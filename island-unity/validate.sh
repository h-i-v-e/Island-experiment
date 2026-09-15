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
suite=contracts
native_exports=false
while [[ $# -gt 0 ]]; do
    case "$1" in
        --native-exports) native_exports=true; shift ;;
        --suite) suite="${2:?Expected suite name}"; shift 2 ;;
        *) echo "Usage: $0 [--suite contracts|lifecycle|streaming|graphics|navigation|serialization|all] [--native-exports]" >&2; exit 1 ;;
    esac
done
case "$suite" in
    contracts) test_filter=Motu.Editor.RuntimeContractsTests ;;
    lifecycle) test_filter='Motu.Editor.RuntimeContractsTests.GenerationCancellationInstallationAndUnload;Motu.Editor.RuntimeContractsTests.RuntimeOwnership;Motu.Editor.RuntimeContractsTests.CancellationAndRestart' ;;
    streaming) test_filter='Motu.Editor.RuntimeContractsTests.TerrainBatchUsesCombinedMeshIndices;Motu.Editor.BoulderTests;Motu.Editor.FallenLogTests;Motu.Editor.IslandLod3Tests' ;;
    graphics) test_filter='Motu.Editor.Ocean;Motu.Editor.RiverSurfaceTests;Motu.Editor.RuntimeContractsTests.ShoreBreaking;Motu.Editor.RuntimeContractsTests.WaveTransitions;Motu.Editor.RuntimeContractsTests.WeatherAndWaves' ;;
    navigation) test_filter=Motu.Editor.IslandNavigationTests ;;
    serialization) test_filter='Motu.Editor.RuntimeContractsTests.SupportedScenes;Motu.Editor.RuntimeContractsTests.ProfileCopiesPreserveValuesAndAuthoredAssets;Motu.Editor.RuntimeContractsTests.MaterialCacheRoundTrip' ;;
    all) test_filter=Motu ;;
    *) echo "Unknown validation suite: $suite" >&2; exit 1 ;;
esac
validation_root="$(mktemp -d "${TMPDIR:-/tmp}/motu-unity-validation.XXXXXX")"
repository_root="$project_root/.."
mkdir -p "$validation_root/island-unity"
rsync -a --exclude '_Recovery' --exclude '_Recovery.meta' \
    "$project_root/Assets" "$project_root/Packages" "$project_root/ProjectSettings" "$validation_root/island-unity/"
rsync -a "$repository_root/packages" "$validation_root/"
validation_root="$validation_root/island-unity"
echo "Validation project and retained logs: $validation_root"
"$unity_editor" -batchmode -force-metal -projectPath "$validation_root" \
    -runTests -testPlatform EditMode -testFilter "${MOTU_TEST_FILTER:-$test_filter}" \
    -testResults "$validation_root/tests.xml" -logFile "$validation_root/tests.log"
"$unity_editor" -batchmode -force-metal -projectPath "$validation_root" \
    -executeMethod Motu.Editor.UnityProjectBuild.BuildPlayer \
    -logFile "$validation_root/player-build.log" -quit
if $native_exports; then
    "$unity_editor" -batchmode -force-metal -projectPath "$validation_root" \
        -executeMethod Motu.Editor.RiverGeometryValidation.Run \
        -logFile "$validation_root/river-fixture.log" -quit
    "$unity_editor" -batchmode -force-metal -projectPath "$validation_root" \
        -executeMethod Motu.Editor.UnityValidation.NativeExports \
        -logFile "$validation_root/native-exports.log" -quit
fi
echo "Unity validation and player build passed: $validation_root"
