const ANSI = {
  reset: "\u001b[0m",
  bold: "\u001b[1m",
  amber: "\u001b[38;5;215m",
  cyan: "\u001b[38;5;80m",
  blue: "\u001b[38;5;111m",
  dim: "\u001b[38;5;246m"
};

function paint(text, color, enabled = true) {
  return enabled ? `${color}${text}${ANSI.reset}` : text;
}

function meter(score) {
  const filled = Math.max(1, Math.round(score / 5));
  return `[${"#".repeat(filled)}${".".repeat(20 - filled)}]`;
}

function signedGap(card, winnerId) {
  if (card.leader === "tie") {
    return "TIE ";
  }

  const sign = card.leader === winnerId ? "+" : "-";
  return `${sign}${card.diff.toFixed(1)}`.padStart(4);
}

export function renderTerminalBattle(result, artifacts, options = {}) {
  const color = options.color !== false;
  const lines = [];
  const divider = "=".repeat(84);
  const softDivider = "-".repeat(84);
  const winnerColor = result.winner.id === "left" ? ANSI.amber : ANSI.blue;

  lines.push(paint(divider, ANSI.dim, color));
  lines.push(paint("BETTERTHANYOU // CLI PORTRAIT BATTLE", ANSI.bold, color));
  lines.push(paint(divider, ANSI.dim, color));
  lines.push(paint(`WINNER : ${result.winner.label.toUpperCase()}`, winnerColor, color));
  lines.push(`TOTAL  : ${result.inputs.left.label} ${result.scores.left.total.toFixed(1)}  vs  ${result.inputs.right.label} ${result.scores.right.total.toFixed(1)}`);
  lines.push(`MARGIN : ${result.winner.margin.toFixed(1)} points`);
  lines.push(softDivider);
  lines.push(paint("ABILITY COMPARISON", ANSI.cyan, color));

  for (const card of result.axisCards) {
    lines.push(`${card.label.padEnd(24)} ${String(card.left.toFixed(1)).padStart(5)} ${meter(card.left)} | ${String(card.right.toFixed(1)).padStart(5)} ${meter(card.right)}  ${signedGap(card, result.winner.id)}`);
  }

  lines.push(softDivider);
  lines.push(paint("OVERALL TAKE", ANSI.cyan, color));
  lines.push(result.sections.overallTake);
  lines.push(softDivider);
  lines.push(paint("WHY THIS WON", ANSI.cyan, color));
  lines.push(result.sections.whyThisWon);
  lines.push(softDivider);
  lines.push(paint("SAVE FILES", ANSI.cyan, color));
  lines.push(paint(`HTML report : ${artifacts.htmlPath}`, ANSI.dim, color));
  lines.push(paint(`JSON result : ${artifacts.jsonPath}`, ANSI.dim, color));
  lines.push(paint(divider, ANSI.dim, color));

  return lines.join("\n");
}

export function renderReportSummary(report, options = {}) {
  const color = options.color !== false;
  const lines = [];
  lines.push(paint("BETTERTHANYOU // REPORT REBUILT", ANSI.bold, color));
  lines.push(paint(`HTML report : ${report.htmlPath}`, ANSI.dim, color));
  lines.push(paint(`JSON result : ${report.jsonPath}`, ANSI.dim, color));
  return lines.join("\n");
}

export function renderOpenSummary(targetPath, options = {}) {
  const color = options.color !== false;
  return paint(`Opened: ${targetPath}`, ANSI.dim, color);
}
