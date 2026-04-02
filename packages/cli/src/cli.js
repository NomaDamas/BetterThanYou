import { createInterface } from "node:readline/promises";
import { resolve } from "node:path";

import {
  analyzePortraitBattle,
  regenerateBattleReport,
  writeBattleArtifacts
} from "@better-than-you/core";

import {
  defaultReportsDir,
  normalizeSourceInput,
  openPathTarget,
  readClipboardSource
} from "./platform.js";
import {
  renderOpenSummary,
  renderReportSummary,
  renderTerminalBattle
} from "./terminal.js";

const KNOWN_COMMANDS = new Set(["battle", "report", "open", "help"]);

function printHelp() {
  return `BetterThanYou

CLI-first portrait battle tool for fictional AI-generated adult portraits.

Usage:
  better-than-you
  better-than-you battle <left> <right> [options]
  better-than-you report <battle-json-path> [options]
  better-than-you open [latest|path] [--out-dir path]

Battle options:
  --left-label <name>        Override the left portrait label
  --right-label <name>       Override the right portrait label
  --out-dir <path>           Write HTML/JSON reports to a custom directory
  --left-clipboard           Read the left portrait source from the clipboard
  --right-clipboard          Read the right portrait source from the clipboard
  --json                     Print structured JSON to stdout instead of the battle HUD
  --open                     Open the generated HTML report after the battle

Notes:
  - Dragging files into the terminal usually pastes their absolute paths.
  - Running without arguments starts guided CLI mode.
  - The web helper is optional; the CLI is the primary product surface.
`;
}

function normalizeInvocation(argv) {
  if (!argv.length) {
    return { command: "battle", rest: [] };
  }

  const [first, ...rest] = argv;
  if (first === "-h" || first === "--help" || first === "help") {
    return { command: "help", rest: [] };
  }

  if (KNOWN_COMMANDS.has(first)) {
    return { command: first, rest };
  }

  return { command: "battle", rest: argv };
}

function parseOptions(rest) {
  const parsed = {
    positional: [],
    outDir: undefined,
    leftLabel: undefined,
    rightLabel: undefined,
    leftClipboard: false,
    rightClipboard: false,
    json: false,
    open: false
  };

  for (let index = 0; index < rest.length; index += 1) {
    const value = rest[index];
    if (value === "--out-dir") {
      parsed.outDir = rest[index + 1];
      index += 1;
      continue;
    }
    if (value === "--left-label") {
      parsed.leftLabel = rest[index + 1];
      index += 1;
      continue;
    }
    if (value === "--right-label") {
      parsed.rightLabel = rest[index + 1];
      index += 1;
      continue;
    }
    if (value === "--left-clipboard") {
      parsed.leftClipboard = true;
      continue;
    }
    if (value === "--right-clipboard") {
      parsed.rightClipboard = true;
      continue;
    }
    if (value === "--json") {
      parsed.json = true;
      continue;
    }
    if (value === "--open") {
      parsed.open = true;
      continue;
    }
    parsed.positional.push(value);
  }

  return parsed;
}

async function promptWithInterface(rl, promptText) {
  const answer = await rl.question(promptText);
  return normalizeSourceInput(answer);
}

async function readPipedInputLines(input) {
  const lines = [];
  for await (const chunk of input) {
    for (const line of chunk.toString().replace(/\r/g, "").split("\n")) {
      const normalized = normalizeSourceInput(line);
      if (normalized) {
        lines.push(normalized);
      }
    }
  }
  return lines;
}

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

async function resolveBattleInputs(parsed, streams, env) {
  let [leftSource, rightSource] = parsed.positional.map(normalizeSourceInput);

  if (parsed.leftClipboard) {
    leftSource = await readClipboardSource("left", env);
  }
  if (parsed.rightClipboard) {
    rightSource = await readClipboardSource("right", env);
  }

  if (leftSource && rightSource) {
    return { leftSource, rightSource };
  }

  if (!streams.stdin || !streams.stdout) {
    throw new Error("Interactive mode requires stdin/stdout streams.");
  }

  if (!streams.stdin.isTTY) {
    const pipedLines = await readPipedInputLines(streams.stdin);
    if (!leftSource) {
      leftSource = pipedLines.shift();
    }
    if (!rightSource) {
      rightSource = pipedLines.shift();
    }
  } else {
    const rl = createInterface({ input: streams.stdin, output: streams.stdout });
    try {
      if (!leftSource) {
        leftSource = await promptWithInterface(rl, "Drag or paste LEFT portrait path/URL/data URL: ");
      }
      if (!rightSource) {
        rightSource = await promptWithInterface(rl, "Drag or paste RIGHT portrait path/URL/data URL: ");
      }
    } finally {
      rl.close();
    }
  }

  if (!leftSource || !rightSource) {
    throw new Error("Two portrait inputs are required.");
  }

  return { leftSource, rightSource };
}

