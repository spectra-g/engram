import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ADAPTER_PATH = resolve(__dirname, "../../../adapter/dist/index.js");

export interface McpClientOptions {
  coreBinaryPath?: string;
}

/**
 * "Fake Agent" - MCP client that connects to the adapter over stdio.
 * Used in E2E tests to simulate an AI agent calling tools.
 */
export class McpTestClient {
  private client: Client;
  private transport: StdioClientTransport | null = null;

  constructor() {
    this.client = new Client({
      name: "engram-test-client",
      version: "0.1.0",
    });
  }

  async connect(options: McpClientOptions = {}): Promise<void> {
    const env: Record<string, string> = {
      ...process.env as Record<string, string>,
    };

    if (options.coreBinaryPath) {
      env.ENGRAM_CORE_BINARY = options.coreBinaryPath;
    }

    this.transport = new StdioClientTransport({
      command: "node",
      args: [ADAPTER_PATH],
      env,
    });

    await this.client.connect(this.transport);
  }

  async callTool(
    name: string,
    args: Record<string, unknown>
  ): Promise<{ content: Array<{ type: string; text?: string }> }> {
    const result = await this.client.callTool({ name, arguments: args });
    return result as { content: Array<{ type: string; text?: string }> };
  }

  async listTools(): Promise<string[]> {
    const result = await this.client.listTools();
    return result.tools.map((t) => t.name);
  }

  async close(): Promise<void> {
    if (this.transport) {
      await this.transport.close();
    }
  }
}
