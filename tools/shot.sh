#!/usr/bin/env bash
# Opens inputsmith-gui on a page and saves a window screenshot.
# usage: tools/shot.sh <page> <out.png> [slot]
set -e
taskkill //F //IM inputsmith-gui.exe >/dev/null 2>&1 && sleep 1 || true  # let it release the single-instance lock
INPUTSMITH_START_PAGE="$1" INPUTSMITH_START_SLOT="$3" "${SHOT_BIN:-./target/debug}/inputsmith-gui" >/dev/null 2>&1 &
sleep "${SHOT_DELAY:-5}"
powershell -NoProfile -ExecutionPolicy Bypass -File tools/capture-window.ps1 inputsmith-gui "$2"
