import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { analyze, addNote, searchNotes, listNotes, getMetrics } from "./process-bridge.js";
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
        "Analyzes the blast radius and coupling of files to prevent breaking changes. This tool reveals which other files are frequently co-committed with the target file, helping you understand what else might be affected by your changes. Use cases: bug fixes, feature additions, refactoring, code review.",
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
        "Save important context, decisions, or learnings about a file or symbol for future reference. Use this when you discover important information about code behavior, architectural decisions, bug fixes, or gotchas that would be valuable for future work. Notes are stored persistently and will appear in future impact analysis results.",
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
        "Read project notes (memories) about files before working on them. Use this when starting work on a file or investigating an area of the codebase to see if there are important notes, learnings, or context that have been saved. Can search by query text, filter by file path, or list all notes.",
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

  server.registerTool(
    "get_usage_metrics",
    {
      description:
        "Get usage statistics for this repository including how many analyses have been performed, risk distributions, notes created, and performance metrics. Useful for understanding your usage patterns and the health of the codebase analysis.",
      inputSchema: {
        repo_root: z.string().describe("Absolute path to the git repository root"),
      },
    },
    async ({ repo_root }) => {
      try {
        const response = await getMetrics({ repo_root });
        return {
          content: [
            {
              type: "text" as const,
              text: JSON.stringify(response, null, 2),
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
