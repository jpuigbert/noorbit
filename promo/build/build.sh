#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# NoOrbit · spot de 30 s — pipeline complet
#
#   ./promo/build/build.sh          → genera els dos formats (16:9 i 9:16)
#   ./promo/build/build.sh wide     → només el 16:9
#   ./promo/build/build.sh v        → només el 9:16
#
# Dependències: node (playwright), python3, ffmpeg (via ffmpeg-static).
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

WHAT="${1:-all}"
FF="$ROOT/node_modules/ffmpeg-static/ffmpeg"

echo "▸ 1/4  veu (TTS)"
node build/make-vo.mjs

echo "▸ 2/4  música + mescla"
python3 build/audio.py

render_and_encode () {
  local fmt="$1" out="$2"
  echo "▸ 3/4  fotogrames ($fmt)"
  rm -rf "frames-$fmt"
  node build/render.mjs full "frames-$fmt" "$fmt"
  echo "▸ 4/4  codificació ($fmt)"
  bash build/encode.sh "frames-$fmt" "$out"
}

if [ "$WHAT" = "all" ] || [ "$WHAT" = "wide" ]; then
  render_and_encode wide "$ROOT/video/noorbit-spot-16x9.mp4"
fi
if [ "$WHAT" = "all" ] || [ "$WHAT" = "v" ]; then
  render_and_encode v "$ROOT/video/noorbit-spot-9x16.mp4"
fi

echo "✓ Fet. Els vídeos són a $ROOT/video/"
