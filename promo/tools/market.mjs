import { chromium } from "playwright";

const b = await chromium.launch();
const page = await b.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2 });
const shots = [
  ["https://themes.thepuigberts.com/", "market-home.png"],
  ["https://themes.thepuigberts.com/tema/atelier-makeup-2026/", "market-theme.png"],
];
for (const [url, name] of shots) {
  try {
    await page.goto(url, { waitUntil: "networkidle", timeout: 45000 });
    await page.waitForTimeout(2500);
    await page.screenshot({ path: `/Users/appleone/programes/noorbit/promo/shots/${name}` });
    console.log("OK", name);
  } catch (e) {
    console.log("ERR", name, String(e).slice(0, 160));
  }
}
await b.close();
