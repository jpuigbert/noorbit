#!/usr/bin/env node
/**
 * EMPAQUETAT PORTÀTIL DE NOORBIT — crea un «NoOrbit-Portable» que es pot
 * copiar a un USB i executar en macOS, Linux o Windows.
 *
 * El paquet conté:
 *   - l'aplicació (o el binari) ja compilada,
 *   - un LLANÇADOR propi per al sistema (`.command` / `.sh` / `.cmd`) que
 *     crea el marcador `.noorbit-portable` si no hi és i obre l'app: així
 *     tot (config, models, especialistes, històric) queda al costat en el USB
 *     i l'app es porta a qualsevol màquina sense instal·lar res,
 *   - un «Llegeix-me.txt» en català.
 *
 * Ús:
 *   node scripts/make-portable.mjs                 # detecta el sistema
 *   node scripts/make-portable.mjs --platform linux
 *   node scripts/make-portable.mjs --platform windows
 *   node scripts/make-portable.mjs --platform macos
 *   node scripts/make-portable.mjs --out ~/Desktop
 */

import { existsSync, mkdirSync, copyFileSync, cpSync, writeFileSync, chmodSync, rmSync } from "node:fs";
import { join, dirname, basename, resolve } from "node:path";
import { homedir } from "node:os";
import { fileURLToPath } from "node:url";
import { execSync } from "node:child_process";

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(__dirname, "..");
const DESKTOP = join(REPO, "apps", "desktop");
const TAURI = join(DESKTOP, "src-tauri");

const args = process.argv.slice(2);
const arg = (name, def = null) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 && args[i + 1] ? args[i + 1] : def;
};

const platform = arg("platform", process.platform);
const outDir = resolve(arg("out", join(REPO, "dist-portable")));

// On busca l'aplicació compilada, per sistema. Primer l'arquitectura del
// equip (el paquet portàtil ha de ser el del mateix tipus de CPU), després
// la universal i, finalment, les altres.
const HOST_ARCH = process.arch === "arm64" ? "aarch64" : "x86_64";
const TARGETS = {
  macos: [
    join(TAURI, "target", `${HOST_ARCH}-apple-darwin`, "release", "bundle", "macos", "NoOrbit.app"),
    join(TAURI, "target", "universal-apple-darwin", "release", "bundle", "macos", "NoOrbit.app"),
    join(TAURI, "target", "x86_64-apple-darwin", "release", "bundle", "macos", "NoOrbit.app"),
    join(TAURI, "target", "aarch64-apple-darwin", "release", "bundle", "macos", "NoOrbit.app"),
    join(TAURI, "target", "release", "bundle", "macos", "NoOrbit.app"),
  ],
  linux: [
    // L'AppImage és la millor opció a Linux: porta les biblioteques
    // (WebKitGTK…) dins, no cal instal·lar res al sistema.
    join(TAURI, "target", "x86_64-unknown-linux-gnu", "release", "bundle", "appimage"),
    join(TAURI, "target", "release", "bundle", "appimage"),
    join(TAURI, "target", "x86_64-unknown-linux-gnu", "release", "no-orbit"),
    join(TAURI, "target", "release", "no-orbit"),
  ],
  windows: [
    join(TAURI, "target", "x86_64-pc-windows-msvc", "release", "no-orbit.exe"),
    join(TAURI, "target", "release", "no-orbit.exe"),
  ],
};

const src = TARGETS[platform]?.find((p) => existsSync(p));
if (!src) {
  console.error(
    `✗ No trobo l'aplicació compilada per a «${platform}».\n` +
      `  Prova: pnpm app:build:mac  (o :linux / :win) i després torna-ho a fer.`
  );
  process.exit(1);
}

const dest = join(outDir, "NoOrbit-Portable");
rmSync(dest, { recursive: true, force: true });
mkdirSync(dest, { recursive: true });

// 1) L'aplicació.
if (platform === "macos") {
  cpSync(src, join(dest, "NoOrbit.app"), { recursive: true, dereference: true });
} else if (platform === "linux") {
  // Si és un AppImage, es copia tal qual (s'executa amb un clic).
  const name = basename(src).endsWith(".AppImage") ? basename(src) : "no-orbit";
  copyFileSync(src, join(dest, name));
  chmodSync(join(dest, name), 0o755);
} else {
  copyFileSync(src, join(dest, "no-orbit.exe"));
}

