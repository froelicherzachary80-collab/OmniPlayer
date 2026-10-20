// Renders the PNG app icons (Apple touch icon + manifest icons) from the pixel design.
import { deflateSync } from "node:zlib";
import { writeFileSync } from "node:fs";
const design = [
  [0, 0, 16, 16, [11, 13, 20]],
  [3, 3, 10, 10, [124, 92, 255]],
  [5, 5, 2, 2, [255, 255, 255]],
  [9, 5, 2, 2, [255, 255, 255]],
  [5, 9, 6, 2, [46, 230, 168]],
];
function crc32(b) { let c, crc = ~0; for (const x of b) { c = (crc ^ x) & 255; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; crc = (crc >>> 8) ^ c; } return ~crc >>> 0; }
function chunk(t, d) { const l = Buffer.alloc(4); l.writeUInt32BE(d.length); const td = Buffer.concat([Buffer.from(t), d]); const c = Buffer.alloc(4); c.writeUInt32BE(crc32(td)); return Buffer.concat([l, td, c]); }
for (const size of [180, 192, 512]) {
  const raw = Buffer.alloc((size * 3 + 1) * size);
  for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
    const px = Math.floor((x / size) * 16), py = Math.floor((y / size) * 16);
    let col = design[0][4];
    for (const [rx, ry, w, h, c] of design) if (px >= rx && px < rx + w && py >= ry && py < ry + h) col = c;
    raw.set(col, y * (size * 3 + 1) + 1 + x * 3);
  }
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(size, 0); ihdr.writeUInt32BE(size, 4); ihdr[8] = 8; ihdr[9] = 2;
  writeFileSync(new URL(`../public/icon-${size}.png`, import.meta.url), Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", ihdr), chunk("IDAT", deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]));
}
console.log("icons written");
