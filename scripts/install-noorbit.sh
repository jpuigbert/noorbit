#!/bin/zsh
set -e
# Tria el NoOrbit.app amb la versió més alta entre les sorts de bundle
# existents (triple x86_64/aarch64/universal i la nativa sense triple).
ROOT="/Users/appleone/programes/noorbit/apps/desktop/src-tauri/target"
APP=""
BEST=0
for cand in \
  "$ROOT/x86_64-apple-darwin/release/bundle/macos/NoOrbit.app" \
  "$ROOT/aarch64-apple-darwin/release/bundle/macos/NoOrbit.app" \
  "$ROOT/universal-apple-darwin/release/bundle/macos/NoOrbit.app" \
  "$ROOT/release/bundle/macos/NoOrbit.app"; do
  [ -d "$cand" ] || continue
  V=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$cand/Contents/Info.plist" 2>/dev/null || echo 0)
  VN=$(printf '%02d%02d%02d' $(echo "$V" | tr '.' ' ') 2>/dev/null || echo 0)
  if [ "$VN" -gt "$BEST" ]; then BEST="$VN"; APP="$cand"; fi
done
if [ -z "$APP" ]; then
  echo "No s'ha trobat cap NoOrbit.app compilat. Fes primer 'pnpm app:build:mac'." >&2
  exit 1
fi
echo "Instal·lant des de: $APP (v$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist"))"
osascript -e 'tell application "NoOrbit" to quit' 2>/dev/null || true
pkill -x no-orbit 2>/dev/null || true
if [ -d "/Applications/NoOrbit.app" ]; then
  rm -rf "/Applications/NoOrbit.app"
fi
cp -R "$APP" /Applications/
xattr -dr com.apple.quarantine "/Applications/NoOrbit.app" 2>/dev/null || true
echo "=== Instal·lada: ==="
ls -d /Applications/NoOrbit.app
du -sh /Applications/NoOrbit.app
