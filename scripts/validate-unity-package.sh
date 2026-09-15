#!/usr/bin/env bash
# Fresh project with packed packages, no development Assets/ProjectSettings/Library.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
unity_version="$(sed -n 's/^m_EditorVersion: //p' "$repo_root/island-unity/ProjectSettings/ProjectVersion.txt")"
unity_editor="${UNITY_EDITOR_EXE:-/Applications/Unity/Hub/Editor/$unity_version/Unity.app/Contents/MacOS/Unity}"
validation_root="$(mktemp -d "${TMPDIR:-/tmp}/motu-consumer.XXXXXX")"
python3 "$repo_root/scripts/pack-unity.py" "$validation_root/artifacts"
python3 - "$validation_root" "$repo_root" <<'PY'
import json, os, shutil, sys, tarfile
from pathlib import Path
root, repo = map(Path, sys.argv[1:])
project = root / 'project'
(project / 'Packages').mkdir(parents=True)
(project / 'ProjectSettings').mkdir()
shutil.copyfile(repo / 'island-unity/ProjectSettings/ProjectVersion.txt', project / 'ProjectSettings/ProjectVersion.txt')
shutil.copytree(repo / 'scripts/unity-consumer/Assets', project / 'Assets')
artifact = next((root / 'artifacts').glob('com.motu.runtime-*.tgz'))
if os.environ.get('MOTU_IMPORT_SAMPLE') == '1':
    prefix = 'package/Samples~/SingleIsland/'
    with tarfile.open(artifact) as archive:
        for member in archive.getmembers():
            if not member.isfile() or not member.name.startswith(prefix): continue
            relative = Path(member.name[len(prefix):])
            if '..' in relative.parts: raise ValueError('Unsafe sample archive path')
            destination = project / 'Assets/ImportedExample' / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(archive.extractfile(member).read())
manifest = {'dependencies': {'com.motu.runtime': 'file:' + str(artifact), 'com.unity.test-framework': '1.7.0'},
            'testables': ['com.motu.runtime']}
if os.environ.get('MOTU_WITH_NAVIGATION') == '1':
    navigation = next((root / 'artifacts').glob('com.motu.navigation-*.tgz'))
    manifest['dependencies']['com.motu.navigation'] = 'file:' + str(navigation)
(project / 'Packages/manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
PY
echo "Consumer artifacts and logs: $validation_root"
"$unity_editor" -batchmode -force-metal -projectPath "$validation_root/project" \
    -runTests -testPlatform EditMode -testFilter Motu.Tests \
    -testResults "$validation_root/tests.xml" -logFile "$validation_root/tests.log"
"$unity_editor" -batchmode -force-metal -projectPath "$validation_root/project" \
    -executeMethod BuildConsumer.Run -quit -logFile "$validation_root/build.log"
(
    cd "$validation_root/project"
    MOTU_CAPTURE_PATH="$validation_root/consumer.png" "$validation_root/project/Build/MotuConsumer.app/Contents/MacOS/MotuConsumer" \
        -batchmode -force-metal -logFile "$validation_root/player.log"
)
echo "Consumer package tests, build, and player passed: $validation_root"
