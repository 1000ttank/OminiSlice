import { mkdir, writeFile } from 'node:fs/promises';
const size = 32;
const pixels = Buffer.alloc(size * size * 4);
for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
  const distance = Math.hypot(x - 15.5, y - 15.5);
  const ring = distance > 8 && distance < 12;
  pixels.set(ring ? [194, 207, 138, 255] : [32, 27, 21, 255], (y * size + x) * 4);
}
const dib = Buffer.alloc(40);
dib.writeUInt32LE(40, 0); dib.writeInt32LE(size, 4); dib.writeInt32LE(size * 2, 8);
dib.writeUInt16LE(1, 12); dib.writeUInt16LE(32, 14);
const image = Buffer.concat([dib, pixels, Buffer.alloc(size * size / 8)]);
const header = Buffer.alloc(22);
header.writeUInt16LE(1, 2); header.writeUInt16LE(1, 4);
header[6] = size; header[7] = size;
header.writeUInt16LE(1, 10); header.writeUInt16LE(32, 12);
header.writeUInt32LE(image.length, 14); header.writeUInt32LE(22, 18);
await mkdir(new URL('../src-tauri/icons/', import.meta.url), { recursive: true });
await writeFile(new URL('../src-tauri/icons/icon.ico', import.meta.url), Buffer.concat([header, image]));
