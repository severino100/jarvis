import { writeFileSync, mkdirSync } from 'node:fs';
import { deflateSync } from 'node:zlib';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const OUT = join(dirname(fileURLToPath(import.meta.url)), '..', 'assets');
mkdirSync(OUT, { recursive: true });

const mix = (a, b, t) => a + (b - a) * t;
const hex = (h) => [parseInt(h.slice(1, 3), 16), parseInt(h.slice(3, 5), 16), parseInt(h.slice(5, 7), 16)];
const TOP = hex('#2a64b8');
const BOTTOM = hex('#0e2650');
const RING = hex('#ffffff');
const CORE = hex('#6fd3ff');

function roundedSquare(x, y, r) {
  const qx = Math.abs(x - 0.5) - (0.5 - r);
  const qy = Math.abs(y - 0.5) - (0.5 - r);
  return Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0) - r;
}

// Arc-reactor mark: a segmented white ring around a glowing core.
function sample(x, y) {
  if (roundedSquare(x, y, 0.22) > 0) return [0, 0, 0, 0];
  let c = TOP.map((v, i) => mix(v, BOTTOM[i], y));
  const dx = x - 0.5;
  const dy = y - 0.5;
  const r = Math.hypot(dx, dy);
  const ang = (Math.atan2(dy, dx) * 180) / Math.PI + 180;

  const glow = Math.max(0, 1 - r / 0.42) ** 2 * 0.35;
  c = c.map((v, i) => mix(v, CORE[i], glow));

  const inGap = [90, 210, 330].some((g) => Math.abs(((ang - g + 540) % 360) - 180) < 9);
  if (r > 0.255 && r < 0.335 && !inGap) c = RING;
  if (r > 0.19 && r < 0.215) c = c.map((v, i) => mix(v, RING[i], 0.55));
  if (r < 0.13) c = CORE.map((v, i) => mix(v, 255, Math.max(0, 1 - r / 0.07)));
  return [...c, 255];
}

function render(size) {
  const ss = size <= 32 ? 6 : 4;
  const px = Buffer.alloc(size * size * 4);
  for (let y = 0; y < size; y++)
    for (let x = 0; x < size; x++) {
      let acc = [0, 0, 0, 0];
      for (let sy = 0; sy < ss; sy++)
        for (let sx = 0; sx < ss; sx++) {
          const s = sample((x + (sx + 0.5) / ss) / size, (y + (sy + 0.5) / ss) / size);
          const a = s[3] / 255;
          acc = [acc[0] + s[0] * a, acc[1] + s[1] * a, acc[2] + s[2] * a, acc[3] + a];
        }
      const n = ss * ss;
      const a = acc[3] / n;
      const o = (y * size + x) * 4;
      px[o] = a ? Math.round(acc[0] / acc[3]) : 0;
      px[o + 1] = a ? Math.round(acc[1] / acc[3]) : 0;
      px[o + 2] = a ? Math.round(acc[2] / acc[3]) : 0;
      px[o + 3] = Math.round(a * 255);
    }
  return png(size, px);
}

const CRC = new Int32Array(256).map((_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c;
});
function crc32(buf) {
  let c = -1;
  for (const b of buf) c = CRC[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ -1) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const td = Buffer.concat([Buffer.from(type), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(td));
  return Buffer.concat([len, td, crc]);
}
function png(size, px) {
  const raw = Buffer.alloc(size * (size * 4 + 1));
  for (let y = 0; y < size; y++) px.copy(raw, y * (size * 4 + 1) + 1, y * size * 4, (y + 1) * size * 4);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('IDAT', deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}

const sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256];
const pngs = sizes.map((s) => [s, render(s)]);
for (const [s, buf] of pngs) if ([64, 256].includes(s)) writeFileSync(join(OUT, `icon-${s}.png`), buf);

const header = Buffer.alloc(6);
header.writeUInt16LE(1, 2);
header.writeUInt16LE(pngs.length, 4);
let offset = 6 + 16 * pngs.length;
const entries = pngs.map(([s, buf]) => {
  const e = Buffer.alloc(16);
  e[0] = s === 256 ? 0 : s;
  e[1] = s === 256 ? 0 : s;
  e.writeUInt16LE(1, 4);
  e.writeUInt16LE(32, 6);
  e.writeUInt32LE(buf.length, 8);
  e.writeUInt32LE(offset, 12);
  offset += buf.length;
  return e;
});
writeFileSync(join(OUT, 'icon.ico'), Buffer.concat([header, ...entries, ...pngs.map(([, b]) => b)]));
console.log('assets/icon.ico, icon-64.png, icon-256.png');
