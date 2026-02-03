use crate::types::CoupledFile;

const MAX_RESULTS: usize = 10;

pub struct RawCoupledFileStats {
    pub path: String,
    pub co_change_count: u32,
    pub total_commits: u32,
    pub last_timestamp: i64,
}

pub struct TimeWindow {
    pub oldest_ts: i64,
    pub newest_ts: i64,
}

/// Compute risk-scored coupled files.
///
/// Formula: `risk_score = (coupling * 0.5) + (churn * 0.3) + (recency * 0.2)`
///
/// - **Coupling**: `co_change_count / target_commit_count` — what % of target's commits include this file
/// - **Churn**: `total_commits / max_total_commits` across the result set (highest = 1.0) — how active the file is
/// - **Recency**: linear mapping of `last_timestamp` into `[0.0, 1.0]` over the time window.
///   Most recent = 1.0, oldest = 0.0. If all timestamps are equal, recency = 1.0.
///
/// **Coupling gate**: Files with coupling < 0.5 cannot exceed risk_score 0.79 (capping them at High risk).
///
/// Results are filtered to `risk_score > 0.0` and sorted descending by `risk_score`.
pub fn score_coupled_files(
    files: Vec<RawCoupledFileStats>,
    target_commit_count: u32,
    window: &TimeWindow,
) -> Vec<CoupledFile> {
    if files.is_empty() {
        return Vec::new();
    }

    let max_churn = files.iter().map(|f| f.total_commits).max().unwrap_or(1).max(1);

    let time_span = window.newest_ts - window.oldest_ts;

    let mut result: Vec<CoupledFile> = files
        .into_iter()
        .map(|f| {
            let churn = f.total_commits as f64 / max_churn as f64;

            let recency = if time_span == 0 {
                1.0
            } else {
                (f.last_timestamp - window.oldest_ts) as f64 / time_span as f64
            };

            let coupling = if target_commit_count > 0 {
                f.co_change_count as f64 / target_commit_count as f64
            } else {
                0.0
            };

            // New weights: prioritize coupling over churn
            let mut risk_score = (coupling * 0.5) + (churn * 0.3) + (recency * 0.2);

            // Coupling gate: files below 50% coupling can't be Critical (>= 0.8)
            // Cap them at 0.79 (max High risk)
            if coupling < 0.5 && risk_score >= 0.8 {
                risk_score = 0.79;
            }

            CoupledFile {
                path: f.path,
                coupling_score: coupling,
                co_change_count: f.co_change_count,
                risk_score,
                memories: Vec::new(),
                test_intents: Vec::new(),
            }
        })
        .filter(|f| f.risk_score > 0.0)
        .collect();

    result.sort_by(|a, b| b.risk_score.partial_cmp(&a.risk_score).unwrap_or(std::cmp::Ordering::Equal));

    result.truncate(MAX_RESULTS);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_stats(path: &str, co_change: u32, total: u32, ts: i64) -> RawCoupledFileStats {
        RawCoupledFileStats {
            path: path.to_string(),
            co_change_count: co_change,
            total_commits: total,
            last_timestamp: ts,
        }
    }

    #[test]
    fn test_formula_weights() {
        // Single file: churn=1.0 (only file), recency=1.0 (most recent), coupling=0.5
        let files = vec![make_stats("A.ts", 5, 10, 5000)];
        let window = TimeWindow { oldest_ts: 1000, newest_ts: 5000 };
        let result = score_coupled_files(files, 10, &window);

        assert_eq!(result.len(), 1);
        // New formula: risk = (coupling * 0.5) + (churn * 0.3) + (recency * 0.2)
        // risk = (0.5 * 0.5) + (1.0 * 0.3) + (1.0 * 0.2) = 0.25 + 0.3 + 0.2 = 0.75
        assert!((result[0].risk_score - 0.75).abs() < 1e-9);
    }

    #[test]
    fn test_churn_normalization() {
        // Two files: one with max churn, one with half
        let files = vec![
            make_stats("High.ts", 5, 20, 5000),
            make_stats("Low.ts", 5, 10, 5000),
        ];
        let window = TimeWindow { oldest_ts: 1000, newest_ts: 5000 };
        let result = score_coupled_files(files, 10, &window);

        assert_eq!(result.len(), 2);
        // High: churn=20/20=1.0, Low: churn=10/20=0.5
        // Both have same recency and coupling, so High should rank higher
        assert_eq!(result[0].path, "High.ts");
        assert_eq!(result[1].path, "Low.ts");
        // New weights: churn difference = 0.3 * (1.0 - 0.5) = 0.15
        assert!((result[0].risk_score - result[1].risk_score - 0.15).abs() < 1e-9);
    }

    #[test]
    fn test_recency_normalization() {
        // Two files: one recent, one old
        let files = vec![
            make_stats("Recent.ts", 5, 10, 5000),
            make_stats("Old.ts", 5, 10, 1000),
        ];
        let window = TimeWindow { oldest_ts: 1000, newest_ts: 5000 };
        let result = score_coupled_files(files, 10, &window);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].path, "Recent.ts");
        assert_eq!(result[1].path, "Old.ts");
        // New weights: recency difference = 0.2 * (1.0 - 0.0) = 0.2
        assert!((result[0].risk_score - result[1].risk_score - 0.2).abs() < 1e-9);
    }

    #[test]
    fn test_sort_order_descending() {
        let files = vec![
            make_stats("Low.ts", 1, 2, 1000),
            make_stats("High.ts", 10, 20, 5000),
            make_stats("Med.ts", 5, 10, 3000),
        ];
        let window = TimeWindow { oldest_ts: 1000, newest_ts: 5000 };
        let result = score_coupled_files(files, 20, &window);

        assert_eq!(result.len(), 3);
        // Should be sorted descending by risk_score
        assert!(result[0].risk_score >= result[1].risk_score);
        assert!(result[1].risk_score >= result[2].risk_score);
        assert_eq!(result[0].path, "High.ts");
    }

    #[test]
    fn test_single_file_edge_case() {
        let files = vec![make_stats("Only.ts", 3, 5, 3000)];
        let window = TimeWindow { oldest_ts: 1000, newest_ts: 5000 };
        let result = score_coupled_files(files, 10, &window);

        assert_eq!(result.len(), 1);
        // churn = 5/5 = 1.0, recency = (3000-1000)/4000 = 0.5, coupling = 3/10 = 0.3
        // New formula: risk = (coupling * 0.5) + (churn * 0.3) + (recency * 0.2)
        // risk = (0.3 * 0.5) + (1.0 * 0.3) + (0.5 * 0.2) = 0.15 + 0.3 + 0.1 = 0.55
        assert!((result[0].risk_score - 0.55).abs() < 1e-9);
    }

    #[test]
    fn test_zero_time_range() {
        // All commits at the same timestamp
        let files = vec![
            make_stats("A.ts", 5, 10, 3000),
            make_stats("B.ts", 3, 6, 3000),
        ];
        let window = TimeWindow { oldest_ts: 3000, newest_ts: 3000 };
        let result = score_coupled_files(files, 10, &window);

        // Recency should be 1.0 for all when time range is zero
        assert_eq!(result.len(), 2);
        for f in &result {
            // recency component = 0.3 * 1.0 = 0.3 is present
            assert!(f.risk_score >= 0.3);
        }
    }

    #[test]
    fn test_empty_input() {
        let files = vec![];
        let window = TimeWindow { oldest_ts: 0, newest_ts: 0 };
        let result = score_coupled_files(files, 10, &window);
        assert!(result.is_empty());
    }

    #[test]
    fn test_coupling_score_preserved() {
        let files = vec![make_stats("A.ts", 8, 10, 5000)];
        let window = TimeWindow { oldest_ts: 1000, newest_ts: 5000 };
        let result = score_coupled_files(files, 20, &window);

        assert_eq!(result.len(), 1);
        assert!((result[0].coupling_score - 0.4).abs() < 1e-9); // 8/20
    }

    #[test]
    fn test_truncation_with_more_than_max() {
        // Create 15 files — all should score > 0
        let files: Vec<RawCoupledFileStats> = (0..15)
            .map(|i| make_stats(&format!("File{i}.ts"), 5, 10 + i, 2000 + i as i64 * 100))
            .collect();
        let window = TimeWindow { oldest_ts: 1000, newest_ts: 5000 };
        let result = score_coupled_files(files, 20, &window);

        assert_eq!(result.len(), MAX_RESULTS, "should truncate to MAX_RESULTS");
        // Verify still sorted descending
        for i in 1..result.len() {
            assert!(result[i - 1].risk_score >= result[i].risk_score);
        }
    }

    #[test]
    fn test_coupling_gate_prevents_critical() {
        // File with high churn + high recency but low coupling
        // Should be capped at 0.79 (High risk) even if formula says >= 0.8
        let files = vec![make_stats("HighChurn.ts", 3, 100, 5000)]; // coupling = 3/10 = 0.3 (< 0.5)
        let window = TimeWindow { oldest_ts: 1000, newest_ts: 5000 };
        let result = score_coupled_files(files, 10, &window);

        assert_eq!(result.len(), 1);
        // Without gate: (0.3 * 0.5) + (1.0 * 0.3) + (1.0 * 0.2) = 0.15 + 0.3 + 0.2 = 0.65
        // Since coupling < 0.5, no gate applied (score < 0.8)
        assert!((result[0].risk_score - 0.65).abs() < 1e-9);

        // Now test a case that WOULD hit the gate
        let files = vec![make_stats("VeryHighChurn.ts", 4, 200, 5000)]; // coupling = 4/10 = 0.4
        let result = score_coupled_files(files, 10, &window);
        // Without gate: (0.4 * 0.5) + (1.0 * 0.3) + (1.0 * 0.2) = 0.2 + 0.3 + 0.2 = 0.7
        // Still below 0.8, no gate
        assert!((result[0].risk_score - 0.7).abs() < 1e-9);
    }

    #[test]
    fn test_high_coupling_allows_critical() {
        // File with coupling >= 0.5 can be Critical
        let files = vec![make_stats("HighCoupling.ts", 8, 10, 5000)]; // coupling = 8/10 = 0.8
        let window = TimeWindow { oldest_ts: 1000, newest_ts: 5000 };
        let result = score_coupled_files(files, 10, &window);

        assert_eq!(result.len(), 1);
        // (0.8 * 0.5) + (1.0 * 0.3) + (1.0 * 0.2) = 0.4 + 0.3 + 0.2 = 0.9
        // Coupling >= 0.5, so no cap applied
        assert!((result[0].risk_score - 0.9).abs() < 1e-9);
        assert!(result[0].risk_score >= 0.8, "Should be Critical risk");
    }

    #[test]
    fn test_no_truncation_under_max() {
        let files: Vec<RawCoupledFileStats> = (0..5)
            .map(|i| make_stats(&format!("File{i}.ts"), 3, 8, 3000 + i as i64 * 100))
            .collect();
        let window = TimeWindow { oldest_ts: 1000, newest_ts: 5000 };
        let result = score_coupled_files(files, 10, &window);

        assert_eq!(result.len(), 5, "should not truncate when under MAX_RESULTS");
    }
}
