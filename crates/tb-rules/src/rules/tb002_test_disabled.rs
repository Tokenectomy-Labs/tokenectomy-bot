use crate::model::{Confidence, Finding, RuleContext, Severity};
use crate::traits::Rule;
use tb_diff::FileKind;
use tb_parse::ParsedSource;

pub struct Tb002TestDisabled;

impl Rule for Tb002TestDisabled {
    fn id(&self) -> &'static str {
        "TB002"
    }

    fn name(&self) -> &'static str {
        "test-disabled"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, ctx: &RuleContext) -> Vec<Finding> {
        let mut findings = Vec::new();

        if ctx.file_diff.kind != FileKind::Test {
            return findings;
        }

        let new_parsed = match ctx.new_parsed {
            Some(p) => p,
            None => return findings,
        };

        let changed_ranges = ctx.file_diff.changed_line_ranges_new();
        if changed_ranges.is_empty() {
            return findings;
        }

        let skip_nodes = new_parsed.find_all_descendants(new_parsed.root_node(), &|node| {
            matches!(
                node.kind(),
                "call_expression" | "call" | "decorator" | "attribute_item"
            )
        });

        for node in skip_nodes {
            if !ParsedSource::node_overlaps_ranges(&node, &changed_ranges) {
                continue;
            }

            let text = new_parsed.node_text(&node).trim();

            let is_disabled = match node.kind() {
                "attribute_item" => text.contains("#[ignore"),
                "decorator" => {
                    text.contains("@pytest.mark.skip")
                        || text.contains("@pytest.mark.xfail")
                        || text.contains("@unittest.skip")
                        || text.starts_with("@skip(")
                }
                "call" | "call_expression" => {
                    text.starts_with("xit(")
                        || text.starts_with("xdescribe(")
                        || text.starts_with("describe.skip(")
                        || text.starts_with("describe.skip ")
                        || text.starts_with("it.skip(")
                        || text.starts_with("it.skip ")
                        || text.starts_with("test.skip(")
                        || text.starts_with("test.skip ")
                        || text.starts_with("it.todo(")
                        || text.starts_with("test.fixme(")
                        || text.starts_with("t.Skip(")
                        || text.starts_with("t.Skipf(")
                        || text.starts_with("t.SkipNow(")
                        || text.starts_with("pytest.skip(")
                }
                _ => false,
            };

            if is_disabled {
                let (start_line, end_line) = ParsedSource::node_line_range(&node);
                let (start_col, _) = ParsedSource::point_to_1indexed(node.start_position());
                let (_, end_col) = ParsedSource::point_to_1indexed(node.end_position());

                let mut finding = Finding::new(
                    self.id(),
                    self.name(),
                    self.default_severity(),
                    Confidence::High,
                    &ctx.file_diff.path,
                    start_line,
                    end_line,
                    start_col,
                    end_col,
                    format!("Test was disabled using: `{}`", text.lines().next().unwrap_or(text)),
                    Some("Do not skip tests to make CI green. Fix the underlying issue or update expected behavior.".to_string()),
                    node.kind(),
                    text,
                );

                if text.starts_with("xit(") {
                    let reenabled = text.replacen("xit(", "it(", 1);
                    finding = finding.with_auto_fix(crate::model::AutoFix::new(
                        reenabled,
                        start_line,
                        end_line,
                        start_col,
                        end_col,
                        "Re-enable test by replacing xit with it",
                    ));
                } else if text.starts_with("xdescribe(") {
                    let reenabled = text.replacen("xdescribe(", "describe(", 1);
                    finding = finding.with_auto_fix(crate::model::AutoFix::new(
                        reenabled,
                        start_line,
                        end_line,
                        start_col,
                        end_col,
                        "Re-enable test by replacing xdescribe with describe",
                    ));
                } else if text.contains(".skip(") {
                    let reenabled = text.replacen(".skip(", "(", 1);
                    finding = finding.with_auto_fix(crate::model::AutoFix::new(
                        reenabled,
                        start_line,
                        end_line,
                        start_col,
                        end_col,
                        "Re-enable skipped test",
                    ));
                }

                findings.push(finding);
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
    fn test_tb002_detects_skipped_test() {
        let code = r#"
            describe.skip("auth suite", () => {
                it("logs in", () => {
                    expect(true).toBe(true);
                });
            });
        "#
        .to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("auth.test.ts"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("auth.test.ts"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Test,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 1,
                old_lines: 1,
                new_start: 2,
                new_lines: 5,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(2),
                    content: "describe.skip(\"auth suite\", () => {".to_string(),
                }],
            }],
        };

        let rule = Tb002TestDisabled;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));

        assert!(!findings.is_empty());
        assert_eq!(findings[0].rule_id, "TB002");
    }

    #[test]
    fn test_tb002_detects_python_pytest_skip() {
        let code =
            "@pytest.mark.skip(reason=\"temporary\")\ndef test_payment():\n    pass\n".to_string();
        let parsed = ParsedSource::parse(&PathBuf::from("test_payment.py"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("test_payment.py"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Test,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 1,
                old_lines: 0,
                new_start: 1,
                new_lines: 1,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(1),
                    content: "@pytest.mark.skip(reason=\"temporary\")".to_string(),
                }],
            }],
        };

        let rule = Tb002TestDisabled;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB002");
    }

    #[test]
    fn test_tb002_detects_go_t_skip() {
        let code = "package auth\n\nfunc TestAuth(t *testing.T) {\n    t.Skip(\"flaky test\")\n}\n"
            .to_string();
        let parsed = ParsedSource::parse(&PathBuf::from("auth_test.go"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("auth_test.go"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Test,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 3,
                old_lines: 0,
                new_start: 4,
                new_lines: 1,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(4),
                    content: "    t.Skip(\"flaky test\")".to_string(),
                }],
            }],
        };

        let rule = Tb002TestDisabled;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB002");
    }

    #[test]
    fn test_tb002_detects_rust_ignore() {
        let code = "#[test]\n#[ignore]\nfn test_expensive() {}\n".to_string();
        let parsed = ParsedSource::parse(&PathBuf::from("tests/expensive_test.rs"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("tests/expensive_test.rs"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Test,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 1,
                old_lines: 0,
                new_start: 2,
                new_lines: 1,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(2),
                    content: "#[ignore]".to_string(),
                }],
            }],
        };

        let rule = Tb002TestDisabled;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB002");
    }
}
