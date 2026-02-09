use crate::persistence::Database;
use crate::types::{AnalysisResponse, MetricsResponse};
use std::error::Error;

// Event type constants to prevent typos
const EVENT_ANALYSIS: &str = "analysis";
const EVENT_ADD_NOTE: &str = "add_note";

/// Record an analysis event after analyze() completes.
pub fn record_analysis_event(
    db: &Database,
    response: &AnalysisResponse,
    repo_root: &str,
) -> Result<(), Box<dyn Error>> {
    let mut critical_count = 0;
    let mut high_count = 0;
    let mut medium_count = 0;
    let mut low_count = 0;
    let mut test_files_found = 0;
    let mut test_intents_total = 0;

    // Classify coupled files by risk score and count test intents
    for file in &response.coupled_files {
        // Risk classification
        if file.risk_score >= 0.8 {
            critical_count += 1;
        } else if file.risk_score >= 0.5 {
            high_count += 1;
        } else if file.risk_score >= 0.25 {
            medium_count += 1;
        } else {
            low_count += 1;
        }

        // Test intent counting
        if !file.test_intents.is_empty() {
            test_files_found += 1;
            test_intents_total += file.test_intents.len() as u32;
        }
    }

    db.insert_metrics_event(
        EVENT_ANALYSIS,
        Some(&response.file_path),
        response.coupled_files.len() as u32,
        critical_count,
        high_count,
        medium_count,
        low_count,
        test_files_found,
        test_intents_total,
        response.commit_count,
        response.analysis_time_ms,
        None,
        repo_root,
    )?;

    Ok(())
}

/// Record a note creation event.
pub fn record_note_event(
    db: &Database,
    note_id: i64,
    file_path: &str,
    repo_root: &str,
) -> Result<(), Box<dyn Error>> {
    db.insert_metrics_event(
        EVENT_ADD_NOTE,
        Some(file_path),
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        Some(note_id),
        repo_root,
    )?;

    Ok(())
}

