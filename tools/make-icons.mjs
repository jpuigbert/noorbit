// Genera iconos PNG (cuadrados) y .icns para Tauri sin dependencias externas.
// Uso: node make-icons.mjs <dirsalida>
import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";

const outDir = process.argv[2] || ".";
mkdirSync(outDir, { recursive: true });

const crcTable = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const td = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(td));
  return Buffer.concat([len, td, crc]);
}

// Icono: fondo degradado oscuro + rombo acento violeta (logo "orbit" simplificado)
function png(size) {
  const rows = [];
  for (let y = 0; y < size; y++) {
    const row = Buffer.alloc(1 + size * 4);
    for (let x = 0; x < size; x++) {
      const i = 1 + x * 4;
      const nx = (x / size) * 2 - 1;
      const ny = (y / size) * 2 - 1;
      const d = Math.sqrt(nx * nx + ny * ny);
      // diamante |nx|+|ny| < r
      const dia = Math.abs(nx) + Math.abs(ny);
      if (dia < 0.62) {
        row[i] = 124; row[i + 1] = 92; row[i + 2] = 255; row[i + 3] = 255;
      } else if (dia < 0.72) {
        row[i] = 34; row[i + 1] = 211; row[i + 2] = 238; row[i + 3] = 255;
      } else {
        const g = Math.round(18 + 22 * (1 - d / 1.5));
        row[i] = g; row[i + 1] = g; row[i + 2] = Math.round(g + 10); row[i + 3] = 255;
      }
    }
    rows.push(row);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; ihdr[9] = 6; // RGBA
  const idat = deflateSync(Buffer.concat(rows), { level: 9 });
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", idat),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

const sizes = [16, 32, 64, 128, 256, 512, 1024];
const pngs = {};
for (const s of sizes) {
  pngs[s] = png(s);
  writeFileSync(join(outDir, `icon-${s}.png`), pngs[s]);
}
writeFileSync(join(outDir, "32x32.png"), pngs[32]);
writeFileSync(join(outDir, "32x32@2x.png"), pngs[64]);
writeFileSync(join(outDir, "128x128.png"), pngs[128]);
writeFileSync(join(outDir, "128x128@2x.png"), pngs[256]);
writeFileSync(join(outDir, "icon.png"), pngs[512]);

// .icns contenedor
const icnsTypes = { 16: "icp4", 32: "icp5", 64: "icp9", 128: "ic07", 256: "ic08", 512: "ic09", 1024: "ic10" };
const entries = [];
let total = 8;
for (const s of sizes) {
  const data = pngs[s];
  const h = Buffer.alloc(8);
  h.write(icnsTypes[s], 0, 4, "latin1");
  h.writeUInt32BE(8 + data.length);
  entries.push(h, data);
  total += 8 + data.length;
}
const header = Buffer.alloc(8);
header.write("icns", 0, "latin1");
header.writeUInt32BE(total);
writeFileSync(join(outDir, "icon.icns"), Buffer.concat([header, ...entries]));

// .ico simple (usa el png de 256, formato PNG dentro de ICO es válido)
const icoData = pngs[256];
const icoHeader = Buffer.alloc(6);
icoHeader.writeUInt16LE(0, 0); icoHeader.writeUInt16LE(1, 2); icoHeader.writeUInt16LE(1, 4);
const icoEntry = Buffer.alloc(16);
icoEntry[0] = 0; icoEntry[1] = 0; icoEntry[2] = 0; icoEntry[3] = 0; // 256x256
icoEntry.writeUInt16LE(1, 4); icoEntry.writeUInt16LE(32, 6);
icoEntry.writeUInt32LE(icoData.length, 8); icoEntry.writeUInt32LE(22, 12);
writeFileSync(join(outDir, "icon.ico"), Buffer.concat([icoHeader, icoEntry, icoData]));

console.log("Iconos generados en", outDir);
