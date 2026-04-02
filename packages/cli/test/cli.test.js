import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";
import { Jimp } from "jimp";

import { createFixturePalette, createPortraitDataUrl } from "../../../test/support/portrait-fixture.js";

async function createPortraitFiles(workDir) {
  const leftDataUrl = await createPortraitDataUrl(Jimp, createFixturePalette("left"));
  const rightDataUrl = await createPortraitDataUrl(Jimp, createFixturePalette("right"));
  const leftBase64 = leftDataUrl.split(",")[1];
  const rightBase64 = rightDataUrl.split(",")[1];

  const leftImage = await Jimp.read(Buffer.from(leftBase64, "base64"));
  const rightImage = await Jimp.read(Buffer.from(rightBase64, "base64"));
  const leftPath = join(workDir, "left.png");
  const rightPath = join(workDir, "right.png");

  await leftImage.write(leftPath);
  await rightImage.write(rightPath);

  return { leftDataUrl, rightDataUrl, leftPath, rightPath };
}

function runCli(args, options = {}) {
  const repoRoot = fileURLToPath(new URL("../../..", import.meta.url));

  return new Promise((resolve, reject) => {
    const child = spawn(
      process.execPath,
      ["packages/cli/bin/better-than-you.js", ...args],
      {
        cwd: repoRoot,
        env: { ...process.env, FORCE_COLOR: "0", ...options.env }
      }
    );

    let stdout = "";
    let stderr = "";

    child.stdout.on("data", chunk => {
      stdout += chunk.toString();
    });

    child.stderr.on("data", chunk => {
      stderr += chunk.toString();
    });

    if (options.stdin) {
      child.stdin.write(options.stdin);
      child.stdin.end();
    }

    child.on("exit", code => {
      if (code !== 0) {
        reject(new Error(stderr || stdout || `CLI exited with ${code}`));
        return;
      }

      resolve({ stdout, stderr });
    });
  });
}

test("CLI prints winner banner and writes reports from direct paths", async () => {
  const workDir = await mkdtemp(join(tmpdir(), "bty-cli-"));
  const { leftPath, rightPath } = await createPortraitFiles(workDir);

  const output = await runCli([
    leftPath,
    rightPath,
    "--out-dir",
    workDir
  ]);

  assert.match(output.stdout, /WINNER/i);
  assert.match(output.stdout, /ABILITY COMPARISON/i);

  const generated = await readFile(join(workDir, "latest-battle.json"), "utf8");
  assert.match(generated, /winner/i);
});

test("CLI accepts piped portrait paths in implicit battle mode", async () => {
  const workDir = await mkdtemp(join(tmpdir(), "bty-guided-"));
  const { leftPath, rightPath } = await createPortraitFiles(workDir);

  const output = await runCli([
    "--out-dir",
    workDir
  ], {
    stdin: `${leftPath}\n${rightPath}\n`
  });

  assert.match(output.stdout, /WINNER/i);
  assert.match(output.stdout, /ABILITY COMPARISON/i);
});

test("CLI can source both portraits from clipboard env overrides and emit JSON", async () => {
  const workDir = await mkdtemp(join(tmpdir(), "bty-clipboard-"));
  const { leftDataUrl, rightDataUrl } = await createPortraitFiles(workDir);

  const output = await runCli([
    "battle",
    "--left-clipboard",
    "--right-clipboard",
    "--json",
    "--out-dir",
    workDir
  ], {
    env: {
      BTY_CLIPBOARD_LEFT: leftDataUrl,
      BTY_CLIPBOARD_RIGHT: rightDataUrl
    }
  });

  const parsed = JSON.parse(output.stdout);
  assert.equal(parsed.result.winner_first, true);
  assert.match(parsed.artifacts.htmlPath, /\.html$/);
});

test("CLI can rebuild a report from saved JSON", async () => {
  const workDir = await mkdtemp(join(tmpdir(), "bty-report-"));
  const { leftPath, rightPath } = await createPortraitFiles(workDir);

  await runCli([
    leftPath,
    rightPath,
    "--out-dir",
    workDir
  ]);

  const report = await runCli([
    "report",
    join(workDir, "latest-battle.json"),
    "--out-dir",
    workDir
  ]);

  assert.match(report.stdout, /REPORT REBUILT/i);
});
