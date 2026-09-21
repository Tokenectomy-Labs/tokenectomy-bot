use serde::Serialize;
use std::collections::HashSet;
use tb_diff::DiffResult;
use tb_rules::{Finding, Severity};

#[derive(Debug, Clone, Serialize)]
pub struct ReviewComment {
    pub path: String,
    pub line: usize,
    pub side: &'static str,
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewBatchPayload {
    pub event: &'static str,
    pub body: String,
    pub comments: Vec<ReviewComment>,
}

pub struct ReviewBatchGenerator;

impl ReviewBatchGenerator {
    /// Builds an HTTP 422-safe review batch payload for GitHub PR Reviews API.
    /// Only lines present in the right side of the diff (added/modified) are included as inline comments.
    pub fn build_payload(diff: &DiffResult, findings: &[Finding]) -> ReviewBatchPayload {
        let has_blocking = findings.iter().any(|f| f.severity == Severity::Error);
        let event = if has_blocking {
            "REQUEST_CHANGES"
        } else {
            "COMMENT"
        };

        // Collect all valid line numbers per file in the right-side diff
        let mut valid_diff_lines: std::collections::HashMap<String, HashSet<usize>> =
            std::collections::HashMap::new();

        for file in &diff.files {
            let key = file.path.to_string_lossy().replace('\\', "/");
            let mut line_set = HashSet::new();
            for line in file.added_lines() {
                line_set.insert(line);
            }
            valid_diff_lines.insert(key, line_set);
        }

        let mut comments = Vec::new();
        let mut outside_diff_count = 0;

        for f in findings {
            let file_key = f.file.to_string_lossy().replace('\\', "/");
            let is_on_diff_line = valid_diff_lines
                .get(&file_key)
                .map(|set| set.contains(&f.start_line))
                .unwrap_or(false);

            if is_on_diff_line {
                let mut body = format!(
                    "<!-- tokenectomy-bot:fingerprint:{} -->\n**{} ({})**: {}\n",
                    f.fingerprint, f.rule_id, f.rule_name, f.message
                );

                if let Some(ref hint) = f.fix_hint {
                    body.push_str(&format!("\n> 💡 **Saran**: {}\n", hint));
                }

                if let Some(ref fix) = f.auto_fix {
                    body.push_str(&format!("\n```suggestion\n{}\n```\n", fix.replacement));
                }

                comments.push(ReviewComment {
                    path: file_key,
                    line: f.start_line,
                    side: "RIGHT",
                    body,
                });
            } else {
                outside_diff_count += 1;
            }
        }

        let body = if findings.is_empty() {
            "✔ **Tokenectomy Bot**: PR verification passed with zero structural or security flaws."
                .to_string()
        } else if outside_diff_count > 0 {
            format!(
                "🗡️ **Tokenectomy Bot**: Menemukan {} masalah ({} inline, {} di luar rentang diff langsung). Lihat ringkasan PR untuk rincian lengkap.",
                findings.len(),
                comments.len(),
                outside_diff_count
            )
        } else {
            format!(
                "🗡️ **Tokenectomy Bot**: Menemukan {} masalah verifikasi pada PR ini.",
                findings.len()
            )
        };

        ReviewBatchPayload {
            event,
            body,
            comments,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tb_diff::{DiffLine, DiffStatus, FileDiff, FileKind, Hunk, LineKind};
    use tb_rules::Confidence;

    #[test]
    fn test_review_batch_prevents_422() {
        let diff = DiffResult {
            base_ref: None,
            head_ref: None,
            files: vec![FileDiff {
                path: PathBuf::from("src/test.ts"),
                old_path: None,
                status: DiffStatus::Modified,
                kind: FileKind::Source,
                is_binary: false,
                hunks: vec![Hunk {
                    old_start: 1,
                    old_lines: 1,
                    new_start: 10,
                    new_lines: 2,
                    lines: vec![
                        DiffLine {
                            kind: LineKind::Added,
                            old_lineno: None,
                            new_lineno: Some(10),
                            content: "console.log();".to_string(),
                        },
                        DiffLine {
                            kind: LineKind::Added,
                            old_lineno: None,
                            new_lineno: Some(11),
                            content: "const a = 1;".to_string(),
                        },
                    ],
                }],
            }],
        };

        let findings = vec![
            // Inside diff right-side
            Finding::new(
                "TB101",
                "silent-catch",
                Severity::Warn,
                Confidence::High,
                PathBuf::from("src/test.ts"),
                10,
                10,
                1,
                1,
                "msg 1",
                None,
                "node",
                "code",
            ),
            // Outside diff right-side (e.g. line 50) -> Must NOT be included in inline comments to avoid 422!
            Finding::new(
                "TB101",
                "silent-catch",
                Severity::Warn,
                Confidence::High,
                PathBuf::from("src/test.ts"),
                50,
                50,
                1,
                1,
                "msg 2",
                None,
                "node",
                "code",
            ),
        ];

        let payload = ReviewBatchGenerator::build_payload(&diff, &findings);
        assert_eq!(payload.comments.len(), 1);
        assert_eq!(payload.comments[0].line, 10);
    }
}
