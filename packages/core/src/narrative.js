import { AXIS_DEFINITIONS } from "./contracts.js";

function rankAxes(axes) {
  return [...AXIS_DEFINITIONS]
    .map(axis => ({ ...axis, value: axes[axis.key] }))
    .sort((left, right) => right.value - left.value);
}

export function buildBattleNarrative({ left, right, leftScores, rightScores, winner, axisCards }) {
  const leftRanked = rankAxes(leftScores.axes);
  const rightRanked = rankAxes(rightScores.axes);
  const leadAxes = axisCards.filter(card => card.leader === winner.id);
  const decisiveLead = leadAxes.sort((leftCard, rightCard) => rightCard.diff - leftCard.diff)[0];
  const marginWord = winner.margin >= 8 ? "clear" : winner.margin >= 4 ? "controlled" : "narrow";

  return {
    overallTake: `${winner.label} takes the battle with a ${marginWord} edge, landing at ${winner.totalScore} to ${winner.opponentScore}. The biggest pressure points were ${decisiveLead.label.toLowerCase()} and the overall style read.` ,
    strengths: {
      left: `${left.label} peaks in ${leftRanked[0].label.toLowerCase()} and ${leftRanked[1].label.toLowerCase()}, giving the portrait a confident first read in the side-by-side.` ,
      right: `${right.label} shows its best form in ${rightRanked[0].label.toLowerCase()} and ${rightRanked[1].label.toLowerCase()}, which keeps the matchup competitive even when it loses.`
    },
    weaknesses: {
      left: `${left.label} leaves points on the table in ${leftRanked.at(-1).label.toLowerCase()} and ${leftRanked.at(-2).label.toLowerCase()}, so its lower-end moments feel less polished.` ,
      right: `${right.label} loses ground most visibly in ${rightRanked.at(-1).label.toLowerCase()} and ${rightRanked.at(-2).label.toLowerCase()}, which softens the overall punch.`
    },
    whyThisWon: `${winner.label} won because it led ${leadAxes.length} of 6 axes and created its best separation in ${decisiveLead.label.toLowerCase()} by ${decisiveLead.diff.toFixed(1)} points.` ,
    modelJuryNotes: `Jury notes are heuristic-only in v1. The engine is deterministic, favors centered portrait presence, and treats totals within 2.5 points as near toss-up territory.`
  };
}
