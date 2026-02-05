#!/usr/bin/env node
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { createMcpServer } from "./mcp-server.js";
import { checkForUpdates } from "./version-check.js";

async function main() {
  const server = createMcpServer();
  const transport = new StdioServerTransport();
  await server.connect(transport);
  checkForUpdates();
}

main().catch((error) => {
  console.error("engram adapter fatal:", error);
  process.exit(1);
});
