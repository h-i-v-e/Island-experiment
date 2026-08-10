#!/bin/zsh
set -eu

project_dir=${0:A:h}
editor_path=/Applications/Unity/Hub/Editor/6000.5.6f1/Unity.app/Contents/MacOS/Unity

if pgrep -f '/Unity.app/Contents/MacOS/Unity' >/dev/null 2>&1; then
    print 'A Unity Editor is already running. Close it before using this launcher.'
    read -k 1 '?Press any key to close...'
    exit 1
fi

# Hub 3.20 ships Licensing Client 1.17.4, while this editor uses protocol
# 1.18.1. Stop stale clients so the editor can launch its matching bundled
# client on a clean version-specific channel.
osascript -e 'tell application "Unity Hub" to quit' >/dev/null 2>&1 || true
sleep 1
pkill -f '/UnityLicensingClient_V1.app/Contents/MacOS/Unity.Licensing.Client' 2>/dev/null || true
pkill -f '/6000.5.6f1/Unity.app/Contents/Helpers/UnityLicensingClient.app' 2>/dev/null || true

exec "$editor_path" -projectPath "$project_dir"
