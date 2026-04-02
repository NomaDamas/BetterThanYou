import { createHash } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { basename, extname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

export function round(value, precision = 1) {
  const factor = 10 ** precision;
  return Math.round(value * factor) / factor;
}

export function average(values) {
  if (!values.length) {
    return 0;
  }

  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

export function stddev(values) {
  if (values.length < 2) {
    return 0;
  }

  const mean = average(values);
  const variance = average(values.map(value => (value - mean) ** 2));
  return Math.sqrt(variance);
}

export function percentile(values, ratio) {
  if (!values.length) {
    return 0;
  }

  const sorted = [...values].sort((left, right) => left - right);
  const index = clamp(Math.floor((sorted.length - 1) * ratio), 0, sorted.length - 1);
  return sorted[index];
}

export function hashBuffer(buffer) {
  return createHash("sha256").update(buffer).digest("hex");
}

export function hashSignal(hash, index, scale = 4) {
  const offset = (index * 2) % hash.length;
  const slice = hash.slice(offset, offset + 2);
  return (Number.parseInt(slice, 16) / 255) * scale;
}

export function slugify(value) {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48) || "battle";
}

export function createBattleId(leftLabel, rightLabel) {
  const timestamp = new Date().toISOString().replace(/[.:]/g, "-");
  return `${timestamp}-${slugify(`${leftLabel}-${rightLabel}`)}`;
}

export function toFsPath(pathLike) {
  if (pathLike instanceof URL) {
    return fileURLToPath(pathLike);
  }

  return pathLike;
}

export async function ensureDirectory(pathLike) {
  const path = toFsPath(pathLike);
  await mkdir(path, { recursive: true });
  return path;
}

export function decodePixelInt(pixelInt) {
  return {
    r: (pixelInt >>> 24) & 255,
    g: (pixelInt >>> 16) & 255,
    b: (pixelInt >>> 8) & 255,
    a: pixelInt & 255
  };
}

export function inferMimeType(source, fallback = "image/png") {
  if (!source) {
    return fallback;
  }

  const normalized = String(source).toLowerCase();

  if (normalized.startsWith("data:")) {
    return normalized.slice(5, normalized.indexOf(";")) || fallback;
  }

  const extension = extname(normalized).replace(".", "");
  const lookup = {
    jpg: "image/jpeg",
    jpeg: "image/jpeg",
    png: "image/png",
    webp: "image/webp",
    gif: "image/gif",
    bmp: "image/bmp"
  };

  return lookup[extension] || fallback;
}

export function deriveLabel(source, fallback) {
  if (!source) {
    return fallback;
  }

  if (/^https?:\/\//i.test(source)) {
    try {
      const url = new URL(source);
      const segment = basename(url.pathname) || url.hostname;
      return segment.replace(extname(segment), "") || fallback;
    } catch {
      return fallback;
    }
  }

  if (source.startsWith("data:")) {
    return fallback;
  }

  return basename(resolve(source)).replace(extname(source), "") || fallback;
}
