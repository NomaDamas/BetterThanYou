import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Jimp } from "jimp";

import { executeBattleTool, executeReportTool } from "../src/tools.js";
import { createFixturePalette, createPortraitDataUrl } from "../../../test/support/portrait-fixture.js";

test("battle tool returns structured result and artifacts", async () => {
  const outputDir = await mkdtemp(join(tmpdir(), "bty-mcp-"));
  const leftSource = await createPortraitDataUrl(Jimp, createFixturePalette("left"));
  const rightSource = await createPortraitDataUrl(Jimp, createFixturePalette("right"));

  const result = await executeBattleTool({
    leftSource,
    rightSource,
    leftLabel: "Aurora",
    rightLabel: "Blaze",
    outputDir
  });

  assert.equal(result.result.winner_first, true);
  assert.match(result.artifacts.htmlPath, /\.html$/);
});

test("report tool rebuilds html report from saved json", async () => {
  const outputDir = await mkdtemp(join(tmpdir(), "bty-report-"));
  const leftSource = await createPortraitDataUrl(Jimp, createFixturePalette("left"));
  const rightSource = await createPortraitDataUrl(Jimp, createFixturePalette("vivid"));

  const battle = await executeBattleTool({
    leftSource,
    rightSource,
    leftLabel: "Aurora",
    rightLabel: "Vivid",
    outputDir
  });

  const report = await executeReportTool({
    battleJsonPath: battle.artifacts.jsonPath,
    outputDir
  });

  assert.match(report.htmlPath, /\.html$/);
});
