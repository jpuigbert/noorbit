#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# NoOrbit · spot de 30 s — codificació
#
#   build/encode.sh <framesDir> <outFile>
#
# Afegeix el graó de pel·lícula en codificació (i no en el navegador: així
# el renderitzament és 6 vegades més ràpid i el graó no pesa en els PNG).
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

ROOT="/Users/appleone/programes/noorbit/promo"
FF="$ROOT/node_modules/ffmpeg-static/ffmpeg"
FRAMES="${1:-$ROOT/frames}"
OUT="${2:-$ROOT/video/noorbit-30s-16x9.mp4}"
AUDIO="$ROOT/build/audio/mix.wav"

mkdir -p "$(dirname "$OUT")"

FPS=30
CRF="${CRF:-21}"
GRAIN="${GRAIN:-4}"

# comptem els fotogrames per fixar la durada exacte
N=$(ls "$FRAMES"/f*.png 2>/dev/null | wc -l | tr -d ' ')
echo "· $N fotogrames a $FPS fps → $(echo "scale=2; $N/$FPS" | bc) s"

"$FF" -y -hide_banner -loglevel warning \
  -framerate $FPS -i "$FRAMES/f%05d.png" \
  -i "$AUDIO" \
  -vf "noise=alls=${GRAIN}:allf=t+u,eq=contrast=1.03:saturation=1.05,format=yuv420p" \
  -c:v libx264 -preset slow -crf $CRF -profile:v high -level 4.1 \
  -g 60 -keyint_min 30 -sc_threshold 0 \
  -c:a aac -b:a 160k -ar 48000 -ac 2 \
  -t $(echo "scale=4; $N/$FPS" | bc) \
  -movflags +faststart \
  "$OUT"

echo "→ $OUT"
"$FF" -hide_banner -i "$OUT" 2>&1 | grep -E "Duration|Stream"
