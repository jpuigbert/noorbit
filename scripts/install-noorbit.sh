#!/bin/zsh
set -e
APP="/Users/appleone/programes/noorbit/apps/desktop/src-tauri/target/release/bundle/macos/NoOrbit.app"
osascript -e 'tell application "NoOrbit" to quit' 2>/dev/null || true
pkill -x no-orbit 2>/dev/null || true
if [ -d "/Applications/NoOrbit.app" ]; then
  rm -rf "/Applications/NoOrbit.app"
fi
cp -R "$APP" /Applications/
xattr -dr com.apple.quarantine "/Applications/NoOrbit.app" 2>/dev/null || true
echo "=== Instalada: ==="
ls -d /Applications/NoOrbit.app
du -sh /Applications/NoOrbit.app
