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
    rightLabel: "Blaze",
    judgeMode: "heuristic"
  });
  const resultTwo = await analyzePortraitBattle({
    leftSource,
    rightSource,
    leftLabel: "Aurora",
    rightLabel: "Blaze",
    judgeMode: "heuristic"
  });

  assert.equal(resultOne.winner.id, resultTwo.winner.id);
  assert.equal(resultOne.axisCards.length, 6);
  assert.match(resultOne.sections.overallTake, /Aurora|Blaze/);
  assert.equal(resultOne.engine.judgeMode, "heuristic");
  assert.ok(resultOne.scores.left.total >= 0 && resultOne.scores.left.total <= 100);
  assert.ok(resultOne.scores.right.total >= 0 && resultOne.scores.right.total <= 100);
});

test("analyzePortraitBattle can use an injected OpenAI judge", async () => {
  const leftSource = await createPortraitDataUrl(Jimp, createFixturePalette("left"));
  const rightSource = await createPortraitDataUrl(Jimp, createFixturePalette("vivid"));

  const result = await analyzePortraitBattle({
    leftSource,
    rightSource,
    leftLabel: "Aurora",
    rightLabel: "Nova",
    judgeMode: "openai",
    openAIModel: "gpt-4.1-mini",
    openAIJudge: async () => ({
      winnerId: "right",
      leftScores: {
        symmetry_harmony: 76,
        lighting_contrast: 62,
        sharpness_detail: 59,
        color_vitality: 58,
        composition_presence: 64,
        style_aura: 61
      },
      rightScores: {
        symmetry_harmony: 81,
        lighting_contrast: 79,
        sharpness_detail: 73,
        color_vitality: 92,
        composition_presence: 84,
        style_aura: 89
      },
      sections: {
        overallTake: "Nova wins on stronger color control and style cohesion.",
        strengths: {
          left: "Aurora keeps cleaner structure than expected.",
          right: "Nova lands richer color and a stronger editorial read."
        },
        weaknesses: {
          left: "Aurora feels flatter and less vibrant.",
          right: "Nova is slightly less balanced in symmetry."
        },
        whyThisWon: "Nova built decisive separation in color vitality and style aura.",
        modelJuryNotes: "Evaluated by a stubbed VLM judge in test mode."
      },
      provider: "openai",
      model: "gpt-4.1-mini"
    })
  });

  assert.equal(result.engine.judgeMode, "openai");
  assert.equal(result.winner.id, "right");
  assert.match(result.sections.modelJuryNotes, /stubbed VLM judge/i);
  assert.equal(result.scores.right.axes.color_vitality, 92);
});

test("generateBattleHtmlReport renders winner-first shareable markup", async () => {
  const leftSource = await createPortraitDataUrl(Jimp, createFixturePalette("left"));
  const rightSource = await createPortraitDataUrl(Jimp, createFixturePalette("vivid"));

  const result = await analyzePortraitBattle({
    leftSource,
    rightSource,
    leftLabel: "Aurora",
    rightLabel: "Vivid",
    judgeMode: "heuristic"
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
    rightLabel: "Blaze",
    judgeMode: "heuristic"
  });

  const artifacts = await writeBattleArtifacts(result, {
    outputDir: new URL("./tmp-artifacts/", import.meta.url)
  });

  assert.match(artifacts.htmlPath, /\.html$/);
  assert.match(artifacts.jsonPath, /\.json$/);
});
