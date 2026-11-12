#!/bin/zsh
set -e
# Tria el NoOrbit.app amb la versió més alta entre les sorts de bundle
# existents (triple x86_64/aarch64/universal i la nativa sense triple).
#
# ROOT es calcula a partir de la ubicació de l'script: abans estava
# hardcodejat amb la ruta de l'usuari que el va escriure (no funcionava en
# cap altre equip ni si el repo es movia).
ROOT="$(cd "$(dirname "$0")/.." && pwd)/apps/desktop/src-tauri/target"
APP=""
BEST=""
for cand in \
  "$ROOT/x86_64-apple-darwin/release/bundle/macos/NoOrbit.app" \
  "$ROOT/aarch64-apple-darwin/release/bundle/macos/NoOrbit.app" \
  "$ROOT/universal-apple-darwin/release/bundle/macos/NoOrbit.app" \
  "$ROOT/release/bundle/macos/NoOrbit.app"; do
  [ -d "$cand" ] || continue
  V=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$cand/Contents/Info.plist" 2>/dev/null || echo "")
  [ -n "$V" ] || V="0"
  # Comparació amb 'sort -V' (versions): l'antig mètode amb printf
  # '%02d%02d%02d' es trencava amb versions de 4 components o quan
  # PlistBuddy fallava (i podia triar una versió equivocada o cap).
  KEY=$(printf '%s\n' "$V" | sort -V | tail -1)
  if [ -z "$BEST" ] || [[ "$(printf '%s\n%s\n' "$BEST" "$KEY" | sort -V | tail -1)" == "$KEY" ]]; then
    BEST="$KEY"; APP="$cand"
  fi
done
if [ -z "$APP" ]; then
  echo "No s'ha trobat cap NoOrbit.app compilat. Fes primer 'pnpm app:build:mac'." >&2
  exit 1
fi
echo "Instal·lant des de: $APP (v$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist" 2>/dev/null || echo "$BEST"))"
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
