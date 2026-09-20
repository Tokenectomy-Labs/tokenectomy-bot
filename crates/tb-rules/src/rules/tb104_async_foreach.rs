use crate::model::{Confidence, Finding, RuleContext, Severity};
use crate::traits::Rule;
use tb_parse::ParsedSource;

pub struct Tb104AsyncForeach;

impl Rule for Tb104AsyncForeach {
    fn id(&self) -> &'static str {
        "TB104"
    }

    fn name(&self) -> &'static str {
        "async-foreach"
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

        let calls = new_parsed.find_all_descendants(new_parsed.root_node(), &|node| {
            node.kind() == "call_expression"
        });

        for node in calls {
            if !ParsedSource::node_overlaps_ranges(&node, &changed_ranges) {
                continue;
            }

            let text = new_parsed.node_text(&node);
            if !text.contains(".forEach(") {
                continue;
            }

            // Check if arguments contain async function or async arrow
            let args = (0..node.child_count())
                .filter_map(|i| node.child(i))
                .find(|c| c.kind() == "arguments");

            if let Some(arg_list) = args {
                let is_async_callback = (0..arg_list.child_count())
                    .filter_map(|i| arg_list.child(i))
                    .any(|arg| {
                        let k = arg.kind();
                        if k == "arrow_function" || k == "function_expression" {
                            let arg_text = new_parsed.node_text(&arg).trim();
                            arg_text.starts_with("async ") || arg_text.starts_with("async(")
                        } else {
                            false
                        }
                    });

                if is_async_callback {
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
                        "Array `forEach` with async callback does not await promises, causing unhandled concurrency races.",
                        Some("Use `for (const item of items)` with `await` or `Promise.all(items.map(async ...))`.".to_string()),
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
    fn test_tb104_detects_async_foreach() {
        let code = r#"
            async function syncItems(items: string[]) {
                items.forEach(async (item) => {
                    await upload(item);
                });
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
                old_start: 3,
                old_lines: 0,
                new_start: 3,
                new_lines: 3,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(3),
                    content: "                items.forEach(async (item) => {".to_string(),
                }],
            }],
        };

        let rule = Tb104AsyncForeach;
        let findings = rule.check(&RuleContext {
            file_diff: &file_diff,
            old_parsed: None,
            new_parsed: Some(&parsed),
        });

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB104");
    }
}
