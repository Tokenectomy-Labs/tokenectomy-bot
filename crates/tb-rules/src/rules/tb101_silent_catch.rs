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
                    let (start_col, end_col) = ParsedSource::node_col_range(&node);

                    let mut param = "err";
                    if let Some(param_node) = (0..node.child_count())
                        .filter_map(|i| node.child(i))
                        .find(|c| c.kind() == "catch_parameter" || c.kind() == "identifier")
                    {
                        let p_text = new_parsed.node_text(&param_node).trim();
                        let p_clean = p_text.trim_matches(|c| c == '(' || c == ')').trim();
                        if !p_clean.is_empty() {
                            param = p_clean;
                        }
                    }

                    let finding = Finding::new(
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
                    ).with_auto_fix(crate::model::AutoFix::new(
                        format!("catch ({}) {{\n    console.error({});\n}}", param, param),
                        start_line,
                        end_line,
                        start_col,
                        end_col,
                        "Log caught error to console",
                    ));
                    findings.push(finding);
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
                        "Promise .catch() with empty handler swallows errors silently.".to_string(),
                        Some("Handle the error or log it.".to_string()),
                        node.kind(),
                        text,
                    ));
                }
            }
        }

        // 3. Check Python except clauses
        let except_clauses = new_parsed.find_all_descendants(new_parsed.root_node(), &|node| {
            node.kind() == "except_clause"
        });

        for node in except_clauses {
            if !ParsedSource::node_overlaps_ranges(&node, &changed_ranges) {
                continue;
            }

            let text = new_parsed.node_text(&node).trim();
            if let Some((_, body)) = text.split_once(':') {
                let trimmed_body = body.trim();
                let is_pass_or_empty = trimmed_body.is_empty()
                    || trimmed_body == "pass"
                    || trimmed_body.lines().all(|l| {
                        let t = l.trim();
                        t.is_empty() || t == "pass" || t.starts_with('#')
                    });

                if is_pass_or_empty {
                    let (start_line, end_line) = ParsedSource::node_line_range(&node);
                    let (start_col, _) = ParsedSource::point_to_1indexed(node.start_position());
                    let (_, end_col) = ParsedSource::point_to_1indexed(node.end_position());

                    let header = text
                        .split_once(':')
                        .map(|(h, _)| h)
                        .unwrap_or("except Exception as err");
                    let fixed_py = format!("{}:\n    logging.exception(err)", header.trim());

                    let finding = Finding::new(
                        self.id(),
                        self.name(),
                        self.default_severity(),
                        Confidence::High,
                        &ctx.file_diff.path,
                        start_line,
                        end_line,
                        start_col,
                        end_col,
                        "Empty or pass-only except block swallows errors silently without logging or re-raising."
                            .to_string(),
                        Some("Log the error with logging or re-raise with raise.".to_string()),
                        node.kind(),
                        text,
                    ).with_auto_fix(crate::model::AutoFix::new(
                        fixed_py,
                        start_line,
                        end_line,
                        start_col,
                        end_col,
                        "Log exception with logging.exception",
                    ));
                    findings.push(finding);
                }
            }
        }

        // 4. Check Go if err != nil empty blocks
        let if_statements = new_parsed.find_all_descendants(new_parsed.root_node(), &|node| {
            node.kind() == "if_statement"
        });

        for node in if_statements {
            if !ParsedSource::node_overlaps_ranges(&node, &changed_ranges) {
                continue;
            }

            let text = new_parsed.node_text(&node).trim();
            if text.contains("err != nil")
                && let Some(block) = (0..node.child_count())
                    .filter_map(|i| node.child(i))
                    .find(|c| c.kind() == "block")
            {
                let b_text = new_parsed.node_text(&block).trim();
                let inner = b_text.strip_prefix('{').unwrap_or(b_text);
                let inner = inner.strip_suffix('}').unwrap_or(inner).trim();

                let is_empty_or_comments = inner.is_empty()
                    || inner.lines().all(|l| {
                        let t = l.trim();
                        t.is_empty()
                            || t.starts_with("//")
                            || t.starts_with("/*")
                            || t.starts_with('*')
                    });

                if is_empty_or_comments {
                    let (start_line, end_line) = ParsedSource::node_line_range(&node);
                    let (start_col, _) = ParsedSource::point_to_1indexed(node.start_position());
                    let (_, end_col) = ParsedSource::point_to_1indexed(node.end_position());

                    let finding = Finding::new(
                        self.id(),
                        self.name(),
                        self.default_severity(),
                        Confidence::High,
                        &ctx.file_diff.path,
                        start_line,
                        end_line,
                        start_col,
                        end_col,
                        "Empty if err != nil block swallows errors silently without handling or returning."
                            .to_string(),
                        Some("Handle the error, log it, or return err.".to_string()),
                        node.kind(),
                        text,
                    ).with_auto_fix(crate::model::AutoFix::new(
                        "if err != nil {\n\treturn fmt.Errorf(\"operation failed: %w\", err)\n}",
                        start_line,
                        end_line,
                        start_col,
                        end_col,
                        "Return wrapped error with fmt.Errorf",
                    ));
                    findings.push(finding);
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
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB101");
        assert!(findings[0].auto_fix.is_some());
    }

    #[test]
    fn test_tb101_detects_python_pass_except() {
        let code = "def fetch():\n    try:\n        call()\n    except Exception:\n        pass\n"
            .to_string();
        let parsed = ParsedSource::parse(&PathBuf::from("service.py"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("service.py"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 3,
                old_lines: 0,
                new_start: 4,
                new_lines: 2,
                lines: vec![
                    DiffLine {
                        kind: LineKind::Added,
                        old_lineno: None,
                        new_lineno: Some(4),
                        content: "    except Exception:".to_string(),
                    },
                    DiffLine {
                        kind: LineKind::Added,
                        old_lineno: None,
                        new_lineno: Some(5),
                        content: "        pass".to_string(),
                    },
                ],
            }],
        };

        let rule = Tb101SilentCatch;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB101");
    }

    #[test]
    fn test_tb101_detects_go_empty_err_if() {
        let code = "package main\n\nfunc query() {\n    if err != nil {\n        // silently ignored\n    }\n}\n".to_string();
        let parsed = ParsedSource::parse(&PathBuf::from("main.go"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("main.go"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 3,
                old_lines: 0,
                new_start: 4,
                new_lines: 3,
                lines: vec![
                    DiffLine {
                        kind: LineKind::Added,
                        old_lineno: None,
                        new_lineno: Some(4),
                        content: "    if err != nil {".to_string(),
                    },
                    DiffLine {
                        kind: LineKind::Added,
                        old_lineno: None,
                        new_lineno: Some(5),
                        content: "        // silently ignored".to_string(),
                    },
                    DiffLine {
                        kind: LineKind::Added,
                        old_lineno: None,
                        new_lineno: Some(6),
                        content: "    }".to_string(),
                    },
                ],
            }],
        };

        let rule = Tb101SilentCatch;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB101");
    }
}
