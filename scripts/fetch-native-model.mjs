#!/usr/bin/env node
// Descarrega el model GGUF petit que la IA NATIVA de NoOrbit porta EN
// l'instal·lador (decisió de l'usuari: «un cop instal·lat, funciono offline
// sense baixar res»). Cal un LLM real que cabiga en 4 GB: un model d'uns
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

// Asset oficial de llama.cpp per a la plataforma actual (per empaquetar el
// motor i fer la nativa realment offline).
function assetLlamaCpp() {
  const p = process.platform;
  const a = process.arch;
  if (p === "darwin") return a === "arm64" ? "bin-macos-arm64" : "bin-macos-x64";
  if (p === "linux") return a === "arm64" ? "bin-ubuntu-arm64" : "bin-ubuntu-x64";
  if (p === "win32") return a === "arm64" ? "bin-win-cpu-arm64" : "bin-win-cpu-x64";
  return null;
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

async function portaEngine() {
  const marker = join(engineDir, ".llamacpp-ok");
  if (existsSync(marker)) {
    console.log("✓ El motor (llama.cpp) ja és empaquetat");
    return;
  }
  const asset = assetLlamaCpp();
  if (!asset) {
    console.warn(`⚠ Plataforma desconeguda (${process.platform}/${process.arch}): no s'empaqueta el motor.`);
    return;
  }
  const rels = await fetch(
    "https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=5",
    { headers: { "User-Agent": "NoOrbit-installer", Accept: "application/vnd.github+json" } }
  ).then((r) => r.json());
  let url;
  let nom;
  for (const rel of rels) {
    for (const a of rel.assets ?? []) {
      if (a.name.includes(asset) && (a.name.endsWith(".zip") || a.name.endsWith(".tar.gz"))) {
        url = a.browser_download_url;
        nom = a.name;
        break;
      }
    }
    if (url) break;
  }
  if (!url) {
    console.warn(`⚠ No trobo un binari oficial de llama.cpp per a ${asset}: la nativa caurà al motor del sistema o al simbòlic.`);
    return;
  }
  const arxiu = join(engineDir, nom);
  mkdirSync(engineDir, { recursive: true });
  await descarrega(url, arxiu);
  if (nom.endsWith(".zip")) {
    execFileSync("unzip", ["-o", arxiu, "-d", engineDir], { stdio: "inherit" });
  } else {
    execFileSync("tar", ["-xzf", arxiu, "-C", engineDir], { stdio: "inherit" });
  }
  // Neteja l'arxiu comprimit: només cal el binari desempaquetat.
  const { rmSync } = await import("node:fs");
  rmSync(arxiu, { force: true });
  // Marca el servidor executable (Unix).
  try {
    execFileSync("chmod", ["-R", "+x", engineDir], { stdio: "ignore" });
  } catch {
    /* Windows: no cal chmod */
  }
  const { writeFileSync } = await import("node:fs");
  writeFileSync(marker, new Date().toISOString());
  console.log(`✓ Motor empaquetat: ${nom}`);
}

async function main() {
  const dest = await portaModel();
  if (withEngine) await portaEngine();
  console.log("\nModel llest per empaquetar als recursos de Tauri:");
  console.log(`  ${dest}`);
  console.log("Reexecuta l'build de l'app perquè el .app/.dmg l'inclogui.");
}

main().catch((e) => {
  console.error("\n✗ ERROR:", e.message ?? e);
  process.exit(1);
});
