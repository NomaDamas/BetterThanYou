import { copyFile, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

import { AXIS_DEFINITIONS, ENGINE_VERSION, PRODUCT_NAME } from "./contracts.js";
import { createBattleId, ensureDirectory, slugify, toFsPath } from "./util.js";

function axisMarkup(result) {
  return result.axisCards.map(card => {
    const winnerClass = card.leader === "left" ? "left-win" : card.leader === "right" ? "right-win" : "tie";
    return `
      <article class="axis-card ${winnerClass}">
        <header>
          <span>${card.label}</span>
          <strong>${card.diff.toFixed(1)} pt gap</strong>
        </header>
        <div class="axis-values">
          <div>
            <small>${result.inputs.left.label}</small>
            <b>${card.left.toFixed(1)}</b>
          </div>
          <div>
            <small>${result.inputs.right.label}</small>
            <b>${card.right.toFixed(1)}</b>
          </div>
        </div>
      </article>
    `;
  }).join("");
}

function renderNarrativeRow(title, body) {
  return `<section class="narrative-block"><h3>${title}</h3><p>${body}</p></section>`;
}

export function generateBattleHtmlReport(result) {
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>${PRODUCT_NAME} • ${result.inputs.left.label} vs ${result.inputs.right.label}</title>
    <style>
      :root {
        --bg: #0d0f14;
        --panel: rgba(18, 23, 34, 0.82);
        --panel-strong: rgba(21, 28, 43, 0.92);
        --line: rgba(255, 255, 255, 0.1);
        --text: #f5efe4;
        --muted: #c7b9a5;
        --accent: #ff8c42;
        --accent-2: #78f0d4;
        --loss: #8db7ff;
        --win: #ffcf5a;
      }

      * { box-sizing: border-box; }
      body {
        margin: 0;
        min-height: 100vh;
        background:
          radial-gradient(circle at top left, rgba(255, 140, 66, 0.28), transparent 36%),
          radial-gradient(circle at right center, rgba(120, 240, 212, 0.18), transparent 28%),
          linear-gradient(140deg, #090b10 0%, #111722 55%, #0f1116 100%);
        color: var(--text);
        font-family: "Avenir Next", "Trebuchet MS", "Segoe UI", sans-serif;
      }

      .shell {
        width: min(1180px, calc(100vw - 32px));
        margin: 0 auto;
        padding: 28px 0 72px;
      }

      .hero {
        display: grid;
        gap: 20px;
        padding: 28px;
        border: 1px solid var(--line);
        border-radius: 28px;
        background: linear-gradient(180deg, rgba(12, 16, 25, 0.88), rgba(17, 22, 34, 0.9));
        box-shadow: 0 24px 70px rgba(0, 0, 0, 0.35);
      }

      .eyebrow {
        text-transform: uppercase;
        letter-spacing: 0.24em;
        font-size: 12px;
        color: var(--muted);
      }

      .winner-row {
        display: flex;
        flex-wrap: wrap;
        align-items: end;
        justify-content: space-between;
        gap: 16px;
      }

      .winner-row h1 {
        margin: 0;
        font-size: clamp(38px, 7vw, 84px);
        line-height: 0.95;
        text-transform: uppercase;
      }

      .winner-pill {
        display: inline-flex;
        align-items: center;
        gap: 10px;
        padding: 10px 16px;
        border-radius: 999px;
        background: rgba(255, 207, 90, 0.14);
        color: var(--win);
        font-size: 14px;
      }

      .totals {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
        gap: 16px;
      }

      .score-panel,
      .axis-card,
      .narrative-block,
      .input-card {
        border: 1px solid var(--line);
        border-radius: 24px;
        background: var(--panel);
        backdrop-filter: blur(14px);
      }

      .score-panel {
        padding: 18px;
      }

      .score-panel small,
      .axis-card small {
        color: var(--muted);
        display: block;
      }

      .score-panel strong {
        display: block;
        font-size: 42px;
        margin-top: 8px;
      }

      .inputs,
      .narrative,
      .axes {
        margin-top: 24px;
      }

      .inputs {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
        gap: 20px;
      }

      .input-card {
        overflow: hidden;
      }

      .input-card img {
        display: block;
        width: 100%;
        aspect-ratio: 4 / 5;
        object-fit: cover;
      }

      .input-copy {
        padding: 18px;
      }

      .input-copy h2 {
        margin: 0 0 8px;
        font-size: 28px;
      }

      .axis-grid,
      .narrative-grid {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
        gap: 16px;
      }

      .axis-card {
        padding: 18px;
      }

      .axis-card header,
      .axis-values {
        display: flex;
        justify-content: space-between;
        gap: 12px;
      }

      .axis-card header {
        margin-bottom: 14px;
      }

      .axis-card.left-win { box-shadow: inset 0 0 0 1px rgba(255, 207, 90, 0.34); }
      .axis-card.right-win { box-shadow: inset 0 0 0 1px rgba(141, 183, 255, 0.34); }
      .axis-card.tie { box-shadow: inset 0 0 0 1px rgba(120, 240, 212, 0.2); }

      .narrative-block {
        padding: 20px;
      }

      .narrative-block h3 {
        margin: 0 0 10px;
        font-size: 18px;
        text-transform: uppercase;
        letter-spacing: 0.08em;
      }

      .narrative-block p {
        margin: 0;
        color: var(--muted);
        line-height: 1.7;
      }

      footer {
        margin-top: 20px;
        color: var(--muted);
        font-size: 14px;
      }
    </style>
  </head>
  <body>
    <main class="shell">
      <section class="hero">
        <div class="eyebrow">${PRODUCT_NAME} • Winner First</div>
        <div class="winner-row">
          <div>
            <div class="winner-pill">Winner • ${result.winner.label}</div>
            <h1>${result.winner.label}</h1>
          </div>
          <div>
            <div class="eyebrow">Engine</div>
            <strong>${ENGINE_VERSION}</strong>
          </div>
        </div>
        <p>${result.sections.overallTake}</p>
        <div class="totals">
          <article class="score-panel">
            <small>${result.inputs.left.label}</small>
            <strong>${result.scores.left.total.toFixed(1)}</strong>
          </article>
          <article class="score-panel">
            <small>${result.inputs.right.label}</small>
            <strong>${result.scores.right.total.toFixed(1)}</strong>
          </article>
          <article class="score-panel">
            <small>Margin</small>
            <strong>${result.winner.margin.toFixed(1)}</strong>
          </article>
        </div>
      </section>

      <section class="inputs">
        <article class="input-card">
          <img alt="${result.inputs.left.label}" src="${result.inputs.left.imageDataUrl}" />
          <div class="input-copy">
            <h2>${result.inputs.left.label}</h2>
            <p>${result.sections.strengths.left}</p>
          </div>
        </article>
        <article class="input-card">
          <img alt="${result.inputs.right.label}" src="${result.inputs.right.imageDataUrl}" />
          <div class="input-copy">
            <h2>${result.inputs.right.label}</h2>
            <p>${result.sections.strengths.right}</p>
          </div>
        </article>
      </section>

      <section class="axes">
        <div class="eyebrow">Quantitative Axes</div>
        <div class="axis-grid">${axisMarkup(result)}</div>
      </section>

      <section class="narrative">
        <div class="eyebrow">Qualitative Analysis</div>
        <div class="narrative-grid">
          ${renderNarrativeRow("Overall Take", result.sections.overallTake)}
          ${renderNarrativeRow("Why This Won", result.sections.whyThisWon)}
          ${renderNarrativeRow(`${result.inputs.left.label} Weaknesses`, result.sections.weaknesses.left)}
          ${renderNarrativeRow(`${result.inputs.right.label} Weaknesses`, result.sections.weaknesses.right)}
          ${renderNarrativeRow("Model Jury Notes", result.sections.modelJuryNotes)}
        </div>
      </section>

      <footer>
        Generated ${result.createdAt} • ${PRODUCT_NAME} • ${result.battleId}
      </footer>
    </main>
  </body>
</html>`;
}

export async function writeBattleArtifacts(result, options = {}) {
  const outputDir = await ensureDirectory(options.outputDir || new URL("../../../reports/", import.meta.url));
  const baseName = options.fileStem || createBattleId(result.inputs.left.label, result.inputs.right.label);
  const fileStem = slugify(baseName);
  const htmlPath = join(outputDir, `${fileStem}.html`);
  const jsonPath = join(outputDir, `${fileStem}.json`);
  const latestHtmlPath = join(outputDir, "latest-battle.html");
  const latestJsonPath = join(outputDir, "latest-battle.json");

  await writeFile(htmlPath, generateBattleHtmlReport(result), "utf8");
  await writeFile(jsonPath, JSON.stringify(result, null, 2), "utf8");
  await copyFile(htmlPath, latestHtmlPath);
  await copyFile(jsonPath, latestJsonPath);

  return {
    htmlPath,
    jsonPath,
    latestHtmlPath,
    latestJsonPath
  };
}

export async function readBattleResultFile(pathLike) {
  const path = toFsPath(pathLike);
  return JSON.parse(await readFile(path, "utf8"));
}

export async function regenerateBattleReport(pathLike, options = {}) {
  const result = await readBattleResultFile(pathLike);
  const artifacts = await writeBattleArtifacts(result, {
    outputDir: options.outputDir,
    fileStem: options.fileStem || `${slugify(result.inputs.left.label)}-vs-${slugify(result.inputs.right.label)}-report`
  });

  return {
    result,
    ...artifacts
  };
}
