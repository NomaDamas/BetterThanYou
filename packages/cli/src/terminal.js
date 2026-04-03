import readline from "node:readline";

const ANSI = {
  reset: "\u001b[0m",
  bold: "\u001b[1m",
  amber: "\u001b[38;5;215m",
  cyan: "\u001b[38;5;80m",
  blue: "\u001b[38;5;111m",
  dim: "\u001b[38;5;246m",
  green: "\u001b[38;5;120m",
  red: "\u001b[38;5;203m"
};

function paint(text, color, enabled = true) {
  return enabled ? `${color}${text}${ANSI.reset}` : text;
}

function meter(score) {
  const filled = Math.max(1, Math.round(score / 5));
  return `${"█".repeat(filled)}${"░".repeat(20 - filled)}`;
}

function padCenter(value, width) {
  const text = String(value);
  const totalPadding = Math.max(0, width - text.length);
  const left = Math.floor(totalPadding / 2);
  const right = totalPadding - left;
  return `${" ".repeat(left)}${text}${" ".repeat(right)}`;
}

function boxedTitle(title, color, width = 84, enabled = true) {
  const inner = Math.max(0, width - 4);
  const centered = padCenter(title, inner);
  return [
    paint(`╔${"═".repeat(inner)}╗`, color, enabled),
    paint(`║ ${centered.slice(1, inner - 1)} ║`, color, enabled),
    paint(`╚${"═".repeat(inner)}╝`, color, enabled)
  ];
}

function signedGap(card, winnerId) {
  if (card.leader === "tie") {
    return { text: "TIE ", color: ANSI.cyan };
  }

  const sign = card.leader === winnerId ? "+" : "-";
  const color = card.leader === winnerId ? ANSI.green : ANSI.red;
  return {
    text: `${sign}${card.diff.toFixed(1)}`.padStart(5),
    color
  };
}

export function renderTerminalBattle(result, artifacts, options = {}) {
  const color = options.color !== false;
  const lines = [];
  const width = 84;
  const winnerColor = result.winner.id === "left" ? ANSI.amber : ANSI.blue;
  const winnerBanner = boxedTitle(`WINNER // ${result.winner.label.toUpperCase()}`, winnerColor, width, color);
  const judgeLine = result.engine.model
    ? `JUDGE  ${result.engine.judgeMode} via ${result.engine.model}`
    : `JUDGE  ${result.engine.judgeMode}`;

  lines.push(...boxedTitle("BETTERTHANYOU // CLI PORTRAIT BATTLE", ANSI.bold, width, color));
  lines.push(...winnerBanner);
  lines.push(paint(judgeLine, ANSI.dim, color));
  lines.push(paint(`TOTAL   ${result.inputs.left.label} ${result.scores.left.total.toFixed(1)}  vs  ${result.inputs.right.label} ${result.scores.right.total.toFixed(1)}`, ANSI.dim, color));
  lines.push(paint(`MARGIN  ${result.winner.margin.toFixed(1)} points`, ANSI.dim, color));
  lines.push("");
  lines.push(paint("ABILITY COMPARISON", ANSI.cyan, color));
  lines.push(paint("Axis                      Left                         Right                        Gap", ANSI.dim, color));

  for (const card of result.axisCards) {
    const gap = signedGap(card, result.winner.id);
    const gapText = paint(gap.text, gap.color, color);
    lines.push(
      `${card.label.padEnd(24)} ${String(card.left.toFixed(1)).padStart(5)} ${meter(card.left)} | ${String(card.right.toFixed(1)).padStart(5)} ${meter(card.right)}  ${gapText}`
    );
  }

  lines.push("");
  lines.push(paint("OVERALL TAKE", ANSI.cyan, color));
  lines.push(result.sections.overallTake);
  lines.push("");
  lines.push(paint("WHY THIS WON", ANSI.cyan, color));
  lines.push(result.sections.whyThisWon);
  lines.push("");
  lines.push(paint("MODEL JURY NOTES", ANSI.cyan, color));
  lines.push(result.sections.modelJuryNotes);
  lines.push("");
  lines.push(paint("SAVE FILES", ANSI.cyan, color));
  lines.push(paint(`HTML report : ${artifacts.htmlPath}`, ANSI.dim, color));
  lines.push(paint(`JSON result : ${artifacts.jsonPath}`, ANSI.dim, color));

  return lines.join("\n");
}

export function renderReportSummary(report, options = {}) {
  const color = options.color !== false;
  const lines = [];
  lines.push(...boxedTitle("BETTERTHANYOU // REPORT REBUILT", ANSI.bold, 84, color));
  lines.push(paint(`HTML report : ${report.htmlPath}`, ANSI.dim, color));
  lines.push(paint(`JSON result : ${report.jsonPath}`, ANSI.dim, color));
  return lines.join("\n");
}

export function renderOpenSummary(targetPath, options = {}) {
  const color = options.color !== false;
  return paint(`Opened: ${targetPath}`, ANSI.dim, color);
}

export async function presentTerminalBattleApp(result, artifacts, options = {}) {
  const stdin = options.stdin || process.stdin;
  const stdout = options.stdout || process.stdout;
  if (!stdin.isTTY || !stdout.isTTY) {
    stdout.write(`${renderTerminalBattle(result, artifacts, { color: stdout.isTTY })}\n`);
    return;
  }

  const footer = [
    "",
    paint("Keys: [o] open report  [q] quit", ANSI.dim, true)
  ].join("\n");

  const screen = `${renderTerminalBattle(result, artifacts, { color: true })}\n${footer}`;
  const wasRaw = Boolean(stdin.isRaw);

  readline.emitKeypressEvents(stdin);
  stdin.setRawMode(true);
  stdout.write("\u001b[?1049h\u001b[2J\u001b[H\u001b[?25l");
  stdout.write(screen);

  await new Promise(resolve => {
    const cleanup = () => {
      stdin.off("keypress", handleKeypress);
      stdin.setRawMode(wasRaw);
      stdout.write("\u001b[?25h\u001b[?1049l");
      resolve();
    };

    const handleKeypress = async (_, key = {}) => {
      if (key.name === "o") {
        if (options.onOpenReport) {
          cleanup();
          await options.onOpenReport(artifacts.htmlPath);
          return;
        }
      }

      if (key.name === "q" || key.name === "return" || key.name === "escape" || (key.ctrl && key.name === "c")) {
        cleanup();
      }
    };

    stdin.on("keypress", handleKeypress);
  });
}
