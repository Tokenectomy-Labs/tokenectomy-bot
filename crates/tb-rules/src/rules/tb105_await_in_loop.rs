use crate::model::{Confidence, Finding, RuleContext, Severity};
use crate::traits::Rule;
use tb_parse::ParsedSource;

pub struct Tb105AwaitInLoop;

impl Rule for Tb105AwaitInLoop {
    fn id(&self) -> &'static str {
        "TB105"
    }

    fn name(&self) -> &'static str {
        "await-in-loop"
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

        // Find loop statements across languages
        let loop_nodes = new_parsed.find_all_descendants(new_parsed.root_node(), &|node| {
            matches!(
                node.kind(),
                "for_statement"
                    | "for_in_statement"
                    | "while_statement"
                    | "do_statement"
                    | "for_expression"
                    | "while_expression"
            )
        });

        for loop_node in loop_nodes {
            // Find all await expressions inside this loop
            let awaits = new_parsed.find_all_descendants(loop_node, &|node| {
                matches!(node.kind(), "await_expression" | "await")
            });

            for await_node in awaits {
                if !ParsedSource::node_overlaps_ranges(&await_node, &changed_ranges) {
                    continue;
                }

                let text = new_parsed.node_text(&await_node).trim();

                // Check if the awaited call performs database, network, or external I/O
                let is_io_call = text.contains("fetch(")
                    || text.contains("axios.")
                    || text.contains("http.")
                    || text.contains(".query(")
                    || text.contains(".findMany(")
                    || text.contains(".find(")
                    || text.contains(".findOne(")
                    || text.contains("db.")
                    || text.contains("prisma.")
                    || text.contains("drizzle.")
                    || text.contains(".request(")
                    || text.contains(".send(");

                if is_io_call {
                    let (start_line, end_line) = ParsedSource::node_line_range(&await_node);
                    let (start_col, _) =
                        ParsedSource::point_to_1indexed(await_node.start_position());
                    let (_, end_col) = ParsedSource::point_to_1indexed(await_node.end_position());

                    let first_line = text.lines().next().unwrap_or(text);

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
                        format!(
                            "Awaiting I/O operation inside loop (`{}`). Causes serial execution bottleneck (N+1 query pattern).",
                            first_line
                        ),
                        Some("Execute batch queries or collect promises and await them concurrently using `Promise.all(items.map(...))`.".to_string()),
                        await_node.kind(),
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
    fn test_tb105_detects_await_in_for_loop() {
        let code = r#"
            async function fetchUsers(ids: string[]) {
                const results = [];
                for (const id of ids) {
                    const user = await fetch(`https://api.example.com/users/${id}`);
                    results.push(user);
                }
                return results;
            }
        "#
        .to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("users.ts"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("users.ts"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 4,
                old_lines: 1,
                new_start: 4,
                new_lines: 3,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(5),
                    content: "                    const user = await fetch(`https://api.example.com/users/${id}`);".to_string(),
                }],
            }],
        };

        let rule = Tb105AwaitInLoop;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB105");
        assert!(findings[0].message.contains("loop"));
    }

    #[test]
    fn test_tb105_detects_await_db_query_in_while_loop() {
        let code = r#"
            async function processQueue() {
                while (hasItems()) {
                    await db.query("UPDATE queue SET status = 1 WHERE id = $1", [nextId()]);
                }
            }
        "#
        .to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("queue.ts"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("queue.ts"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 3,
                old_lines: 1,
                new_start: 3,
                new_lines: 2,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(4),
                    content: "                    await db.query(\"UPDATE queue SET status = 1 WHERE id = $1\", [nextId()]);".to_string(),
                }],
            }],
        };

        let rule = Tb105AwaitInLoop;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB105");
    }

    #[test]
    fn test_tb105_ignores_promise_all_outside_loop() {
        let code = r#"
            async function fetchAll(ids: string[]) {
                return await Promise.all(ids.map(id => fetch(`/users/${id}`)));
            }
        "#
        .to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("users.ts"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("users.ts"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 2,
                old_lines: 1,
                new_start: 2,
                new_lines: 2,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(2),
                    content: "                return await Promise.all(ids.map(id => fetch(`/users/${id}`)));".to_string(),
                }],
            }],
        };

        let rule = Tb105AwaitInLoop;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert!(findings.is_empty());
    }
}
