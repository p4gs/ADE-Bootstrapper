#!/usr/bin/env bun
// Generates the app icon source: assets/icon-1024.png.
//
// Pure Bun — PNG chunks written by hand, IDAT via Bun.deflateSync, pixels
// from signed-distance fields (rounded rect + a check polyline with round
// caps). Flat colors, no text, no gradients, no emoji: a deep-navy tile with
// the product's one gesture — the check that means "your environment is
// sound". Deterministic output: same script, same bytes.
//
// Run: bun scripts/make-icon.ts   (then scripts/bundle-apps.sh turns the PNG
// into an .icns via sips + iconutil at bundle time)

const SIZE = 1024;
// Apple's modern icon grid: content tile inset from the canvas edge, corner
// radius ~22.6% of the tile.
const INSET = 100;
const TILE = SIZE - 2 * INSET;
const RADIUS = TILE * 0.226;

// Deep navy tile, white mark. The navy leans toward the app accent's hue
// without being the accent itself; the mark carries all the contrast.
const BG = [19, 26, 40]; // #131A28
const MARK = [255, 255, 255];

// The check, in tile-relative coordinates (y down).
const CHECK: [number, number][] = [
  [0.28, 0.54],
  [0.44, 0.70],
  [0.74, 0.34],
];
const STROKE = TILE * 0.085;

function sdRoundedRect(px: number, py: number): number {
  const cx = SIZE / 2;
  const cy = SIZE / 2;
  const hx = TILE / 2 - RADIUS;
  const hy = TILE / 2 - RADIUS;
  const dx = Math.abs(px - cx) - hx;
  const dy = Math.abs(py - cy) - hy;
  const ox = Math.max(dx, 0);
  const oy = Math.max(dy, 0);
  return Math.hypot(ox, oy) + Math.min(Math.max(dx, dy), 0) - RADIUS;
}

function sdSegment(px: number, py: number, a: [number, number], b: [number, number]): number {
  const ax = INSET + a[0] * TILE;
  const ay = INSET + a[1] * TILE;
  const bx = INSET + b[0] * TILE;
  const by = INSET + b[1] * TILE;
  const abx = bx - ax;
  const aby = by - ay;
  const t = Math.max(0, Math.min(1, ((px - ax) * abx + (py - ay) * aby) / (abx * abx + aby * aby)));
  return Math.hypot(px - (ax + t * abx), py - (ay + t * aby));
}

function sdCheck(px: number, py: number): number {
  let d = Infinity;
  for (let i = 0; i + 1 < CHECK.length; i++) {
    d = Math.min(d, sdSegment(px, py, CHECK[i]!, CHECK[i + 1]!));
  }
  return d - STROKE / 2;
}

// 1px-wide smoothstep antialiasing on both fields.
function coverage(d: number): number {
  return Math.max(0, Math.min(1, 0.5 - d));
}

const raw = new Uint8Array(SIZE * (1 + SIZE * 4));
let offset = 0;
for (let y = 0; y < SIZE; y++) {
  raw[offset++] = 0; // filter: none
  for (let x = 0; x < SIZE; x++) {
    const px = x + 0.5;
    const py = y + 0.5;
    const tile = coverage(sdRoundedRect(px, py));
    const mark = coverage(sdCheck(px, py)) * tile;
    const r = BG[0]! * (1 - mark) + MARK[0]! * mark;
    const g = BG[1]! * (1 - mark) + MARK[1]! * mark;
    const b = BG[2]! * (1 - mark) + MARK[2]! * mark;
    raw[offset++] = Math.round(r);
    raw[offset++] = Math.round(g);
    raw[offset++] = Math.round(b);
    raw[offset++] = Math.round(tile * 255);
  }
}

const crcTable = new Uint32Array(256).map((_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
function crc32(bytes: Uint8Array): number {
  let c = 0xffffffff;
  for (const byte of bytes) c = crcTable[(c ^ byte) & 0xff]! ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type: string, data: Uint8Array): Uint8Array {
  const out = new Uint8Array(12 + data.length);
  const view = new DataView(out.buffer);
  view.setUint32(0, data.length);
  out.set(new TextEncoder().encode(type), 4);
  out.set(data, 8);
  const crcInput = out.subarray(4, 8 + data.length);
  view.setUint32(8 + data.length, crc32(crcInput));
  return out;
}

const ihdr = new Uint8Array(13);
new DataView(ihdr.buffer).setUint32(0, SIZE);
new DataView(ihdr.buffer).setUint32(4, SIZE);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // RGBA

// PNG's IDAT must be ZLIB-wrapped (RFC1950). Bun.deflateSync emits RAW
// deflate (RFC1951) — wrap it by hand: 2-byte header + stream + adler32.
function adler32(bytes: Uint8Array): number {
  let a = 1;
  let b = 0;
  for (const byte of bytes) {
    a = (a + byte) % 65521;
    b = (b + a) % 65521;
  }
  return ((b << 16) | a) >>> 0;
}
const deflated = Bun.deflateSync(raw);
const idat = new Uint8Array(2 + deflated.length + 4);
idat[0] = 0x78;
idat[1] = 0x9c;
idat.set(deflated, 2);
new DataView(idat.buffer).setUint32(2 + deflated.length, adler32(raw));
const png = new Uint8Array([
  ...[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a],
  ...chunk("IHDR", ihdr),
  ...chunk("IDAT", idat),
  ...chunk("IEND", new Uint8Array(0)),
]);

await Bun.write("assets/icon-1024.png", png);
console.log(`wrote assets/icon-1024.png (${png.length} bytes)`);
