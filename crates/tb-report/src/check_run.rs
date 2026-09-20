use serde::Serialize;
use tb_rules::{Finding, Severity};

pub const MAX_GITHUB_ANNOTATIONS_PER_BATCH: usize = 50;

#[derive(Debug, Clone, Serialize)]
pub struct CheckAnnotation {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub start_column: usize,
    pub end_column: usize,
    pub annotation_level: &'static str,
    pub message: String,
    pub title: String,
}

pub struct CheckRunBatcher;

impl CheckRunBatcher {
    pub fn batch_annotations(findings: &[Finding]) -> Vec<Vec<CheckAnnotation>> {
        let annotations: Vec<CheckAnnotation> = findings
            .iter()
            .map(|f| {
                let level = match f.severity {
                    Severity::Error => "failure",
                    Severity::Warn => "warning",
                    Severity::Info => "notice",
                };

                let title = format!("{} ({})", f.rule_id, f.rule_name);
                let message = if let Some(ref hint) = f.fix_hint {
                    format!("{}. Fix: {}", f.message, hint)
                } else {
                    f.message.clone()
                };

                CheckAnnotation {
                    path: f.file.to_string_lossy().replace('\\', "/"),
                    start_line: f.start_line,
                    end_line: f.end_line,
                    start_column: f.start_col,
                    end_column: f.end_col,
                    annotation_level: level,
                    message,
                    title,
                }
            })
            .collect();

        annotations
            .chunks(MAX_GITHUB_ANNOTATIONS_PER_BATCH)
            .map(|chunk| chunk.to_vec())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tb_rules::Confidence;

    #[test]
    fn test_batch_splits_at_50() {
        let findings: Vec<Finding> = (0..120)
            .map(|i| {
                Finding::new(
                    "TB101",
                    "silent-catch",
                    Severity::Warn,
                    Confidence::High,
                    PathBuf::from(format!("src/file_{}.ts", i)),
                    1,
                    1,
                    1,
                    1,
                    "msg",
                    None,
                    "node",
                    "code",
                )
            })
            .collect();

        let batches = CheckRunBatcher::batch_annotations(&findings);
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].len(), 50);
        assert_eq!(batches[1].len(), 50);
        assert_eq!(batches[2].len(), 20);
    }
}
