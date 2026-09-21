use crate::model::{Confidence, Finding, RuleContext, Severity};
use crate::traits::Rule;
use std::collections::HashSet;
use tb_parse::ParsedSource;

pub struct Tb103FloatingPromise;

impl Rule for Tb103FloatingPromise {
    fn id(&self) -> &'static str {
        "TB103"
    }

    fn name(&self) -> &'static str {
        "floating-promise"
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

        // 1. Collect names of all functions explicitly declared as `async` in the current file
        let mut local_async_functions = HashSet::new();
        let fn_declarations = new_parsed.find_all_descendants(new_parsed.root_node(), &|node| {
            matches!(
                node.kind(),
                "function_declaration" | "function_definition" | "method_definition"
            )
        });

        for fn_node in fn_declarations {
            let fn_text = new_parsed.node_text(&fn_node);
            if !fn_text.starts_with("async ") && !fn_text.starts_with("async\n") {
                continue;
            }
            if let Some(id_node) = fn_node
                .children(&mut fn_node.walk())
                .find(|c| c.kind() == "identifier")
            {
                let name = new_parsed.node_text(&id_node).trim();
                if !name.is_empty() {
                    local_async_functions.insert(name.to_string());
                }
            }
        }

        // 2. Scan all expression statements in the file
        let expr_statements = new_parsed.find_all_descendants(new_parsed.root_node(), &|node| {
            node.kind() == "expression_statement"
        });

        for stmt in expr_statements {
            if !ParsedSource::node_overlaps_ranges(&stmt, &changed_ranges) {
                continue;
            }

            let text = new_parsed.node_text(&stmt).trim();

            // Safe patterns: void, await, assignment, return
            if text.starts_with("await ")
                || text.starts_with("void ")
                || text.starts_with("return ")
                || text.contains(".catch(")
            {
                continue;
            }

            // Check if statement is a call to a known async API or local async function
            let is_known_async_api = text.starts_with("fetch(")
                || text.starts_with("axios(")
                || text.starts_with("axios.get(")
                || text.starts_with("axios.post(")
                || text.starts_with("axios.put(")
                || text.starts_with("axios.delete(")
                || text.starts_with("fs.promises.")
                || text.starts_with("Promise.all(")
                || text.starts_with("Promise.allSettled(")
                || text.starts_with("Promise.resolve(")
                || text.starts_with("Promise.reject(");

            let is_local_async_call = if let Some(first_word) = text.split('(').next() {
                let func_name = first_word.trim();
                local_async_functions.contains(func_name)
            } else {
                false
            };

            if is_known_async_api || is_local_async_call {
                let (start_line, end_line) = ParsedSource::node_line_range(&stmt);
                let (start_col, _) = ParsedSource::point_to_1indexed(stmt.start_position());
                let (_, end_col) = ParsedSource::point_to_1indexed(stmt.end_position());

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
                        "Floating unhandled promise detected (`{}`). Asynchronous operation is not awaited or handled.",
                        first_line
                    ),
                    Some("Add `await`, `void`, `return`, or attach a `.catch()` error handler to prevent unhandled promise rejections.".to_string()),
                    stmt.kind(),
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
    use tb_diff::{DiffLine, DiffStatus, FileDiff, FileKind, Hunk, LineKind};

    #[test]
    fn test_tb103_detects_floating_fetch() {
        let code = r#"
            function syncData() {
                fetch("https://api.example.com/sync");
                console.log("Sync initiated");
            }
        "#
        .to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("sync.ts"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("sync.ts"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 2,
                old_lines: 2,
                new_start: 2,
                new_lines: 3,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(3),
                    content: "                fetch(\"https://api.example.com/sync\");".to_string(),
                }],
            }],
        };

        let rule = Tb103FloatingPromise;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB103");
        assert!(findings[0].message.contains("fetch"));
    }

    #[test]
    fn test_tb103_detects_local_async_function_call() {
        let code = r#"
            async function saveProfile() {
                return true;
            }

            function handleForm() {
                saveProfile();
            }
        "#
        .to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("profile.ts"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("profile.ts"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 6,
                old_lines: 1,
                new_start: 6,
                new_lines: 3,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(7),
                    content: "                saveProfile();".to_string(),
                }],
            }],
        };

        let rule = Tb103FloatingPromise;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("saveProfile"));
    }

    #[test]
    fn test_tb103_ignores_awaited_or_void_promise() {
        let code = r#"
            async function sync() {
                await fetch("https://api.example.com/sync");
                void fetch("https://api.example.com/telemetry");
                fetch("https://api.example.com/log").catch(console.error);
            }
        "#
        .to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("sync.ts"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("sync.ts"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 2,
                old_lines: 1,
                new_start: 2,
                new_lines: 4,
                lines: vec![
                    DiffLine {
                        kind: LineKind::Added,
                        old_lineno: None,
                        new_lineno: Some(3),
                        content: "                await fetch(\"https://api.example.com/sync\");".to_string(),
                    },
                    DiffLine {
                        kind: LineKind::Added,
                        old_lineno: None,
                        new_lineno: Some(4),
                        content: "                void fetch(\"https://api.example.com/telemetry\");".to_string(),
                    },
                    DiffLine {
                        kind: LineKind::Added,
                        old_lineno: None,
                        new_lineno: Some(5),
                        content: "                fetch(\"https://api.example.com/log\").catch(console.error);".to_string(),
                    },
                ],
            }],
        };

        let rule = Tb103FloatingPromise;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert!(findings.is_empty());
    }
}
