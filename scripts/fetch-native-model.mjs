#!/usr/bin/env node
// Descarrega el model GGUF petit que la IA NATIVA de NoOrbit porta EN
// l'instal·lador (decisió de l'usuari: «un cop instal·lat, funciono offline
// sense baixar res»). Cal un LLM real que capigui en 4 GB: un model d'uns
// 0.5 B params quantitzat Q4 (~350 MB), prou bo per parlar, raonar i programar
// una mica, i suficientment petit per no saturar la RAM.
//
// Es desen a `apps/desktop/src-tauri/native-model/`, que `tauri.conf.json`
// inclou com a recurs. `ai/native_llm.rs` el resol a l'arrencada.
//
// Ús:
//   node scripts/fetch-native-model.mjs            // només el model
//   node scripts/fetch-native-model.mjs --engine   // model + llama.cpp
//
// Idempotent: si el fitxer ja és al seu lloc amb la mida esperada, no el
// torna a baixar.
import { createWriteStream, existsSync, mkdirSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { pipeline } from "node:stream/promises";
import { execFileSync } from "node:child_process";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, "..");
const srcTauri = join(root, "apps", "desktop", "src-tauri");
const modelDir = join(srcTauri, "native-model");
const engineDir = join(srcTauri, "native-engine");

const withEngine = process.argv.includes("--engine");

// Model per defecte: Qwen2.5-0.5B-Instruct Q4_K_M (multilingüe, bo en català,
// ~350 MB). Es pot sobreescriure amb --model <url> --out <nom>.
const MODEL_URL =
  valorFlag("--model") ??
  "https://huggingface.co/QuantFactory/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/Qwen2.5-0.5B-Instruct.Q4_K_M.gguf";
const MODEL_NAME =
  valorFlag("--out") ?? MODEL_URL.split("/").pop().split("?")[0];

function valorFlag(flag) {
  const i = process.argv.indexOf(flag);
  return i >= 0 ? process.argv[i + 1] : undefined;
}

async function descarrega(url, dest) {
  mkdirSync(dirname(dest), { recursive: true });
  console.log(`⟳ Baixant ${url}`);
  console.log(`  → ${dest}`);
  const res = await fetch(url, { redirect: "follow", headers: { "User-Agent": "NoOrbit-installer" } });
  if (!res.ok || !res.body) {
    throw new Error(`El servidor ha respost ${res.status} per ${url}`);
  }
  const total = Number(res.headers.get("content-length") || 0);
  let fets = 0;
  let ultims = 0;
  const comptador = new TransformStream({
    transform(chunk, controller) {
      fets += chunk.length;
      const pct = total ? Math.round((fets * 100) / total) : 0;
      if (pct >= ultims + 5) {
        ultims = pct;
        process.stdout.write(`  ${pct}% (${(fets / 1e6).toFixed(0)} MB)\n`);
      }
      controller.enqueue(chunk);
    },
  });
  await pipeline(res.body, comptador, createWriteStream(dest));
  console.log("  ✓ fet");
}

async function portaModel() {
  const dest = join(modelDir, MODEL_NAME);
  if (existsSync(dest) && statSync(dest).size > 1_000_000) {
    console.log(`✓ El model ja és a lloc: ${MODEL_NAME} (${(statSync(dest).size / 1e6).toFixed(0)} MB)`);
    return dest;
  }
  await descarrega(MODEL_URL, dest);
  return dest;
}

// Avís visible TAMBÉ a GitHub Actions. El motor és opcional, però que no hi
// sigui no ha de quedar mai en silenci: a la 0.71.17 va fallar només a macOS
// (el build de Windows i Linux el portaven) i ho vam de descobrir llegint el
// llistat del `.app` ja publicat.
function avisa(text) {
  if (process.env.GITHUB_ACTIONS) console.log(`::warning::${text}`);
  else console.warn(`⚠ ${text}`);
}

// Quins assets oficials de llama.cpp cal empaquetar. A macOS el bundle és
// UNIVERSAL (el mateix .app ha de córrer natiu en Apple Silicon i en Intel),
// així que s'hi posen LES DOS slices en subcarpetes `macos-arm64/` i
// `macos-x64/`; `ai/native_llm.rs` n'escull la de l'arquitectura que
// s'està executant. Windows i Linux van en pla, com sempre.
function assetsAEmpaquetar() {
  const p = process.platform;
  const a = process.arch;
  if (p === "darwin") {
    return [
      { clau: "bin-macos-arm64", sub: "macos-arm64" },
      { clau: "bin-macos-x64", sub: "macos-x64" },
    ];
  }
  if (p === "win32") {
    return [{ clau: a === "arm64" ? "bin-win-cpu-arm64" : "bin-win-cpu-x64", sub: "" }];
  }
  if (p === "linux") {
    return [{ clau: a === "arm64" ? "bin-ubuntu-arm64" : "bin-ubuntu-x64", sub: "" }];
  }
  return [];
}

