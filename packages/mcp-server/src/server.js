import { fileURLToPath } from "node:url";

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";

import { executeBattleTool, executeReportTool } from "./tools.js";

export function createMcpServer() {
  const server = new McpServer({
    name: "better-than-you",
    version: "0.1.0"
  });

  server.tool(
    "battle_portraits",
    "Compare two fictional AI-generated adult portraits and generate battle artifacts.",
    {
      leftSource: z.string().min(1),
      rightSource: z.string().min(1),
      leftLabel: z.string().optional(),
      rightLabel: z.string().optional(),
      outputDir: z.string().optional()
    },
    async input => ({
      content: [
        {
          type: "text",
          text: JSON.stringify(await executeBattleTool(input), null, 2)
        }
      ]
    })
  );

  server.tool(
    "generate_battle_report",
    "Regenerate a static HTML report from a saved BetterThanYou battle JSON file.",
    {
      battleJsonPath: z.string().min(1),
      outputDir: z.string().optional()
    },
    async input => ({
      content: [
        {
          type: "text",
          text: JSON.stringify(await executeReportTool(input), null, 2)
        }
      ]
    })
  );

  return server;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const transport = new StdioServerTransport();
  await createMcpServer().connect(transport);
}
