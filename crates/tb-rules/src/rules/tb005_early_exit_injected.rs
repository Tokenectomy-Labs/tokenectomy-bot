use crate::model::{Confidence, Finding, RuleContext, Severity};
use crate::traits::Rule;
use tb_parse::ParsedSource;

pub struct Tb005EarlyExitInjected;

impl Rule for Tb005EarlyExitInjected {
    fn id(&self) -> &'static str {
        "TB005"
    }

    fn name(&self) -> &'static str {
        "early-exit-injected"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, ctx: &RuleContext) -> Vec<Finding> {
        let mut findings = Vec::new();

        let new_parsed = match ctx.new_parsed {
            Some(p) => p,
            None => return findings,
        };

        let changed_ranges = ctx.file_diff.changed_line_ranges_new();
        if changed_ranges.is_empty() {
            return findings;
        }

        // Find statement blocks
        let blocks = new_parsed.find_all_descendants(new_parsed.root_node(), &|node| {
            node.kind() == "statement_block"
        });

        for block in blocks {
            let child_count = block.child_count();
            if child_count < 3 {
                // Not enough statements to have dead code (at least '{', statement1, statement2, '}')
                continue;
            }

            // Iterate over statements in block
            for i in 0..child_count {
                let stmt = match block.child(i) {
                    Some(s) => s,
                    None => continue,
                };

                let kind = stmt.kind();
                let is_exit = if kind == "return_statement" {
                    true
                } else if kind == "expression_statement" {
                    let text = new_parsed.node_text(&stmt).trim();
                    text.starts_with("process.exit(")
                } else {
                    false
                };

                if !is_exit {
                    continue;
                }

                // Check if this exit statement was added in this diff
                if !ParsedSource::node_overlaps_ranges(&stmt, &changed_ranges) {
                    continue;
                }

                // Check if there are non-trivial statements after this in the same block
                let has_unreachable_code = (i + 1..child_count).any(|next_idx| {
                    if let Some(next_stmt) = block.child(next_idx) {
                        let k = next_stmt.kind();
                        k != "}" && k != "comment" && !k.is_empty()
                    } else {
                        false
                    }
                });

                if has_unreachable_code {
                    let (start_line, end_line) = ParsedSource::node_line_range(&stmt);
                    let (start_col, _) = ParsedSource::point_to_1indexed(stmt.start_position());
                    let (_, end_col) = ParsedSource::point_to_1indexed(stmt.end_position());
                    let text = new_parsed.node_text(&stmt).trim();

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
                        format!("Injected early exit (`{}`) makes subsequent block statements unreachable dead code.", text.lines().next().unwrap_or(text)),
                        Some("Remove artificial early return/exit or place it conditionally where appropriate.".to_string()),
                        stmt.kind(),
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
    use tb_diff::{DiffLine, DiffStatus, FileDiff, FileKind, Hunk, LineKind};

    #[test]
    fn test_tb005_detects_unreachable_early_return() {
        let code = r#"
            function executeTask() {
                return; // Early return injected by AI
                doExpensiveOperation();
                cleanup();
            }
        "#
        .to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("task.ts"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("task.ts"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 3,
                old_lines: 0,
                new_start: 3,
                new_lines: 1,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(3),
                    content: "                return;".to_string(),
                }],
            }],
        };

        let rule = Tb005EarlyExitInjected;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB005");
    }
}
