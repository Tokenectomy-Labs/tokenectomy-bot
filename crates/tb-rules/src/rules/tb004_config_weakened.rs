use crate::model::{Confidence, Finding, RuleContext, Severity};
use crate::traits::Rule;
use tb_diff::FileKind;

pub struct Tb004ConfigWeakened;

impl Rule for Tb004ConfigWeakened {
    fn id(&self) -> &'static str {
        "TB004"
    }

    fn name(&self) -> &'static str {
        "config-weakened"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, ctx: &RuleContext) -> Vec<Finding> {
        let mut findings = Vec::new();

        if ctx.file_diff.kind != FileKind::Config && ctx.file_diff.kind != FileKind::Ci {
            return findings;
        }

        for hunk in &ctx.file_diff.hunks {
            for line in &hunk.lines {
                if line.kind != tb_diff::LineKind::Added {
                    continue;
                }

                let text = line.content.trim();
                let lineno = line.new_lineno.unwrap_or(hunk.new_start);

                let mut reason: Option<(&'static str, &'static str)> = None;

                // 1. CI weakening
                if ctx.file_diff.kind == FileKind::Ci && text.contains("continue-on-error: true") {
                    reason = Some((
                        "CI step configured with `continue-on-error: true`",
                        "Do not bypass failing CI steps. Fix the failure instead.",
                    ));
                }

                // 2. tsconfig weakening
                if ctx.file_diff.kind == FileKind::Config {
                    if text.contains("\"strict\": false") || text.contains("\"strict\":false") {
                        reason = Some((
                            "TypeScript `strict` typechecking was disabled (`strict: false`)",
                            "Keep strict typechecking enabled to maintain type safety.",
                        ));
                    } else if text.contains("\"noImplicitAny\": false")
                        || text.contains("\"noImplicitAny\":false")
                    {
                        reason = Some((
                            "TypeScript `noImplicitAny` was disabled",
                            "Explicitly type your variables instead of disabling noImplicitAny.",
                        ));
                    } else if text.contains("passWithNoTests: true")
                        || text.contains("\"passWithNoTests\": true")
                    {
                        reason = Some((
                            "Test configuration weakened with `passWithNoTests: true`",
                            "Ensure test runner fails when tests are missing or broken.",
                        ));
                    }
                }

                if let Some((msg, fix)) = reason {
                    findings.push(Finding::new(
                        self.id(),
                        self.name(),
                        self.default_severity(),
                        Confidence::High,
                        &ctx.file_diff.path,
                        lineno,
                        lineno,
                        1,
                        text.len().max(1),
                        msg,
                        Some(fix.to_string()),
                        "config_line",
                        text,
                    ));
                }
            }
        }

        findings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tb_diff::{DiffLine, DiffStatus, FileDiff, Hunk, LineKind};

    #[test]
    fn test_tb004_detects_continue_on_error() {
        let file_diff = FileDiff {
            path: PathBuf::from(".github/workflows/ci.yml"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Ci,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 10,
                old_lines: 1,
                new_start: 10,
                new_lines: 2,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(11),
                    content: "        continue-on-error: true".to_string(),
                }],
            }],
        };

        let rule = Tb004ConfigWeakened;
        let findings = rule.check(&RuleContext {
            file_diff: &file_diff,
            old_parsed: None,
            new_parsed: None,
        });

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB004");
    }

    #[test]
    fn test_tb004_detects_tsconfig_strict_false() {
        let file_diff = FileDiff {
            path: PathBuf::from("tsconfig.json"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Config,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 5,
                old_lines: 1,
                new_start: 5,
                new_lines: 1,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(5),
                    content: "    \"strict\": false,".to_string(),
                }],
            }],
        };

        let rule = Tb004ConfigWeakened;
        let findings = rule.check(&RuleContext {
            file_diff: &file_diff,
            old_parsed: None,
            new_parsed: None,
        });

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB004");
    }
}
