import test from "node:test";
import assert from "node:assert/strict";
import { once } from "node:events";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Jimp } from "jimp";

import { createWebServer } from "../src/server.js";
import { createFixturePalette, createPortraitDataUrl } from "../../../test/support/portrait-fixture.js";

test("web api accepts two portrait sources and returns battle result", async () => {
  const reportsDir = await mkdtemp(join(tmpdir(), "bty-web-"));
  const server = createWebServer({ port: 0, reportsDir });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");

  const address = server.address();
  const leftSource = await createPortraitDataUrl(Jimp, createFixturePalette("left"));
  const rightSource = await createPortraitDataUrl(Jimp, createFixturePalette("right"));

  const response = await fetch(`http://127.0.0.1:${address.port}/api/v1/battles`, {
    method: "POST",
    headers: {
      "content-type": "application/json"
    },
    body: JSON.stringify({
      leftSource,
      rightSource,
      leftLabel: "Aurora",
      rightLabel: "Blaze"
    })
  });

  const payload = await response.json();
  server.close();

  assert.equal(response.status, 200);
  assert.equal(payload.data.winner_first, true);
  assert.match(payload.meta.reportUrl, /reports/);
});
