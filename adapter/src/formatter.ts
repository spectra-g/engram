import type { AnalysisResponse, CoupledFile, RiskLevel, FormattedCoupledFile } from "./types.js";

const DISPLAY_LIMIT = 5;

export function classifyRisk(score: number): RiskLevel {
  if (score >= 0.8) return "Critical";
  if (score >= 0.6) return "High";
  if (score >= 0.3) return "Medium";
  return "Low";
}

export function describeFile(file: CoupledFile, commitCount: number): string {
  const pct = commitCount > 0
    ? Math.round((file.co_change_count / commitCount) * 100)
    : 0;
  return `Changed together in ${file.co_change_count} of ${commitCount} commits (${pct}%)`;
}

export function buildSummaryLine(filePath: string, files: FormattedCoupledFile[]): string {
  if (files.length === 0) {
    return `Changing ${filePath} has no coupled files.`;
  }

  const riskCounts: Partial<Record<RiskLevel, number>> = {};
  for (const f of files) {
    riskCounts[f.risk_level] = (riskCounts[f.risk_level] || 0) + 1;
  }

  const parts: string[] = [];
  for (const level of ["Critical", "High", "Medium", "Low"] as RiskLevel[]) {
    const count = riskCounts[level];
    if (count) {
      parts.push(`${count} ${level.toLowerCase()} risk`);
    }
  }

  return `Changing ${filePath} may affect ${files.length} file${files.length === 1 ? "" : "s"}. ${parts.join(", ")}.`;
}

export function buildFileDetails(files: FormattedCoupledFile[]): string {
  if (files.length === 0) return "";

  const emojiMap: Record<RiskLevel, string> = {
    Critical: "\u26A0\uFE0F",
    High: "\u26A0",
    Medium: "\u26A0",
    Low: "\u2139\uFE0F",
  };

  return files
    .map((f) => {
      const emoji = emojiMap[f.risk_level];
      let line = `${emoji} ${f.risk_level} Risk (${f.risk_score.toFixed(2)}): ${f.path}\n   ${f.description}`;
      if (f.memories && f.memories.length > 0) {
        line += `\n   Notes: ${f.memories.join("; ")}`;
      }
      if (f.test_intents && f.test_intents.length > 0) {
        line += `\n   Current test behavior (may need updating):`;
        for (const intent of f.test_intents) {
          line += `\n     - ${intent}`;
        }
      }
      return line;
    })
    .join("\n\n");
}

/**
 * Formats raw analysis JSON into a human-readable + machine-parseable
 * response for the MCP tool call. Returns JSON text that contains
 * both structured data and LLM-friendly warnings.
 */
export function formatAnalysisResponse(response: AnalysisResponse): string {
  const displayFiles = response.coupled_files.slice(0, DISPLAY_LIMIT);

  const formattedFiles: FormattedCoupledFile[] = displayFiles.map((f) => {
    const formatted: FormattedCoupledFile = {
      path: f.path,
      risk_level: classifyRisk(f.risk_score),
      risk_score: f.risk_score,
      description: describeFile(f, response.commit_count),
    };
    if (f.memories && f.memories.length > 0) {
      formatted.memories = f.memories.map((m) => m.content);
    }
    if (f.test_intents && f.test_intents.length > 0) {
      formatted.test_intents = f.test_intents.map((t) => t.title);
    }
    return formatted;
  });

  const summaryLine = buildSummaryLine(response.file_path, formattedFiles);
  const details = buildFileDetails(formattedFiles);
  const summary = details ? `${summaryLine}\n\n${details}` : summaryLine;

  return JSON.stringify({
    summary,
    formatted_files: formattedFiles,
    ...response,
  });
}
