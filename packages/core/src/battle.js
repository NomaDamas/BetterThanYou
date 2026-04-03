import { AXIS_DEFINITIONS, ENGINE_VERSION, PRODUCT_NAME, QUALITATIVE_SECTION_KEYS } from "./contracts.js";
import { loadPortraitInput } from "./ingest.js";
import { scorePortrait } from "./metrics.js";
import { buildBattleNarrative } from "./narrative.js";
import { DEFAULT_OPENAI_MODEL, judgePortraitBattleWithOpenAI } from "./openai-judge.js";
import { createBattleId, round } from "./util.js";

function computeTotalFromAxes(axes) {
  const weightedTotal = AXIS_DEFINITIONS.reduce((sum, axis) => sum + axes[axis.key] * axis.weight, 0);
  const weightSum = AXIS_DEFINITIONS.reduce((sum, axis) => sum + axis.weight, 0);
  return round(weightedTotal / weightSum);
}

function buildScoreBundle(axes, telemetry = {}) {
  return {
    axes,
    total: computeTotalFromAxes(axes),
    telemetry
  };
}

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

function pickWinner({ leftPortrait, rightPortrait, leftScores, rightScores, axisCards, preferredWinnerId }) {
  if (preferredWinnerId === "left" || preferredWinnerId === "right") {
    return preferredWinnerId;
  }

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

function buildBattleResult({
  leftPortrait,
  rightPortrait,
  leftScores,
  rightScores,
  sections,
  judgeMode,
  engineVersion,
  provider,
  model,
  preferredWinnerId,
  fallbackReason
}) {
  const axisCards = buildAxisCards(leftScores, rightScores);
  const winnerId = pickWinner({
    leftPortrait,
    rightPortrait,
    leftScores,
    rightScores,
    axisCards,
    preferredWinnerId
  });
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

  const enrichedSections = {
    ...sections,
    modelJuryNotes: fallbackReason
      ? `${sections.modelJuryNotes} Fallback: ${fallbackReason}`
      : sections.modelJuryNotes
  };

  return {
    battleId,
    productName: PRODUCT_NAME,
    createdAt: new Date().toISOString(),
    engine: {
      version: engineVersion,
      qualitativeSections: QUALITATIVE_SECTION_KEYS,
      judgeMode,
      provider,
      model
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
    sections: enrichedSections
  };
}

export async function analyzePortraitBattle({
  leftSource,
  rightSource,
  leftLabel,
  rightLabel,
  judgeMode = "auto",
  openAIModel = DEFAULT_OPENAI_MODEL,
  openAIJudge = judgePortraitBattleWithOpenAI,
  openAIConfig = {}
}) {
  const leftPortrait = await loadPortraitInput(leftSource, leftLabel, "left");
  const rightPortrait = await loadPortraitInput(rightSource, rightLabel, "right");

  const openAIKey = openAIConfig.apiKey || process.env.BTY_OPENAI_API_KEY || process.env.OPENAI_API_KEY;
  const shouldUseOpenAI = judgeMode === "openai" || (judgeMode === "auto" && Boolean(openAIKey));

  if (shouldUseOpenAI) {
    try {
      const judged = await openAIJudge({
        leftPortrait,
        rightPortrait,
        model: openAIModel,
        ...openAIConfig
      });

      return buildBattleResult({
        leftPortrait,
        rightPortrait,
        leftScores: buildScoreBundle(judged.leftScores),
        rightScores: buildScoreBundle(judged.rightScores),
        sections: judged.sections,
        judgeMode: "openai",
        engineVersion: `openai-${judged.model}`,
        provider: judged.provider,
        model: judged.model,
        preferredWinnerId: judged.winnerId
      });
    } catch (error) {
      if (judgeMode === "openai") {
        throw error;
      }

      const leftScores = scorePortrait(leftPortrait);
      const rightScores = scorePortrait(rightPortrait);
      const sections = buildBattleNarrative({
        left: leftPortrait,
        right: rightPortrait,
        leftScores,
        rightScores,
        winner: {
          id: leftScores.total >= rightScores.total ? "left" : "right",
          label: leftScores.total >= rightScores.total ? leftPortrait.label : rightPortrait.label,
          totalScore: Math.max(leftScores.total, rightScores.total),
          opponentScore: Math.min(leftScores.total, rightScores.total),
          margin: round(Math.abs(leftScores.total - rightScores.total)),
          decisive: Math.abs(leftScores.total - rightScores.total) >= 6
        },
        axisCards: buildAxisCards(leftScores, rightScores)
      });

      return buildBattleResult({
        leftPortrait,
        rightPortrait,
        leftScores,
        rightScores,
        sections,
        judgeMode: "heuristic",
        engineVersion: ENGINE_VERSION,
        provider: "local",
        model: null,
        preferredWinnerId: null,
        fallbackReason: error instanceof Error ? error.message : String(error)
      });
    }
  }

  const leftScores = scorePortrait(leftPortrait);
  const rightScores = scorePortrait(rightPortrait);
  const axisCards = buildAxisCards(leftScores, rightScores);
  const heuristicWinnerId = pickWinner({ leftPortrait, rightPortrait, leftScores, rightScores, axisCards });
  const heuristicWinner = {
    id: heuristicWinnerId,
    label: heuristicWinnerId === "left" ? leftPortrait.label : rightPortrait.label,
    totalScore: heuristicWinnerId === "left" ? leftScores.total : rightScores.total,
    opponentScore: heuristicWinnerId === "left" ? rightScores.total : leftScores.total,
    margin: round(Math.abs(leftScores.total - rightScores.total)),
    decisive: Math.abs(leftScores.total - rightScores.total) >= 6
  };

  const sections = buildBattleNarrative({
    left: leftPortrait,
    right: rightPortrait,
    leftScores,
    rightScores,
    winner: heuristicWinner,
    axisCards
  });

  return buildBattleResult({
    leftPortrait,
    rightPortrait,
    leftScores,
    rightScores,
    sections,
    judgeMode: "heuristic",
    engineVersion: ENGINE_VERSION,
    provider: "local",
    model: null,
    preferredWinnerId: heuristicWinnerId,
    fallbackReason: judgeMode === "auto" ? "No OPENAI_API_KEY detected. Using heuristic judge." : undefined
  });
}
