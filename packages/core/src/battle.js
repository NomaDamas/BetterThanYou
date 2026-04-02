import { AXIS_DEFINITIONS, ENGINE_VERSION, PRODUCT_NAME, QUALITATIVE_SECTION_KEYS } from "./contracts.js";
import { loadPortraitInput } from "./ingest.js";
import { scorePortrait } from "./metrics.js";
import { buildBattleNarrative } from "./narrative.js";
import { createBattleId, round } from "./util.js";

function buildAxisCards(leftScores, rightScores) {
  return AXIS_DEFINITIONS.map(axis => {
    const left = leftScores.axes[axis.key];
    const right = rightScores.axes[axis.key];
    const diff = round(Math.abs(left - right));
    const leader = left === right ? "tie" : left > right ? "left" : "right";

    return {
      key: axis.key,
      label: axis.label,
      left,
      right,
      diff,
      leader
    };
  });
}

function pickWinner(leftPortrait, rightPortrait, leftScores, rightScores, axisCards) {
  const totalDiff = round(Math.abs(leftScores.total - rightScores.total));
  if (leftScores.total === rightScores.total) {
    const leftLeads = axisCards.filter(card => card.leader === "left").length;
    const rightLeads = axisCards.filter(card => card.leader === "right").length;
    if (leftLeads === rightLeads) {
      return leftPortrait.hash > rightPortrait.hash ? leftPortrait.id : rightPortrait.id;
    }
    return leftLeads > rightLeads ? leftPortrait.id : rightPortrait.id;
  }

  return leftScores.total > rightScores.total ? leftPortrait.id : rightPortrait.id;
}

export async function analyzePortraitBattle({
  leftSource,
  rightSource,
  leftLabel,
  rightLabel
}) {
  const leftPortrait = await loadPortraitInput(leftSource, leftLabel, "left");
  const rightPortrait = await loadPortraitInput(rightSource, rightLabel, "right");
  const leftScores = scorePortrait(leftPortrait);
  const rightScores = scorePortrait(rightPortrait);
  const axisCards = buildAxisCards(leftScores, rightScores);
  const winnerId = pickWinner(leftPortrait, rightPortrait, leftScores, rightScores, axisCards);
  const winnerPortrait = winnerId === "left" ? leftPortrait : rightPortrait;
  const winnerScores = winnerId === "left" ? leftScores : rightScores;
  const opponentScores = winnerId === "left" ? rightScores : leftScores;
  const battleId = createBattleId(leftPortrait.label, rightPortrait.label);

  const winner = {
    id: winnerPortrait.id,
    label: winnerPortrait.label,
    totalScore: winnerScores.total,
    opponentScore: opponentScores.total,
    margin: round(Math.abs(winnerScores.total - opponentScores.total)),
    decisive: Math.abs(winnerScores.total - opponentScores.total) >= 6
  };

  const sections = buildBattleNarrative({
    left: leftPortrait,
    right: rightPortrait,
    leftScores,
    rightScores,
    winner,
    axisCards
  });

  return {
    battleId,
    productName: PRODUCT_NAME,
    createdAt: new Date().toISOString(),
    engine: {
      version: ENGINE_VERSION,
      qualitativeSections: QUALITATIVE_SECTION_KEYS
    },
    winner_first: true,
    quantitative_axes: AXIS_DEFINITIONS.map(axis => axis.key),
    qualitative_sections: QUALITATIVE_SECTION_KEYS,
    inputs: {
      left: {
        id: leftPortrait.id,
        label: leftPortrait.label,
        sourceType: leftPortrait.sourceType,
        width: leftPortrait.width,
        height: leftPortrait.height,
        hash: leftPortrait.hash,
        imageDataUrl: leftPortrait.imageDataUrl
      },
      right: {
        id: rightPortrait.id,
        label: rightPortrait.label,
        sourceType: rightPortrait.sourceType,
        width: rightPortrait.width,
        height: rightPortrait.height,
        hash: rightPortrait.hash,
        imageDataUrl: rightPortrait.imageDataUrl
      }
    },
    scores: {
      left: leftScores,
      right: rightScores
    },
    axisCards,
    winner,
    sections
  };
}
