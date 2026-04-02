import { access, readFile } from "node:fs/promises";
import { constants as fsConstants } from "node:fs";

import { Jimp } from "jimp";

import { deriveLabel, hashBuffer, inferMimeType } from "./util.js";

function isHttpUrl(value) {
  return /^https?:\/\//i.test(value);
}

function isDataUrl(value) {
  return /^data:image\//i.test(value);
}

function looksLikeBase64(value) {
  return /^[A-Za-z0-9+/=\s]+$/.test(value) && value.length > 96;
}

async function exists(path) {
  try {
    await access(path, fsConstants.F_OK);
    return true;
  } catch {
    return false;
  }
}

async function loadSourceBuffer(source) {
  if (isDataUrl(source)) {
    const [header, encoded] = source.split(",");
    const mimeType = header.slice(5, header.indexOf(";"));
    return {
      buffer: Buffer.from(encoded, "base64"),
      mimeType,
      sourceType: "data-url",
      resolvedSource: source.slice(0, 32) + "..."
    };
  }

  if (isHttpUrl(source)) {
    const response = await fetch(source);
    if (!response.ok) {
      throw new Error(`Failed to fetch ${source}: HTTP ${response.status}`);
    }

    return {
      buffer: Buffer.from(await response.arrayBuffer()),
      mimeType: inferMimeType(response.headers.get("content-type") || source),
      sourceType: "url",
      resolvedSource: source
    };
  }

  if (await exists(source)) {
    return {
      buffer: await readFile(source),
      mimeType: inferMimeType(source),
      sourceType: "path",
      resolvedSource: source
    };
  }

  if (looksLikeBase64(source)) {
    return {
      buffer: Buffer.from(source.replace(/\s+/g, ""), "base64"),
      mimeType: "image/png",
      sourceType: "base64",
      resolvedSource: "inline-base64"
    };
  }

  throw new Error(`Unsupported portrait input: ${source}`);
}

export async function loadPortraitInput(source, label, side) {
  const loaded = await loadSourceBuffer(source);
  const image = await Jimp.read(loaded.buffer);
  const hash = hashBuffer(loaded.buffer);
  const finalLabel = label || deriveLabel(source, side === "left" ? "Left Portrait" : "Right Portrait");

  return {
    id: side,
    label: finalLabel,
    sourceType: loaded.sourceType,
    originalSource: source,
    resolvedSource: loaded.resolvedSource,
    mimeType: loaded.mimeType,
    width: image.bitmap.width,
    height: image.bitmap.height,
    hash,
    imageDataUrl: `data:${loaded.mimeType};base64,${loaded.buffer.toString("base64")}`,
    image
  };
}