// 2) El marcador: l'app es posa en mode portàtil automàticament.
writeFileSync(join(dest, ".noorbit-portable"), "portable\n");

// 3) El llançador propi del sistema.
const launchers = {
  macos: {
    name: "NoOrbit.command",
    body: `#!/bin/bash
# NoOrbit portàtil: obre l'aplicació DES DE aquest USB. Les dades
# (config, models, especialistes, històric) queden a la carpeta
# «NoOrbitData» al costat, així que l'USB es pot endur a qualsevol Mac.
cd "$(dirname "$0")" || exit 1
[ -f .noorbit-portable ] || echo portable > .noorbit-portable
# El macOS pot marcar l'app copiada com a "d'un origen no identificat":
# en traiem el segell perquè s'obri amb un sol clic.
xattr -dr com.apple.quarantine "./NoOrbit.app" 2>/dev/null
open "./NoOrbit.app"
`,
  },
  linux: {
    name: "NoOrbit.sh",
    body: `#!/bin/bash
# NoOrbit portàtil: executa l'aplicació DES DE aquest USB/pen drive.
# Les dades queden a la carpeta «NoOrbitData» al costat.
cd "$(dirname "$0")" || exit 1
[ -f .noorbit-portable ] || echo portable > .noorbit-portable
if [ -x ./no-orbit ]; then
  chmod +x ./no-orbit 2>/dev/null
  exec ./no-orbit "$@"
fi
for app in *.AppImage; do
  [ -e "$app" ] || continue
  chmod +x "$app" 2>/dev/null
  exec "./$app" "$@"
done
echo "No trobo el binari de NoOrbit en aquesta carpeta."
exit 1
`,
  },
  windows: {
    name: "NoOrbit.cmd",
    body: `@echo off
rem NoOrbit portàtil: arrenca l'aplicació DES DE aquest USB.
rem Les dades queden a la carpeta "NoOrbitData" al costat.
cd /d "%~dp0"
if not exist ".noorbit-portable" echo portable> .noorbit-portable
start "" "%~dp0no-orbit.exe" %*
`,
  },
};
const l = launchers[platform];
if (l) {
  const p = join(dest, l.name);
  writeFileSync(p, l.body);
  if (platform !== "windows") chmodSync(p, 0o755);
}

// 4) Les instruccions.
writeFileSync(
  join(dest, "Llegeix-me.txt"),
  `NoOrbit — versió PORTÀTIL
========================

Aquesta carpeta es pot copiar a un USB i executar en qualsevol
${platform === "macos" ? "Mac" : platform === "linux" ? "ordinador Linux" : "ordinador Windows"}.
No cal instal·lar res: les dades (configuració, models, especialistes,
historial de xats i imatges) es desen a la carpeta «NoOrbitData» que hi ha
al costat. Per tant, el mateix USB serveix per continuar la feina on vulguis.

COMENÇAR
---------
${
  platform === "macos"
    ? "Fes doble clic a «NoOrbit.command» (la primera vegada, si el macOS el bloqueja:\n    clic dret ▸ Obre)."
    : platform === "linux"
    ? "Executa «NoOrbit.sh» (o, des d'un terminal: ./NoOrbit.sh).\n    Cal el permís d'execució: chmod +x NoOrbit.sh"
    : "Fes doble clic a «NoOrbit.cmd»."
}

MODELS DE IA
------------
Al menú «IA ▸ Models» pots posar els models (i el motor local) en aquest
mateix USB: així un model de 20 GB no ocupa el disc del portàtil i el
porta amb tu. Des de «Hugging Bay» també els pots baixar directament al USB.

RECOMANACIÓ
------------
Desa el projecte (la carpeta de codi) dins del mateix USB si hi ha de
treballar-hi: així tot viatja junts.

Fet per Joan Puigbert Carreras · Projecte de Thepuigberts.com
`
);

console.log(`✓ Paquet portàtil creat: ${dest}`);
console.log(`  Origen: ${src}`);
console.log(`  Contingut: ${basename(src)} + ${l.name} + .noorbit-portable + Llegeix-me.txt`);
