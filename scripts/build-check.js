import { mkdir } from "node:fs/promises";

await import("../packages/core/src/index.js");
await import("../packages/cli/src/cli.js");
await import("../packages/mcp-server/src/server.js");
await import("../apps/web/src/server.js");

await mkdir(new URL("../reports/", import.meta.url), { recursive: true });

console.log("Build check passed.");
