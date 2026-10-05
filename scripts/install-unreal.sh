#!/usr/bin/env bash
# ==============================================================================
# NoOrbit — install-unreal.sh
# Prepara el entorn perquè NoOrbit pugui controlar Unreal Engine 5.
#
# Què fa?
#   1. Detecta instal·lacions d'Unreal Engine 5 (Epic Games).
#   2. Si no en troba cap, obre la pàgina de descàrrega i dona instruccions.
#   3. Copia unreal-scripts/bootstrap.py allà on l'app el busca
#      (costat del binari de desenvolupament).
#   4. Comprova si el servidor Remote Control (port 30010) ja respon.
#
# Ús:  bash scripts/install-unreal.sh
# ==============================================================================
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOOTSTRAP_SRC="$ROOT_DIR/unreal-scripts/bootstrap.py"
RC_PORT=30010

say()  { printf '\033[1;36m[noorbit]\033[0m %s\n' "$*"; }
ok()   { printf '\033[1;32m✓\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31m✗\033[0m %s\n' "$*"; }

# ------------------------------------------------------------------------------
# 1. Detecció de l'engine (mateixos rutes que usa src-tauri/src/unreal/launcher.rs)
# ------------------------------------------------------------------------------
say "Detectant instal·lacions d'Unreal Engine 5…"
ENGINES=()

detect_in() {
  local base="$1"
  [[ -d "$base" ]] || return 0
  while IFS= read -r d; do
    [[ "$(basename "$d")" == UE_* ]] && ENGINES+=("$d")
  done < <(find "$base" -maxdepth 1 -type d -name 'UE_*' 2>/dev/null | sort -r)
}

case "$(uname -s)" in
  Darwin)
    detect_in "/Users/Shared/Epic Games"
    detect_in "/Applications/Epic Games"
    ;;
  Linux)
    detect_in "$HOME/UnrealEngine"
    detect_in "/opt/UnrealEngine"
    ;;
  CYGWIN*|MINGW*|MSYS*)
    detect_in "/c/Program Files/Epic Games"
    ;;
esac

if [[ ${#ENGINES[@]} -gt 0 ]]; then
  for e in "${ENGINES[@]}"; do ok "UE trobat: $e"; done
else
  warn "No s'ha trobat cap instal·lació d'Unreal Engine 5."
  cat <<'EOF'
   Per instal·lar-lo:
     1. Instal·la l'Epic Games Launcher: https://store.epicgames.com/download
     2. Obre'l > Pestanya "Unreal Engine" > "Instal·lacions" >tria UE 5.x > Instal·la.
     3. (Opcional, codi font) git clone --branch 5.5 https://github.com/EpicGames/UnrealEngine.git
        i executa Setup.sh + ./GenerateProjectFiles.sh + cmake/build.
EOF
  if command -v open >/dev/null 2>&1; then
    say "Obrint la pàgina de descàrrega…"
    open "https://www.unrealengine.com/download" || true
  fi
fi

# ------------------------------------------------------------------------------
# 2. Bootstrap de Remote Control: copiar-lo costat dels binaris de desenvolupament
# ------------------------------------------------------------------------------
if [[ -f "$BOOTSTRAP_SRC" ]]; then
  for target_dir in "$ROOT_DIR/apps/desktop/src-tauri/target/debug" \
                    "$ROOT_DIR/apps/desktop/src-tauri/target/release"; do
    if [[ -d "$target_dir" ]]; then
      mkdir -p "$target_dir/unreal-scripts"
      cp "$BOOTSTRAP_SRC" "$target_dir/unreal-scripts/bootstrap.py"
      ok "bootstrap.py copiat a $target_dir/unreal-scripts/"
    fi
  done
  say "En producció, bootstrap.py ja s'inclou al bundle (.app/Contents/Resources) via tauri.conf.json."
else
  fail "No trobo $BOOTSTRAP_SRC"
fi

# ------------------------------------------------------------------------------
# 3. Estat del servidor Remote Control
# ------------------------------------------------------------------------------
if command -v nc >/dev/null 2>&1 && nc -z 127.0.0.1 "$RC_PORT" 2>/dev/null; then
  ok "Remote Control API escoltant al port $RC_PORT (Unreal obert i connectat)."
else
  warn "El port $RC_PORT no respon. Quan obres l'editor des de NoOrbit, s'habilita sol;"
  warn "si l'obres a mà: Window > Virtual Production > Remote Control > Enable Remote Control."
fi

say "Llest. Reinicia NoOrbit si has instal·lat l'engine ara."
