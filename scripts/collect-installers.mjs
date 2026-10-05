#!/usr/bin/env node
/**
 * collect-installers.mjs
 *
 * Repleix els paquets d'instal·lació ja compilats per Tauri (dins de
 * la carpeta «bundle» del directori de sortida de Tauri) i els copia en una
 * carpeta neta a l'arrel del projecte: «NoOrbit-Instalador/».
 *
 * Objectiu: després de qualsevol build (macOS/Linux/Windows), tenir en UN
 * SOL LLOC els fitxers per a fer-hi DOBLE CLIC i instal·lar, sense haver
 * de navegar la ruta llarga del directori de sortida de Tauri.
 *
 * No compila res: només recol·lecta. Per construir, usa primer
 * `pnpm app:build:mac` (o el target que corresponga) i després `pnpm installers`.
 */
import { readdirSync, statSync, mkdirSync, copyFileSync, existsSync, rmSync } from "node:fs";
import { join, dirname, basename } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, "..");
const BUNDLE_BASE = join(ROOT, "apps", "desktop", "src-tauri", "target");
const OUT_DIR = join(ROOT, "NoOrbit-Instalador");

// Extensions de paquet que tenen sentit com a «doble clic per instal·lar».
const INSTALLER_EXT = new Set([
  ".dmg", ".app", ".pkg",           // macOS
  ".deb", ".rpm", ".AppImage", ".appimage", // Linux
  ".exe", ".msi",                   // Windows
]);

/// Recorre recursivament i retorna tots les rutes que acaben en una extensió
/// d'instal·lador coneguda, dins de qualsevol carpeta `bundle`.
function findInstallers(dir, acc = []) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return acc; // el directori no existeix (p. ex. no s'ha compilat encara)
  }
  for (const e of entries) {
    const full = join(dir, e.name);
    if (e.isDirectory()) {
      findInstallers(full, acc);
      continue;
    }
    const ext = extOf(e.name);
    if (!INSTALLER_EXT.has(ext)) continue;
    // Ignora les imatges temporals «rw.<pid>.….dmg» que Tauri deixa al
    // directori macos/ mentre construi el DMG final: no són instal·ladors.
    if (/^rw\.[0-9]+\./.test(e.name)) continue;
    acc.push(full);
  }
  return acc;
}

/// Extensió que reconeix el script (incloent `.AppImage`, que té majúscula).
function extOf(name) {
  const m = name.match(/\.(dmg|app|pkg|deb|rpm|appimage|AppImage|exe|msi)$/i);
  return m ? (m[0] === ".AppImage" ? ".AppImage" : m[0].toLowerCase()) : "";
}

function main() {
  if (!existsSync(BUNDLE_BASE)) {
    console.error("No s'ha trobat cap sortida de build en src-tauri/target.");
    console.error("Compila primer (p. ex. `pnpm app:build:mac`) i torna-ho a provar.");
    process.exit(1);
  }

  const found = findInstallers(BUNDLE_BASE);
  if (found.length === 0) {
    console.error("No hi ha paquets d'instal·lació per a recol·lectar encara.");
    console.error("Executa un build (`pnpm app:build:mac`, `:win`, `:linux`…) abans.");
    process.exit(1);
  }

  // Carpeta de destinació neta: se'n buida el contingut anterior per a
  // que sempre reflexis l'últim build.
  mkdirSync(OUT_DIR, { recursive: true });
  for (const stale of readdirSync(OUT_DIR)) {
    if (stale.toLowerCase().endsWith(".dmg") || INSTALLER_EXT.has(extOf(stale))) {
      rmSync(join(OUT_DIR, stale), { recursive: true, force: true });
    }
  }

  const copied = [];
  for (const src of found) {
    // .app és un directori: el copiem recursivament; la resta, fitxers.
    const isDir = statSync(src).isDirectory();
    const dest = join(OUT_DIR, basename(src));
    if (isDir) {
      rmSync(dest, { recursive: true, force: true });
      copyTree(src, dest);
    } else {
      copyFileSync(src, dest);
    }
    copied.push(`${basename(src)}  ←  ${src.replace(ROOT + "/", "")}`);
  }

  console.log(`\n✔ ${copied.length} paquet(s) preparats a:  ${OUT_DIR}`);
  for (const line of copied) console.log("   • " + line);
  console.log("\nFes DOBLE CLIC al .dmg (o .app/.exe/…) per a instal·lar.");
}

function copyTree(from, to) {
  mkdirSync(to, { recursive: true });
  for (const e of readdirSync(from, { withFileTypes: true })) {
    const a = join(from, e.name);
    const b = join(to, e.name);
    if (e.isDirectory()) copyTree(a, b);
    else copyFileSync(a, b);
  }
}

main();
