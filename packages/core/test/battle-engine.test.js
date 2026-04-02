import test from "node:test";
import assert from "node:assert/strict";
import { Jimp } from "jimp";

import {
  analyzePortraitBattle,
  generateBattleHtmlReport,
  writeBattleArtifacts
} from "../src/index.js";
import { createFixturePalette, createPortraitDataUrl } from "../../../test/support/portrait-fixture.js";

test("analyzePortraitBattle returns deterministic winner and axis cards", async () => {
  const leftSource = await createPortraitDataUrl(Jimp, createFixturePalette("left"));
  const rightSource = await createPortraitDataUrl(Jimp, createFixturePalette("right"));

  const resultOne = await analyzePortraitBattle({
    leftSource,
    rightSource,
    leftLabel: "Aurora",
    rightLabel: "Blaze"
  });
  const resultTwo = await analyzePortraitBattle({
    leftSource,
    rightSource,
    leftLabel: "Aurora",
    rightLabel: "Blaze"
  });

  assert.equal(resultOne.winner.id, resultTwo.winner.id);
  assert.equal(resultOne.axisCards.length, 6);
  assert.match(resultOne.sections.overallTake, /Aurora|Blaze/);
  assert.ok(resultOne.scores.left.total >= 0 && resultOne.scores.left.total <= 100);
  assert.ok(resultOne.scores.right.total >= 0 && resultOne.scores.right.total <= 100);
});

test("generateBattleHtmlReport renders winner-first shareable markup", async () => {
  const leftSource = await createPortraitDataUrl(Jimp, createFixturePalette("left"));
  const rightSource = await createPortraitDataUrl(Jimp, createFixturePalette("vivid"));

  const result = await analyzePortraitBattle({
    leftSource,
    rightSource,
    leftLabel: "Aurora",
    rightLabel: "Vivid"
  });

  const html = generateBattleHtmlReport(result);

  assert.match(html, /Winner/i);
  assert.match(html, /symmetry/i);
  assert.match(html, /why this won/i);
  assert.match(html, /data:image\/png;base64/);
});

test("writeBattleArtifacts persists html and json outputs", async () => {
  const leftSource = await createPortraitDataUrl(Jimp, createFixturePalette("left"));
  const rightSource = await createPortraitDataUrl(Jimp, createFixturePalette("right"));
  const result = await analyzePortraitBattle({
    leftSource,
    rightSource,
    leftLabel: "Aurora",
    rightLabel: "Blaze"
  });

  const artifacts = await writeBattleArtifacts(result, {
    outputDir: new URL("./tmp-artifacts/", import.meta.url)
  });

  assert.match(artifacts.htmlPath, /\.html$/);
  assert.match(artifacts.jsonPath, /\.json$/);
});