async function runBattle(rest, streams, env) {
  const parsed = parseOptions(rest);
  const { leftSource, rightSource } = await resolveBattleInputs(parsed, streams, env);
  const outputDir = parsed.outDir ? resolve(parsed.outDir) : defaultReportsDir();

  const result = await analyzePortraitBattle({
    leftSource,
    rightSource,
    leftLabel: parsed.leftLabel,
    rightLabel: parsed.rightLabel
  });

  const artifacts = await writeBattleArtifacts(result, {
    outputDir
  });

  if (parsed.open) {
    await openPathTarget(artifacts.htmlPath, env);
  }

  if (parsed.json) {
    streams.stdout.write(`${JSON.stringify({ result: stripEmbeddedImages(result), artifacts }, null, 2)}\n`);
    return 0;
  }

  streams.stdout.write(`${renderTerminalBattle(result, artifacts, { color: streams.stdout.isTTY })}\n`);
  return 0;
}

async function runReport(rest, streams, env) {
  const parsed = parseOptions(rest);
  const [battleJsonPath] = parsed.positional;
  let resolvedBattleJson = battleJsonPath ? resolve(normalizeSourceInput(battleJsonPath)) : undefined;

  if (!resolvedBattleJson) {
    if (!streams.stdin || !streams.stdout) {
      throw new Error("Interactive mode requires stdin/stdout streams.");
    }

    if (!streams.stdin.isTTY) {
      const pipedLines = await readPipedInputLines(streams.stdin);
      resolvedBattleJson = pipedLines[0] ? resolve(pipedLines[0]) : undefined;
    } else {
      const rl = createInterface({ input: streams.stdin, output: streams.stdout });
      try {
        resolvedBattleJson = resolve(await promptWithInterface(rl, "Paste battle JSON path: "));
      } finally {
        rl.close();
      }
    }
  }

  if (!resolvedBattleJson) {
    throw new Error("A battle JSON path is required.");
  }

  const outputDir = parsed.outDir ? resolve(parsed.outDir) : defaultReportsDir();
  const report = await regenerateBattleReport(resolvedBattleJson, {
    outputDir
  });

  if (parsed.open) {
    await openPathTarget(report.htmlPath, env);
  }

  if (parsed.json) {
    streams.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    return 0;
  }

  streams.stdout.write(`${renderReportSummary(report, { color: streams.stdout.isTTY })}\n`);
  return 0;
}

async function runOpen(rest, streams, env) {
  const parsed = parseOptions(rest);
  const [target] = parsed.positional;
  const outputDir = parsed.outDir ? resolve(parsed.outDir) : defaultReportsDir();
  const targetPath = target && target !== "latest"
    ? resolve(normalizeSourceInput(target))
    : resolve(outputDir, "latest-battle.html");

  await openPathTarget(targetPath, env);
  streams.stdout.write(`${renderOpenSummary(targetPath, { color: streams.stdout.isTTY })}\n`);
  return 0;
}

export async function runCli(argv = process.argv.slice(2), streams = process, env = process.env) {
  const invocation = normalizeInvocation(argv);

  if (invocation.command === "help") {
    streams.stdout.write(printHelp());
    return 0;
  }

  if (invocation.command === "battle") {
    return runBattle(invocation.rest, streams, env);
  }

  if (invocation.command === "report") {
    return runReport(invocation.rest, streams, env);
  }

  if (invocation.command === "open") {
    return runOpen(invocation.rest, streams, env);
  }

  streams.stdout.write(printHelp());
  return 0;
}
