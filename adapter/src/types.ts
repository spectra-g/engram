export interface Memory {
  id: number;
  file_path: string;
  symbol_name?: string;
  content: string;
  created_at: string;
}

export interface TestIntent {
  title: string;
}

export interface CoupledFile {
  path: string;
  coupling_score: number;
  co_change_count: number;
  risk_score: number;
  memories?: Memory[];
  test_intents?: TestIntent[];
}

export interface AnalysisResponse {
  file_path: string;
  repo_root: string;
  coupled_files: CoupledFile[];
  commit_count: number;
  analysis_time_ms: number;
}

export interface AnalysisRequest {
  file_path: string;
  repo_root: string;
}

export interface ProcessResult {
  stdout: string;
  stderr: string;
  exitCode: number;
}

export type RiskLevel = "Critical" | "High" | "Medium" | "Low";

export interface FormattedCoupledFile {
  path: string;
  risk_level: RiskLevel;
  risk_score: number;
  description: string;
  memories?: string[];
  test_intents?: string[];
}

export interface FormattedAnalysisResponse extends AnalysisResponse {
  summary: string;
  formatted_files: FormattedCoupledFile[];
}

export interface AddNoteRequest {
  file_path: string;
  repo_root: string;
  content: string;
  symbol_name?: string;
}

export interface AddNoteResponse {
  id: number;
  file_path: string;
  content: string;
}

export interface SearchNotesRequest {
  query: string;
  repo_root: string;
}

export interface SearchNotesResponse {
  query: string;
  memories: Memory[];
}

export interface ListNotesRequest {
  repo_root: string;
  file_path?: string;
}

export interface ListNotesResponse {
  file_path?: string;
  memories: Memory[];
}

export interface MetricsSummary {
  total_analyses: number;
  notes_created: number;
  searches_performed: number;
  lists_performed: number;
  total_coupled_files: number;
  critical_risk_count: number;
  high_risk_count: number;
  medium_risk_count: number;
  low_risk_count: number;
  test_files_found: number;
  test_intents_extracted: number;
  avg_analysis_time_ms: number;
}

export interface MetricsResponse {
  repo_root: string;
  summary: MetricsSummary;
}

export interface GetMetricsRequest {
  repo_root: string;
}
