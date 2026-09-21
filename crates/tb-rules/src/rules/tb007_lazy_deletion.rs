use crate::model::{Confidence, Finding, RuleContext, Severity};
use crate::traits::Rule;
use tb_parse::ParsedSource;

pub struct Tb007LazyDeletion;

impl Rule for Tb007LazyDeletion {
    fn id(&self) -> &'static str {
        "TB007"
    }

    fn name(&self) -> &'static str {
        "lazy-deletion"
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

        // Find function and method definitions across supported languages
        let fn_nodes = new_parsed.find_all_descendants(new_parsed.root_node(), &|node| {
            matches!(
                node.kind(),
                "function_declaration"
                    | "function_item"
                    | "function_definition"
                    | "method_definition"
                    | "method_declaration"
                    | "arrow_function"
                    | "function_expression"
            )
        });

        for fn_node in fn_nodes {
            if !ParsedSource::node_overlaps_ranges(&fn_node, &changed_ranges) {
                continue;
            }

            // Locate the body of the function
            let body_opt = fn_node
                .children(&mut fn_node.walk())
                .find(|c| matches!(c.kind(), "statement_block" | "block"));

            let body = match body_opt {
                Some(b) => b,
                None => continue,
            };

            // Inspect non-structural statements in the body
            let mut meaningful_statements = Vec::new();
            for i in 0..body.child_count() {
                if let Some(child) = body.child(i) {
                    let k = child.kind();
                    if k != "{" && k != "}" && k != "comment" && !k.is_empty() {
                        meaningful_statements.push(child);
                    }
                }
            }

            // If the entire function body was gutted to a single lazy stub statement
            if meaningful_statements.len() == 1 {
                let stmt = meaningful_statements[0];
                let text = new_parsed.node_text(&stmt).trim();
                let text_lower = text.to_lowercase();

                let is_lazy_stub =
                    // 1. Surrender dummies: todo!(), unimplemented!(), panic!("not implemented")
                    text.starts_with("todo!(")
                    || text.starts_with("unimplemented!(")
                    || text_lower.starts_with("panic!(\"todo")
                    || text_lower.starts_with("panic!(\"not implemented")
                    || text_lower.starts_with("panic(\"not implemented")
                    || text_lower.starts_with("panic(\"todo")
                    // 2. Python NotImplementedError or pass
                    || text == "pass"
                    || text.starts_with("raise NotImplementedError")
                    // 3. JS/TS throw new Error("not implemented") / throw "TODO"
                    || (text.starts_with("throw ") && (text_lower.contains("not implemented") || text_lower.contains("todo") || text_lower.contains("unimplemented")))
                    // 4. Return null / undefined / false / empty dummy returns
                    || text == "return null;"
                    || text == "return undefined;"
                    || text == "return false;"
                    || text == "return \"\";"
                    || text == "return 0;"
                    || text == "return None"
                    || text == "return False"
                    || text == "return nil"
                    || text == "return;";

                if is_lazy_stub {
                    let (start_line, end_line) = ParsedSource::node_line_range(&stmt);
                    let (start_col, _) = ParsedSource::point_to_1indexed(stmt.start_position());
                    let (_, end_col) = ParsedSource::point_to_1indexed(stmt.end_position());

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
                            "Function body gutted to lazy surrender stub (`{}`). Real business logic was deleted or unaddressed.",
                            text.lines().next().unwrap_or(text)
                        ),
                        Some("Restore genuine function implementation instead of surrendering with a dummy return or todo!() stub.".to_string()),
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
    fn test_tb007_detects_gutted_return_null() {
        let code = r#"
            function processPayment(amount: number) {
                return null;
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
                old_start: 2,
                old_lines: 5,
                new_start: 2,
                new_lines: 3,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(3),
                    content: "                return null;".to_string(),
                }],
            }],
        };

        let rule = Tb007LazyDeletion;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB007");
        assert!(findings[0].message.contains("return null;"));
    }

    #[test]
    fn test_tb007_detects_rust_todo() {
        let code = r#"
            fn authenticate(user: &str) -> bool {
                todo!("implement later");
            }
        "#
        .to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("auth.rs"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("auth.rs"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 2,
                old_lines: 4,
                new_start: 2,
                new_lines: 3,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(3),
                    content: "                todo!(\"implement later\");".to_string(),
                }],
            }],
        };

        let rule = Tb007LazyDeletion;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("todo!"));
    }

    #[test]
    fn test_tb007_detects_python_not_implemented() {
        let code =
            "def calculate_discount(price):\n    raise NotImplementedError(\"TODO\")\n".to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("calc.py"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("calc.py"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 1,
                old_lines: 3,
                new_start: 1,
                new_lines: 2,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(2),
                    content: "    raise NotImplementedError(\"TODO\")".to_string(),
                }],
            }],
        };

        let rule = Tb007LazyDeletion;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("NotImplementedError"));
    }

    #[test]
    fn test_tb007_ignores_genuine_logic() {
        let code = r#"
            function processPayment(amount: number) {
                const validated = validate(amount);
                return executeGateway(validated);
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
                old_lines: 4,
                new_start: 1,
                new_lines: 4,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(3),
                    content: "                const validated = validate(amount);".to_string(),
                }],
            }],
        };

        let rule = Tb007LazyDeletion;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert!(findings.is_empty());
    }
}
