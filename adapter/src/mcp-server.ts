import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { analyze, addNote, searchNotes, listNotes } from "./process-bridge.js";
import { formatAnalysisResponse } from "./formatter.js";

export function createMcpServer(): McpServer {
  const server = new McpServer({
    name: "engram",
    version: "0.1.0",
  });

  server.registerTool(
    "get_impact_analysis",
    {
      description:
        "Analyze the blast radius of a file change. Returns files that are frequently co-committed (coupled) with the target file.",
      inputSchema: {
        file_path: z.string().describe("Path to the file to analyze, relative to repo root"),
        repo_root: z.string().describe("Absolute path to the git repository root"),
      },
    },
    async ({ file_path, repo_root }) => {
      try {
        const response = await analyze({ file_path, repo_root });
        return {
          content: [
            {
              type: "text" as const,
              text: formatAnalysisResponse(response),
            },
          ],
        };
      } catch (error) {
        const message =
          error instanceof Error ? error.message : String(error);
        return {
          content: [
            {
              type: "text" as const,
              text: JSON.stringify({ error: message }),
            },
          ],
          isError: true,
        };
      }
    }
  );

  server.registerTool(
    "save_project_note",
    {
      description:
        "Save a note (memory) about a file or symbol for future reference. Notes are stored persistently and will appear in impact analysis results.",
      inputSchema: {
        file_path: z.string().describe("File path the note relates to"),
        note: z.string().describe("The note content to save"),
        repo_root: z.string().describe("Absolute path to the git repository root"),
        symbol_name: z.string().optional().describe("Optional symbol name the note relates to"),
      },
    },
    async ({ file_path, note, repo_root, symbol_name }) => {
      try {
        const response = await addNote({
          file_path,
          content: note,
          repo_root,
          symbol_name,
        });
        return {
          content: [
            {
              type: "text" as const,
              text: JSON.stringify(response),
            },
          ],
        };
      } catch (error) {
        const message =
          error instanceof Error ? error.message : String(error);
        return {
          content: [
            {
              type: "text" as const,
              text: JSON.stringify({ error: message }),
            },
          ],
          isError: true,
        };
      }
    }
  );

  server.registerTool(
    "read_project_notes",
    {
      description:
        "Read project notes (memories). Search by query text, filter by file path, or list all notes.",
      inputSchema: {
        query: z.string().optional().describe("Search query to match against note content and file paths"),
        file_path: z.string().optional().describe("Filter notes for a specific file path"),
        repo_root: z.string().describe("Absolute path to the git repository root"),
      },
    },
    async ({ query, file_path, repo_root }) => {
      try {
        let response;
        if (query) {
          response = await searchNotes({ query, repo_root });
        } else {
          response = await listNotes({ repo_root, file_path });
        }
        return {
          content: [
            {
              type: "text" as const,
              text: JSON.stringify(response),
            },
          ],
        };
      } catch (error) {
        const message =
          error instanceof Error ? error.message : String(error);
        return {
          content: [
            {
              type: "text" as const,
              text: JSON.stringify({ error: message }),
            },
          ],
          isError: true,
        };
      }
    }
  );

  return server;
}