// Llista de releases de llama.cpp amb reintent i verificació. Dos forats
// reals que tapava: (1) l'API sense autenticar respon un OBJECTE de rate
// limit amb HTTP 403 i el codi ho iterava com si fos la llista
// («rels is not iterable»); (2) qualsevol d'aquells errors feia sortir el
// script amb codi 1 DESPRÉS d'haver baixat el model, de manera que el build
// continuava (continue-on-error) sense el motor i sense cap senyal clar.
async function releasesLlamaCpp() {
  const url = "https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=10";
  const headers = { "User-Agent": "NoOrbit-installer", Accept: "application/vnd.github+json" };
  // El token del runner (si existeix) puja el límit de peticions de 60 a
  // 5000/h: és el que fa que tres builds simultanis no es mengin la quota.
  if (process.env.GITHUB_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  let ultimError;
  for (let intent = 1; intent <= 3; intent++) {
    try {
      const res = await fetch(url, { headers });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const dades = await res.json();
      if (!Array.isArray(dades)) {
        throw new Error(`l'API ha respost «${dades?.message ?? "una estructura inesperada"}»`);
      }
      return dades;
    } catch (e) {
      ultimError = e;
      if (intent < 3) await new Promise((r) => setTimeout(r, intent * 5000));
    }
  }
  throw ultimError;
}

function cercaAsset(rels, clau) {
  for (const rel of rels) {
    for (const a of rel.assets ?? []) {
      if (a.name.includes(clau) && (a.name.endsWith(".zip") || a.name.endsWith(".tar.gz"))) {
        return { url: a.browser_download_url, nom: a.name };
      }
    }
  }
  return null;
}

async function desempaquetar(arxiu, nom, dest) {
  if (nom.endsWith(".zip")) {
    execFileSync("unzip", ["-o", arxiu, "-d", dest], { stdio: "inherit" });
  } else {
    execFileSync("tar", ["-xzf", arxiu, "-C", dest], { stdio: "inherit" });
  }
  // Neteja l'arxiu comprimit: només cal el binari desempaquetat.
  const { rmSync } = await import("node:fs");
  rmSync(arxiu, { force: true });
  // Marca el servidor executable (Unix).
  try {
    execFileSync("chmod", ["-R", "+x", dest], { stdio: "ignore" });
  } catch {
    /* Windows: no cal chmod */
  }
}

async function portaEngine() {
  const targets = assetsAEmpaquetar();
  if (!targets.length) {
    avisa(`Plataforma desconeguda (${process.platform}/${process.arch}): no s'empaqueta el motor.`);
    return;
  }
  let rels;
  try {
    rels = await releasesLlamaCpp();
  } catch (e) {
    avisa(
      `No llisto les releases de llama.cpp (${e?.message ?? e}): la IA nativa farà servir Ollama o el motor del sistema.`
    );
    return;
  }
  let empaquetats = 0;
  for (const { clau, sub } of targets) {
    const dest = sub ? join(engineDir, sub) : engineDir;
    const marker = join(engineDir, `.llamacpp-ok${sub ? `-${sub}` : ""}`);
    if (existsSync(marker)) {
      console.log(`✓ El motor (${sub || clau}) ja és empaquetat`);
      empaquetats++;
      continue;
    }
    const trobat = cercaAsset(rels, clau);
    if (!trobat) {
      avisa(`No trobo cap binari oficial de llama.cpp per a «${clau}».`);
      continue;
    }
    const arxiu = join(dest, trobat.nom);
    // CADA slice es protegeix a part: si la baixada o l'extracció d'una
    // falla a mig camí, l'altra arquitectura NO ha de quedar-se també sense
    // motor (era el comportament anterior: un sol `try` per a tot el bucle).
    try {
      mkdirSync(dest, { recursive: true });
      await descarrega(trobat.url, arxiu);
      await desempaquetar(arxiu, trobat.nom, dest);
      const { writeFileSync } = await import("node:fs");
      writeFileSync(marker, new Date().toISOString());
      console.log(`✓ Motor empaquetat: ${trobat.nom}${sub ? ` → native-engine/${sub}/` : ""}`);
      empaquetats++;
    } catch (e) {
      avisa(`Ha fallat el motor «${trobat.nom}»: ${e?.message ?? e}`);
    }
  }
  if (empaquetats === 0) {
    avisa(
      "El bundle NO porta el motor llama.cpp: la IA nativa dependrà d'Ollama o del motor del sistema."
    );
  }
}

async function main() {
  const dest = await portaModel();
  if (withEngine) {
    // El motor és OPCIONAL i mai ha de fer fallar el build — però cada
    // fracàs seu deixa un avís visible (vegeu `avisa`).
    try {
      await portaEngine();
    } catch (e) {
      avisa(`Motor no empaquetat: ${e?.message ?? e}`);
    }
  }
  console.log("\nModel llest per empaquetar als recursos de Tauri:");
  console.log(`  ${dest}`);
  console.log("Reexecuta l'build de l'app perquè el .app/.dmg l'inclogui.");
}

main().catch((e) => {
  console.error("\n✗ ERROR:", e.message ?? e);
  process.exit(1);
});
