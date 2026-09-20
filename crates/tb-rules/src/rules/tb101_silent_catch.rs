use crate::model::{Confidence, Finding, RuleContext, Severity};
use crate::traits::Rule;
use tb_parse::ParsedSource;

pub struct Tb101SilentCatch;

impl Rule for Tb101SilentCatch {
    fn id(&self) -> &'static str {
        "TB101"
    }

    fn name(&self) -> &'static str {
        "silent-catch"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warn
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

        // 1. Check JS/TS catch clauses
        let catch_clauses = new_parsed.find_all_descendants(new_parsed.root_node(), &|node| {
            node.kind() == "catch_clause"
        });

        for node in catch_clauses {
            if !ParsedSource::node_overlaps_ranges(&node, &changed_ranges) {
                continue;
            }

            // Find statement_block inside catch_clause
            let body = (0..node.child_count())
                .filter_map(|i| node.child(i))
                .find(|c| c.kind() == "statement_block");

            if let Some(block) = body {
                let text = new_parsed.node_text(&block).trim();
                // strip { and }
                let inner = text.strip_prefix('{').unwrap_or(text);
                let inner = inner.strip_suffix('}').unwrap_or(inner).trim();

                let is_empty_or_comments_only = inner.is_empty()
                    || inner.lines().all(|l| {
                        let trimmed = l.trim();
                        trimmed.is_empty()
                            || trimmed.starts_with("//")
                            || trimmed.starts_with("/*")
                            || trimmed.starts_with('*')
                    });

                if is_empty_or_comments_only {
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
                        "Empty catch block swallows errors silently without logging, throwing, or handling."
                            .to_string(),
                        Some("Log the error with a logger, re-throw, or return a fallback result.".to_string()),
                        node.kind(),
                        text,
                    ));
                }
            }
        }

        // 2. Check .catch(() => {}) promise pattern
        let calls = new_parsed.find_all_descendants(new_parsed.root_node(), &|node| {
            node.kind() == "call_expression"
        });

        for node in calls {
            if !ParsedSource::node_overlaps_ranges(&node, &changed_ranges) {
                continue;
            }

            let text = new_parsed.node_text(&node);
            if text.contains(".catch(") {
                let is_noop = text.contains(".catch(() => {})")
                    || text.contains(".catch(noop)")
                    || text.contains(".catch((_) => {})")
                    || text.contains(".catch(function() {})")
                    || text.contains(".catch(function(e) {})");

                if is_noop {
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
                        "Promise `.catch()` swallows rejected promises without handling or logging.".to_string(),
                        Some("Handle the error or log it appropriately.".to_string()),
                        node.kind(),
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
    fn test_silent_catch_detected() {
        let code = r#"
            try {
                doSomething();
            } catch (err) {
                // Ignore error
            }
        "#
        .to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("service.ts"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("service.ts"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 1,
                old_lines: 1,
                new_start: 4,
                new_lines: 3,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(4),
                    content: "catch (err) {".to_string(),
                }],
            }],
        };

        let rule = Tb101SilentCatch;
        let findings = rule.check(&RuleContext {
            file_diff: &file_diff,
            old_parsed: None,
            new_parsed: Some(&parsed),
        });

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB101");
    }
}