/// Get aggregated metrics for a repository.
pub fn get_metrics(
    db: &Database,
    repo_root: &str,
) -> Result<MetricsResponse, Box<dyn Error>> {
    let summary = db.get_metrics_summary(repo_root)?;
    Ok(MetricsResponse {
        repo_root: repo_root.to_string(),
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CoupledFile, TestIntent};

    #[test]
    fn test_record_analysis_event() {
        let db = Database::in_memory().unwrap();

        let response = AnalysisResponse {
            file_path: "src/A.ts".to_string(),
            repo_root: "/repo".to_string(),
            coupled_files: vec![
                CoupledFile {
                    path: "src/B.ts".to_string(),
                    coupling_score: 0.9,
                    co_change_count: 10,
                    risk_score: 0.85,
                    memories: vec![],
                    test_intents: vec![],
                },
                CoupledFile {
                    path: "src/C.ts".to_string(),
                    coupling_score: 0.6,
                    co_change_count: 5,
                    risk_score: 0.6,
                    memories: vec![],
                    test_intents: vec![],
                },
            ],
            commit_count: 15,
            analysis_time_ms: 150,
            test_info: None,
            indexing_status: None,
        };

        record_analysis_event(&db, &response, "/repo").unwrap();

        let metrics = db.get_metrics_summary("/repo").unwrap();
        assert_eq!(metrics.total_analyses, 1);
        assert_eq!(metrics.total_coupled_files, 2);
        assert_eq!(metrics.critical_risk_count, 1);
        assert_eq!(metrics.high_risk_count, 1);
        assert_eq!(metrics.avg_analysis_time_ms, 150);
    }

    #[test]
    fn test_risk_classification() {
        let db = Database::in_memory().unwrap();

        let response = AnalysisResponse {
            file_path: "src/A.ts".to_string(),
            repo_root: "/repo".to_string(),
            coupled_files: vec![
                CoupledFile {
                    path: "critical.ts".to_string(),
                    coupling_score: 1.0,
                    co_change_count: 10,
                    risk_score: 0.8,
                    memories: vec![],
                    test_intents: vec![],
                },
                CoupledFile {
                    path: "high.ts".to_string(),
                    coupling_score: 0.7,
                    co_change_count: 7,
                    risk_score: 0.5,
                    memories: vec![],
                    test_intents: vec![],
                },
                CoupledFile {
                    path: "medium.ts".to_string(),
                    coupling_score: 0.4,
                    co_change_count: 4,
                    risk_score: 0.25,
                    memories: vec![],
                    test_intents: vec![],
                },
                CoupledFile {
                    path: "low.ts".to_string(),
                    coupling_score: 0.2,
                    co_change_count: 2,
                    risk_score: 0.1,
                    memories: vec![],
                    test_intents: vec![],
                },
            ],
            commit_count: 10,
            analysis_time_ms: 100,
            test_info: None,
            indexing_status: None,
        };

        record_analysis_event(&db, &response, "/repo").unwrap();

        let metrics = db.get_metrics_summary("/repo").unwrap();
        assert_eq!(metrics.critical_risk_count, 1);
        assert_eq!(metrics.high_risk_count, 1);
        assert_eq!(metrics.medium_risk_count, 1);
        assert_eq!(metrics.low_risk_count, 1);
    }

    #[test]
    fn test_test_intent_counting() {
        let db = Database::in_memory().unwrap();

        let response = AnalysisResponse {
            file_path: "src/A.ts".to_string(),
            repo_root: "/repo".to_string(),
            coupled_files: vec![
                CoupledFile {
                    path: "test1.ts".to_string(),
                    coupling_score: 0.5,
                    co_change_count: 5,
                    risk_score: 0.5,
                    memories: vec![],
                    test_intents: vec![
                        TestIntent {
                            title: "test 1".to_string(),
                        },
                        TestIntent {
                            title: "test 2".to_string(),
                        },
                    ],
                },
                CoupledFile {
                    path: "test2.ts".to_string(),
                    coupling_score: 0.4,
                    co_change_count: 4,
                    risk_score: 0.4,
                    memories: vec![],
                    test_intents: vec![TestIntent {
                        title: "test 3".to_string(),
                    }],
                },
                CoupledFile {
                    path: "notest.ts".to_string(),
                    coupling_score: 0.3,
                    co_change_count: 3,
                    risk_score: 0.3,
                    memories: vec![],
                    test_intents: vec![],
                },
            ],
            commit_count: 5,
            analysis_time_ms: 100,
            test_info: None,
            indexing_status: None,
        };

        record_analysis_event(&db, &response, "/repo").unwrap();

        let metrics = db.get_metrics_summary("/repo").unwrap();
        assert_eq!(metrics.test_files_found, 2);
        assert_eq!(metrics.test_intents_extracted, 3);
    }

    #[test]
    fn test_multiple_repos_isolation() {
        let db = Database::in_memory().unwrap();

        let response1 = AnalysisResponse {
            file_path: "src/A.ts".to_string(),
            repo_root: "/repo1".to_string(),
            coupled_files: vec![],
            commit_count: 5,
            analysis_time_ms: 100,
            test_info: None,
            indexing_status: None,
        };

        let response2 = AnalysisResponse {
            file_path: "src/B.ts".to_string(),
            repo_root: "/repo2".to_string(),
            coupled_files: vec![],
            commit_count: 10,
            analysis_time_ms: 200,
            test_info: None,
            indexing_status: None,
        };

        record_analysis_event(&db, &response1, "/repo1").unwrap();
        record_analysis_event(&db, &response2, "/repo2").unwrap();

        let metrics1 = db.get_metrics_summary("/repo1").unwrap();
        let metrics2 = db.get_metrics_summary("/repo2").unwrap();

        assert_eq!(metrics1.total_analyses, 1);
        assert_eq!(metrics2.total_analyses, 1);
        assert_eq!(metrics1.avg_analysis_time_ms, 100);
        assert_eq!(metrics2.avg_analysis_time_ms, 200);
    }

    #[test]
    fn test_average_analysis_time() {
        let db = Database::in_memory().unwrap();

        for i in 0..3 {
            let response = AnalysisResponse {
                file_path: format!("src/{i}.ts"),
                repo_root: "/repo".to_string(),
                coupled_files: vec![],
                commit_count: 5,
                analysis_time_ms: 100 + (i as u64 * 50),
                test_info: None,
                indexing_status: None,
            };
            record_analysis_event(&db, &response, "/repo").unwrap();
        }

        let metrics = db.get_metrics_summary("/repo").unwrap();
        assert_eq!(metrics.total_analyses, 3);
        // (100 + 150 + 200) / 3 = 150
        assert_eq!(metrics.avg_analysis_time_ms, 150);
    }

    #[test]
    fn test_empty_metrics() {
        let db = Database::in_memory().unwrap();
        let result = get_metrics(&db, "/nonexistent").unwrap();
        assert_eq!(result.summary.total_analyses, 0);
        assert_eq!(result.summary.total_coupled_files, 0);
    }

    #[test]
    fn test_record_note_event() {
        let db = Database::in_memory().unwrap();

        record_note_event(&db, 123, "src/A.ts", "/repo").unwrap();

        let metrics = db.get_metrics_summary("/repo").unwrap();
        assert_eq!(metrics.notes_created, 1);
    }
}
