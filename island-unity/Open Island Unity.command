#!/bin/zsh
set -eu

project_dir=${0:A:h}

if pgrep -f '/Unity.app/Contents/MacOS/Unity' >/dev/null 2>&1; then
    print 'A Unity Editor is already running. Close it before using this launcher.'
    read -k 1 '?Press any key to close...'
    exit 1
fi

# Launch the registered project through Unity Hub so the Editor receives the
# signed-in Unity account token required by Package Manager's My Assets view.
# Opening the editor executable directly provides a licence but not that token.
open -a 'Unity Hub' "$project_dir"
