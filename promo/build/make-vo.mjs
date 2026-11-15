#!/usr/bin/env node
// Genera la locució (TTS) i mesura la durada real de cada segment.
// Sortida: build/vo/seg-<id>.wav  +  build/vo/timings.json
//
// Cada segment pot tenir la seva pròpia veu, ritme i to (`pbas`): així el
// personatge humà i el personatge NoOrbit sonen diferents però comparteixen
// el mateix timbre de veu (català, Montse) — coherent, no accelerat.
import { execFile, spawn } from "node:child_process";
import { promisify } from "node:util";
import fs from "node:fs";
import path from "node:path";

const exec = promisify(execFile);
const ROOT = "/Users/appleone/programes/noorbit/promo";
const FF = path.join(ROOT, "node_modules/ffmpeg-static/ffmpeg");
const OUT = path.join(ROOT, "build/vo");
fs.mkdirSync(OUT, { recursive: true });

const cfg = JSON.parse(fs.readFileSync(path.join(ROOT, "build/vo.json"), "utf8"));

async function duration(file) {
  return new Promise((resolve, reject) => {
    const p = spawn(FF, ["-hide_banner", "-i", file], { stdio: ["ignore", "ignore", "pipe"] });
    let err = "";
    p.stderr.on("data", (d) => (err += d));
    p.on("close", () => {
      const m = /Duration:\s*(\d+):(\d+):([\d.]+)/.exec(err);
      if (!m) return reject(new Error(`no duration for ${file}\n${err.slice(0, 400)}`));
      resolve((+m[1]) * 3600 + (+m[2]) * 60 + (+m[3]));
    });
  });
}

const timings = [];
let prevOut = 0;
for (const seg of cfg.segments) {
  const aiff = path.join(OUT, `seg-${seg.id}.aiff`);
  const wav = path.join(OUT, `seg-${seg.id}.wav`);
  // ordre de comandes de veu de macOS: primer el to (pbas), després el text.
  const spoken = (seg.pbas ? `[[pbas ${seg.pbas}]]` : "") + seg.text;
  await exec("say", ["-v", seg.voice || "Montse", "-r", String(seg.rate || 150), "-o", aiff, spoken]);
  await exec(FF, ["-y", "-hide_banner", "-loglevel", "error", "-i", aiff,
    "-ar", "48000", "-ac", "1", "-c:a", "pcm_s16le", wav]);
  const d = await duration(aiff);
  const out = +(seg.in + d).toFixed(3);
  const gap = +(seg.in - prevOut).toFixed(2);
  timings.push({ ...seg, file: wav, aiff, dur: +d.toFixed(3), out });
  console.log(
    `${seg.id.padEnd(4)} in=${seg.in.toFixed(2).padStart(6)}  dur=${d.toFixed(2)}  out=${out.toFixed(2).padStart(6)}` +
    `  gap=${String(gap).padStart(5)}  pbas=${seg.pbas || 50} r=${seg.rate || 150}  «${seg.text.slice(0, 42)}»`
  );
  prevOut = out;
}

fs.writeFileSync(path.join(OUT, "timings.json"), JSON.stringify(timings, null, 2));
const total = Math.max(...timings.map((t) => t.out));
console.log(`\nLa veu acaba a ${total.toFixed(2)}s. Durada del vídeo: 53.00s`);
