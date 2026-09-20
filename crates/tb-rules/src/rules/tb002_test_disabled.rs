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

        let calls = new_parsed.find_all_descendants(new_parsed.root_node(), &|node| {
            node.kind() == "call_expression" || node.kind() == "member_expression"
        });

        for node in calls {
            if !ParsedSource::node_overlaps_ranges(&node, &changed_ranges) {
                continue;
            }

            let text = new_parsed.node_text(&node);

            let is_disabled = text.starts_with("xit(")
                || text.starts_with("xdescribe(")
                || text.contains(".skip(")
                || text.contains(".skip ")
                || text.contains("it.todo(")
                || text.contains("test.fixme(");

            if is_disabled {
                let (start_line, end_line) = ParsedSource::node_line_range(&node);
                let (start_col, _) = ParsedSource::point_to_1indexed(node.start_position());
                let (_, end_col) = ParsedSource::point_to_1indexed(node.end_position());

                findings.push(Finding::new(
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
                ));
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
        let findings = rule.check(&RuleContext {
            file_diff: &file_diff,
            old_parsed: None,
            new_parsed: Some(&parsed),
        });

        assert!(!findings.is_empty());
        assert_eq!(findings[0].rule_id, "TB002");
    }
}
