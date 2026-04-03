import { analyzePortraitBattle, regenerateBattleReport, writeBattleArtifacts } from "@better-than-you/core";

function stripEmbeddedImages(result) {
  return {
    ...result,
    inputs: {
      left: {
        ...result.inputs.left,
        imageDataUrl: undefined
      },
      right: {
        ...result.inputs.right,
        imageDataUrl: undefined
      }
    }
  };
}

export async function executeBattleTool({
  leftSource,
  rightSource,
  leftLabel,
  rightLabel,
  judgeMode,
  openAIModel,
  outputDir
}) {
  const result = await analyzePortraitBattle({
    leftSource,
    rightSource,
    leftLabel,
    rightLabel,
    judgeMode,
    openAIModel
  });
  const artifacts = await writeBattleArtifacts(result, { outputDir });

  return {
    result: stripEmbeddedImages(result),
    artifacts
  };
}

export async function executeReportTool({ battleJsonPath, outputDir }) {
  const report = await regenerateBattleReport(battleJsonPath, { outputDir });
  return {
    htmlPath: report.htmlPath,
    jsonPath: report.jsonPath,
    latestHtmlPath: report.latestHtmlPath,
    latestJsonPath: report.latestJsonPath
  };
}
