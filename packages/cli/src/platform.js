import { execFile as execFileCallback } from "node:child_process";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execFile = promisify(execFileCallback);

export function defaultReportsDir() {
  return resolve(fileURLToPath(new URL("../../../reports/", import.meta.url)));
}

export function normalizeSourceInput(rawValue) {
  let value = String(rawValue || "").trim();
  if (!value) {
    return value;
  }

  const quoteWrapped = (
    (value.startsWith('"') && value.endsWith('"')) ||
    (value.startsWith("'") && value.endsWith("'"))
  );

  if (quoteWrapped) {
    value = value.slice(1, -1);
  }

  value = value
    .replace(/\\ /g, " ")
    .replace(/\\([()\[\]{}'\"])/g, "$1");

  if (value.startsWith("~/")) {
    value = join(homedir(), value.slice(2));
  }

  if (value.startsWith("file://")) {
    try {
      return fileURLToPath(value);
    } catch {
      return value;
    }
  }

  return value;
}

export async function readClipboardSource(side, env = process.env) {
  const envKey = `BTY_CLIPBOARD_${side.toUpperCase()}`;
  if (env[envKey]) {
    return normalizeSourceInput(env[envKey]);
  }

  if (env.BTY_CLIPBOARD_VALUE) {
    return normalizeSourceInput(env.BTY_CLIPBOARD_VALUE);
  }

  if (process.platform === "darwin") {
    const { stdout } = await execFile("pbpaste", []);
    const value = normalizeSourceInput(stdout);
    if (!value) {
      throw new Error(`Clipboard was empty for ${side} portrait.`);
    }
    return value;
  }

  throw new Error(`Clipboard input is only implemented for macOS right now. Set ${envKey} to test or override it.`);
}

export async function openPathTarget(targetPath, env = process.env) {
  if (env.BTY_SKIP_OPEN === "1") {
    return targetPath;
  }

  const opener = env.BTY_OPEN_BIN || (process.platform === "darwin" ? "open" : "xdg-open");
  await execFile(opener, [targetPath]);
  return targetPath;
}
