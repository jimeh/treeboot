import { gunzipSync } from "node:zlib";

export interface TarEntry {
  readonly bytes: Buffer;
  readonly mode: number;
  readonly name: string;
}

export function readTarGzip(bytes: Buffer): readonly TarEntry[] {
  const tar = gunzipSync(bytes);
  const entries: TarEntry[] = [];
  let offset = 0;
  while (offset + 512 <= tar.length) {
    const header = tar.subarray(offset, offset + 512);
    if (header.every((byte) => byte === 0)) {
      break;
    }
    const name = readString(header, 0, 100);
    const prefix = readString(header, 345, 155);
    const fullName = prefix.length === 0 ? name : `${prefix}/${name}`;
    const mode = readOctal(header, 100, 8);
    const size = readOctal(header, 124, 12);
    const type = String.fromCharCode(header[156] ?? 0);
    const contentsStart = offset + 512;
    if (type === "0" || type === "\0") {
      entries.push({
        bytes: tar.subarray(contentsStart, contentsStart + size),
        mode,
        name: fullName,
      });
    }
    offset = contentsStart + Math.ceil(size / 512) * 512;
  }
  return entries;
}

function readString(buffer: Buffer, offset: number, length: number): string {
  const end = buffer.indexOf(0, offset);
  const boundedEnd = end < 0 || end > offset + length ? offset + length : end;
  return buffer.toString("utf8", offset, boundedEnd);
}

function readOctal(buffer: Buffer, offset: number, length: number): number {
  const value = readString(buffer, offset, length).trim();
  return value.length === 0 ? 0 : Number.parseInt(value, 8);
}
