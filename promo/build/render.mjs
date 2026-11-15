#!/usr/bin/env node
/* Renderitzador de fotogrames — Storyboard → PNG
   ús:  node build/render.mjs preview <t1,t2,...> [outDir]
        node build/render.mjs full [outDir] [wide|v]
*/
import { chromium } from "playwright";
import fs from "node:fs";
import path from "node:path";

const ROOT = "/Users/appleone/programes/noorbit/promo";
const STORY = "file://" + path.join(ROOT, "build/story.html");
const FPS = 30;
const DUR = 53;

const mode = process.argv[2] || "preview";
const fmt = mode === "preview" ? process.argv[5] || "wide" : process.argv[4] || "wide";
const VP = fmt === "v" ? { width: 1080, height: 1920 } : { width: 1920, height: 1080 };
const outDir = path.join(ROOT, mode === "preview" ? process.argv[4] || "frames" : process.argv[3] || "frames");
fs.mkdirSync(outDir, { recursive: true });

const timings = JSON.parse(fs.readFileSync(path.join(ROOT, "build/vo/timings.json"), "utf8"));
const captions = timings.map((t) => ({ in: t.in, dur: t.dur, text: t.text }));

const browser = await chromium.launch({ args: ["--force-color-profile=srgb", "--disable-lcd-text"] });
const page = await browser.newPage({ viewport: VP, deviceScaleFactor: 1 });
page.on("pageerror", (e) => console.error("[pageerror]", String(e).slice(0, 400)));
await page.goto(`${STORY}?fmt=${fmt}`, { waitUntil: "load" });
await page.waitForFunction(() => window.__ready === true);
await page.evaluate(() => document.fonts.ready);
await page.evaluate((c) => window.__setCaptions(c), captions);

const times =
  mode === "preview"
    ? (process.argv[3] || "0.6,1.6,2.8,4.6,5.6,7.6,8.6,10.4,12.2,14.5,16.5,18.6,20.9,21.8,23.0,24.3,25.2,26.4,28.0,29.2")
        .split(",")
        .map(Number)
    : null;

const t0 = Date.now();
if (mode === "preview") {
  for (const t of times) {
    await page.evaluate((tt) => window.__render(tt), t);
    await page.waitForTimeout(40);
    const name = `t${String(t).padStart(5, "0").replace(".", "_")}.png`;
    await page.screenshot({ path: path.join(outDir, name) });
    console.log("→", name);
  }
} else {
  const total = Math.round(DUR * FPS);
  for (let i = 0; i < total; i++) {
    const t = i / FPS;
    const out = path.join(outDir, `f${String(i).padStart(5, "0")}.png`);
    if (fs.existsSync(out)) continue;          // represa: no tornem a renderitzar
    await page.evaluate((tt) => window.__render(tt), t);
    await page.screenshot({ path: out });
    if (i % 30 === 0) {
      const pct = ((i / total) * 100).toFixed(0);
      const el = (Date.now() - t0) / 1000;
      const eta = (el / Math.max(1, i)) * (total - i);
      console.log(`  ${String(i).padStart(4)}/${total}  ${pct}%  ${el.toFixed(0)}s · ETA ${eta.toFixed(0)}s`);
    }
  }
}
console.log(`Fet en ${((Date.now() - t0) / 1000).toFixed(1)}s → ${outDir}`);
await browser.close();
