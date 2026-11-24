#!/usr/bin/env bash
# ==============================================================================
# NoOrbit — install-android-sdk.sh
# Baixa l'Android SDK DE COMPILACIÓ (cmdline-tools + platform-tools +
# platforms + build-tools) a la carpeta que TRIA L'USUARI.
#
# Decisions de disseny (importants, no són un caprici):
#   * Cap ruta fixa: «--dest» és obligatori. Pot ser un disc extern (avui un
#     volum anomenat com l'usuari vulga, demà un altre). NoOrbit no ha de
#     conèixer cap nom de volum: només rep un camí.
#   * NO es baixa l'«emulator» ni les «system images»: pesen diversos GB i només
#     fan falta per arrencar un AVD. Qui les vol, passa «--with-emulator».
#   * El número de build dels cmdline-tools es DESCOBREIX del repositori
#     oficial: macOS va per darrere de linux/win (builds diferents), així que
#     hardcodejar un número únic trencaria la descàrrega en un dels tres SO.
#   * «sdkmanager» necessita un JDK 17 (Java 21+ encara el pot trencar per
#     accessos il·legals). Si no n'hi ha cap, s'avisa i es marxa — mai no
#     instal·lem Java d'amagat a l'usuari.
#
# Ús:
#   bash scripts/install-android-sdk.sh --dest /ruta/al/sdk [--api 34]
#                                       [--with-emulator] [--force] [--dry-run]
# ==============================================================================
set -euo pipefail

# Builds coneguts vàlids com a últim recurs si el XML oficial no respon.
# (verificats 2026-11: el zip de mac NO té el mateix número que linux/win)
BUILD_MAC_FALLBACK="15641748"
BUILD_LINUX_FALLBACK="16111833"
BUILD_WIN_FALLBACK="16111833"
REPO_XML="https://dl.google.com/android/repository/repository2-1.xml"
BASE_URL="https://dl.google.com/android/repository"

DEST=""
API="34"
WITH_EMULATOR=0
FORCE=0
DRY_RUN=0
BUILD_OVERRIDE=""

die() {
  echo "ERROR: $*" >&2
  exit 1
}

usage() {
  sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'
}

while [ $# -gt 0 ]; do
  case "$1" in
    --dest) DEST="${2:-}"; shift 2 ;;
    --dest=*) DEST="${1#*=}"; shift ;;
    --api) API="${2:-}"; shift 2 ;;
    --api=*) API="${1#*=}"; shift ;;
    --build) BUILD_OVERRIDE="${2:-}"; shift 2 ;;
    --with-emulator) WITH_EMULATOR=1; shift ;;
    --force) FORCE=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die "opció desconeguda: $1 (prova --help)" ;;
  esac
done

[ -n "$DEST" ] || die "cal indicar --dest <carpeta> on posar l'SDK (usa --help)"
case "$API" in
  ''|*[!0-9]*) die "«--api» ha de ser un número (p. ex. 34)" ;;
esac

# ── 1. Sistema operatiu ──────────────────────────────────────────────────────
case "$(uname -s)" in
  Darwin) OS_KEY="mac" ;;
  Linux) OS_KEY="linux" ;;
  MINGW*|MSYS*|CYGWIN*) OS_KEY="win" ;;
  *) die "sistema no suportat: $(uname -s)" ;;
esac

# ── 2. Java 17 (requisit dur de sdkmanager) ──────────────────────────────────
find_java() {
  # macOS: el gestor de JVMs oficial, després el JBR d'Android Studio.
  if [ "$OS_KEY" = "mac" ]; then
    if v="$(/usr/libexec/java_home -v 17 2>/dev/null)" && [ -x "$v/bin/java" ]; then
      echo "$v"; return 0
    fi
    if [ -x "/Applications/Android Studio.app/Contents/jbr/Contents/Home/bin/java" ]; then
      echo "/Applications/Android Studio.app/Contents/jbr/Contents/Home"; return 0
    fi
  fi
  # Qualsevol SO: si el «java» del PATH és la 17, el fem servir.
  if command -v java >/dev/null 2>&1; then
    major="$(java -version 2>&1 | head -1 | sed -E 's/.*"([0-9]+).*/\1/')"
    if [ "$major" = "17" ]; then
      dirname "$(dirname "$(readlink -f "$(command -v java)")" 2>/dev/null || command -v java)"
      return 0
    fi
  fi
  return 1
}

if ! JAVA_HOME_RESOLVED="$(find_java)"; then
  echo "No s'ha trobat cap JDK 17, que és el que «sdkmanager» necessita." >&2
  echo "  macOS:  brew install --cask temurin@17" >&2
  echo "  Linux:  sudo apt install openjdk-17-jdk-headless" >&2
  echo "  O bé instal·la Android Studio (porta el seu propi JBR)." >&2
  exit 2
fi
echo "JDK 17 detectat: $JAVA_HOME_RESOLVED"

