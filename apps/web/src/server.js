import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, normalize } from "node:path";

import { analyzePortraitBattle, writeBattleArtifacts } from "@better-than-you/core";

function json(response, statusCode, payload) {
  response.writeHead(statusCode, {
    "content-type": "application/json; charset=utf-8"
  });
  response.end(JSON.stringify(payload));
}

function text(response, statusCode, payload, contentType = "text/html; charset=utf-8") {
  response.writeHead(statusCode, {
    "content-type": contentType
  });
  response.end(payload);
}

function getMimeType(path) {
  const ext = extname(path).toLowerCase();
  const lookup = {
    ".html": "text/html; charset=utf-8",
    ".json": "application/json; charset=utf-8"
  };
  return lookup[ext] || "text/plain; charset=utf-8";
}

async function readJsonBody(request) {
  const chunks = [];
  for await (const chunk of request) {
    chunks.push(chunk);
  }
  return JSON.parse(Buffer.concat(chunks).toString("utf8") || "{}");
}

function stripEmbeddedImages(result) {
  return {
    ...result,
    inputs: {
      left: { ...result.inputs.left, imageDataUrl: undefined },
      right: { ...result.inputs.right, imageDataUrl: undefined }
    }
  };
}

function renderHomePage() {
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>BetterThanYou</title>
    <style>
      :root {
        --bg: #0b0f15;
        --panel: rgba(18, 25, 36, 0.82);
        --panel-2: rgba(11, 17, 27, 0.82);
        --line: rgba(255,255,255,0.1);
        --text: #f4ecde;
        --muted: #ccbca3;
        --accent: #ff8f42;
        --accent-2: #63ebd3;
      }
      * { box-sizing: border-box; }
      body {
        margin: 0;
        font-family: "Avenir Next", "Trebuchet MS", "Segoe UI", sans-serif;
        color: var(--text);
        background:
          radial-gradient(circle at top left, rgba(255,143,66,0.25), transparent 36%),
          radial-gradient(circle at bottom right, rgba(99,235,211,0.14), transparent 28%),
          linear-gradient(145deg, #090b10 0%, #121824 100%);
      }
      main {
        width: min(1200px, calc(100vw - 32px));
        margin: 0 auto;
        padding: 28px 0 56px;
      }
      .hero, .panel, iframe {
        border: 1px solid var(--line);
        border-radius: 28px;
        background: var(--panel);
        backdrop-filter: blur(14px);
      }
      .hero {
        padding: 28px;
      }
      h1 {
        margin: 0;
        font-size: clamp(38px, 7vw, 82px);
        line-height: 0.92;
        text-transform: uppercase;
      }
      .eyebrow {
        font-size: 12px;
        text-transform: uppercase;
        letter-spacing: 0.24em;
        color: var(--muted);
        margin-bottom: 12px;
      }
      .hero p {
        color: var(--muted);
        max-width: 720px;
        line-height: 1.7;
      }
      .grid {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
        gap: 18px;
        margin-top: 22px;
      }
      .panel {
        padding: 20px;
      }
      label {
        display: block;
        margin-bottom: 8px;
        text-transform: uppercase;
        letter-spacing: 0.08em;
        font-size: 12px;
        color: var(--muted);
      }
      input, button {
        width: 100%;
        border-radius: 16px;
        border: 1px solid var(--line);
        padding: 14px 16px;
        background: var(--panel-2);
        color: var(--text);
        font: inherit;
      }
      button {
        cursor: pointer;
        background: linear-gradient(135deg, var(--accent), #ffbc5c);
        color: #111;
        font-weight: 700;
      }
      .stack { display: grid; gap: 12px; }
      .summary {
        margin-top: 22px;
        display: none;
      }
      .summary strong {
        display: block;
        font-size: 28px;
        margin-top: 8px;
      }
      iframe {
        width: 100%;
        min-height: 820px;
        margin-top: 22px;
        display: none;
      }
      a { color: var(--accent-2); }
    </style>
  </head>
  <body>
    <main>
      <section class="hero">
        <div class="eyebrow">CLI-first • Optional web helper • Winner-first</div>
        <h1>BetterThanYou</h1>
        <p>This optional helper is for non-developers and quick report viewing. The CLI remains the primary BetterThanYou product surface.</p>
      </section>

      <form id="battle-form" class="grid">
        <section class="panel stack">
          <h2>Left Portrait</h2>
          <div>
            <label for="left-file">File</label>
            <input id="left-file" type="file" accept="image/*" />
          </div>
          <div>
            <label for="left-url">URL</label>
            <input id="left-url" type="url" placeholder="https://..." />
          </div>
          <div>
            <label for="left-label">Label</label>
            <input id="left-label" type="text" placeholder="Aurora" />
          </div>
        </section>

        <section class="panel stack">
          <h2>Right Portrait</h2>
          <div>
            <label for="right-file">File</label>
            <input id="right-file" type="file" accept="image/*" />
          </div>
          <div>
            <label for="right-url">URL</label>
            <input id="right-url" type="url" placeholder="https://..." />
          </div>
          <div>
            <label for="right-label">Label</label>
            <input id="right-label" type="text" placeholder="Blaze" />
          </div>
        </section>

        <section class="panel stack">
          <h2>Battle</h2>
          <p class="eyebrow">Uploads convert to data URLs in-browser.</p>
          <button type="submit">Run Battle</button>
          <div id="status">Ready.</div>
          <div id="summary" class="summary panel"></div>
          <a id="report-link" href="#" target="_blank" rel="noreferrer" hidden>Open static report</a>
        </section>
      </form>

      <iframe id="viewer" title="Battle report viewer"></iframe>
    </main>
    <script>
      const form = document.querySelector('#battle-form');
      const statusNode = document.querySelector('#status');
      const summaryNode = document.querySelector('#summary');
      const viewer = document.querySelector('#viewer');
      const reportLink = document.querySelector('#report-link');

      function fileToDataUrl(file) {
        return new Promise((resolve, reject) => {
          const reader = new FileReader();
          reader.onload = () => resolve(reader.result);
          reader.onerror = reject;
          reader.readAsDataURL(file);
        });
      }

      async function pickSource(fileInputId, urlInputId) {
        const file = document.querySelector(fileInputId).files[0];
        const url = document.querySelector(urlInputId).value.trim();
        if (file) return fileToDataUrl(file);
        if (url) return url;
        throw new Error('Provide either a file or a URL for each side.');
      }

      form.addEventListener('submit', async event => {
        event.preventDefault();
        statusNode.textContent = 'Running battle...';
        summaryNode.style.display = 'none';
        viewer.style.display = 'none';
        reportLink.hidden = true;

        try {
          const [leftSource, rightSource] = await Promise.all([
            pickSource('#left-file', '#left-url'),
            pickSource('#right-file', '#right-url')
          ]);

          const response = await fetch('/api/v1/battles', {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({
              leftSource,
              rightSource,
              leftLabel: document.querySelector('#left-label').value.trim() || undefined,
              rightLabel: document.querySelector('#right-label').value.trim() || undefined
            })
          });

          const payload = await response.json();
          if (!response.ok) {
            throw new Error(payload.error?.message || 'Battle failed.');
          }

          statusNode.textContent = 'Battle complete.';
          summaryNode.style.display = 'block';
          summaryNode.innerHTML = '<div class="eyebrow">Winner First</div><strong>' + payload.data.winner.label + '</strong><p>' + payload.data.sections.overallTake + '</p>';
          viewer.src = payload.meta.reportUrl;
          viewer.style.display = 'block';
          reportLink.href = payload.meta.reportUrl;
          reportLink.hidden = false;
          reportLink.textContent = 'Open static report: ' + payload.meta.reportUrl;
        } catch (error) {
          statusNode.textContent = error.message;
        }
      });
    </script>
  </body>
</html>`;
}

function safeJoin(baseDir, pathname) {
  const normalized = normalize(pathname).replace(/^([.][.][\/\\])+/, "");
  return join(baseDir, normalized);
}

export function createWebServer({ port = 3000, reportsDir = new URL("../../../reports/", import.meta.url) } = {}) {
  const resolvedReportsDir = typeof reportsDir === "string" ? reportsDir : reportsDir.pathname;

  return createServer(async (request, response) => {
    const url = new URL(request.url, `http://127.0.0.1:${port}`);

    if (request.method === "GET" && (url.pathname === "/" || url.pathname === "/index.html")) {
      text(response, 200, renderHomePage());
      return;
    }

    if (request.method === "GET" && url.pathname.startsWith("/reports/")) {
      const filePath = safeJoin(resolvedReportsDir, url.pathname.replace("/reports/", ""));
      try {
        const body = await readFile(filePath);
        text(response, 200, body, getMimeType(filePath));
      } catch {
        text(response, 404, "Report not found", "text/plain; charset=utf-8");
      }
      return;
    }

    if (request.method === "POST" && url.pathname === "/api/v1/battles") {
      try {
        const payload = await readJsonBody(request);
        if (!payload.leftSource || !payload.rightSource) {
          json(response, 400, {
            error: {
              code: "validation_error",
              message: "leftSource and rightSource are required"
            }
          });
          return;
        }

        const result = await analyzePortraitBattle(payload);
        const artifacts = await writeBattleArtifacts(result, { outputDir: resolvedReportsDir });
        const reportUrl = `/reports/${artifacts.htmlPath.split("/").pop()}`;

        json(response, 200, {
          data: stripEmbeddedImages(result),
          meta: {
            reportUrl,
            reportPath: artifacts.htmlPath,
            jsonPath: artifacts.jsonPath
          }
        });
      } catch (error) {
        json(response, 500, {
          error: {
            code: "battle_failed",
            message: error instanceof Error ? error.message : "Unknown error"
          }
        });
      }
      return;
    }

    text(response, 404, "Not found", "text/plain; charset=utf-8");
  });
}

if (process.argv[1] === new URL(import.meta.url).pathname) {
  const port = Number.parseInt(process.env.PORT || "3000", 10);
  const server = createWebServer({ port });
  server.listen(port, () => {
    console.log(`BetterThanYou web listening on http://127.0.0.1:${port}`);
  });
}