# ── 3. Número de build dels cmdline-tools (autodescoberta) ───────────────────
case "$OS_KEY" in
  mac) FALLBACK="$BUILD_MAC_FALLBACK" ;;
  win) FALLBACK="$BUILD_WIN_FALLBACK" ;;
  *) FALLBACK="$BUILD_LINUX_FALLBACK" ;;
esac

BUILD="$BUILD_OVERRIDE"
if [ -z "$BUILD" ]; then
  BUILD="$(curl -fsSL --retry 2 --max-time 40 "$REPO_XML" 2>/dev/null \
    | grep -oE "commandlinetools-${OS_KEY}-[0-9]+_latest\.zip" \
    | sed -E "s/.*-${OS_KEY}-([0-9]+)_latest\.zip/\1/" \
    | sort -n | tail -1 || true)"
fi
[ -n "$BUILD" ] || BUILD="$FALLBACK"
case "$BUILD" in
  ''|*[!0-9]*) die "número de build no vàlid: $BUILD" ;;
esac

URL="$BASE_URL/commandlinetools-${OS_KEY}-${BUILD}_latest.zip"
echo "Descàrrega prevista: $URL"

if [ "$DRY_RUN" = "1" ]; then
  PKGS="platform-tools platforms;android-$API build-tools;$API.0.0"
  [ "$WITH_EMULATOR" = "1" ] && PKGS="$PKGS emulator system-images;android-$API;google_apis;x86_64"
  echo "(dry-run) destinació: $DEST"
  echo "(dry-run) paquets: $PKGS"
  echo "(dry-run) JAVA_HOME=$JAVA_HOME_RESOLVED"
  exit 0
fi

# ── 4. Layout del SDK: <dest>/cmdline-tools/latest ───────────────────────────
SDKM="$DEST/cmdline-tools/latest/bin/sdkmanager"
if [ -x "$SDKM" ] && [ "$FORCE" != "1" ];
then
  echo "Ja hi ha «sdkmanager» a $DEST/cmdline-tools/latest: s'usa l'existent (passa --force per tornar-lo a baixar)."
else
  mkdir -p "$DEST/cmdline-tools" || die "no puc crear $DEST (volum desendollat / sense permisos?)"
  TMP="$(mktemp -d)"
  trap 'rm -rf "$TMP"' EXIT
  curl -fL --retry 3 -C - -o "$TMP/cmdline-tools.zip" "$URL" || die "ha fallat la descàrrega de $URL"
  unzip -tq "$TMP/cmdline-tools.zip" >/dev/null || die "el zip descarregat està corrupte"
  unzip -q "$TMP/cmdline-tools.zip" -d "$TMP/unpacked" || die "no s'ha pogut descomprimir"
  [ -d "$TMP/unpacked/cmdline-tools" ] || die "el zip no porta la carpeta «cmdline-tools» esperada"
  rm -rf "$DEST/cmdline-tools/latest"
  mv "$TMP/unpacked/cmdline-tools" "$DEST/cmdline-tools/latest"
  chmod -R u+x "$DEST/cmdline-tools/latest/bin"
  echo "cmdline-tools $BUILD instal·lats a $DEST/cmdline-tools/latest"
fi

# ── 5. Llicències i paquets de compilació ────────────────────────────────────
export JAVA_HOME="$JAVA_HOME_RESOLVED"
export ANDROID_HOME="$DEST"
export ANDROID_SDK_ROOT="$DEST"

echo "Acceptant llicències (primer cop pot trigar una mica)…"
yes 2>/dev/null | "$SDKM" --sdk_root="$DEST" --licenses >/dev/null || true

PKGS=("platform-tools" "platforms;android-$API" "build-tools;$API.0.0")
if [ "$WITH_EMULATOR" = "1" ]; then
  PKGS+=("emulator" "system-images;android-$API;google_apis;x86_64")
fi

echo "Baixant ${PKGS[*]} …"
"$SDKM" --sdk_root="$DEST" "${PKGS[@]}" || die "sdkmanager ha fallat (mira el missatge de sobre)"

# ── 6. Resum i com fer-ho visible a la resta de programs ─────────────────────
echo
echo "NOORBIT_ANDROID_SDK_OK"
echo "SDK a: $DEST"
du -sh "$DEST" 2>/dev/null || true
echo
echo "Perquè els subprocessos (Gradle, adb, NoOrbit) el troben:"
if [ "$OS_KEY" = "mac" ]; then
  echo "  launchctl setenv ANDROID_HOME \"$DEST\"    # per a aquesta sessió"
  echo "  (NoOrbit ho fa sol si guardes la carpeta des del seu panell d'Android.)"
else
  echo "  export ANDROID_HOME=\"$DEST\""
fi
echo
if [ "$WITH_EMULATOR" != "1" ]; then
  echo "Nota: sense «emulator» ni system images NO hi haurà AVDs; es pot compilar"
  echo "l'APK i connectar un dispositiu físic amb «adb». Per als AVDs, reexecuta"
  echo "amb --with-emulator (baixa diversos GB més)."
fi
