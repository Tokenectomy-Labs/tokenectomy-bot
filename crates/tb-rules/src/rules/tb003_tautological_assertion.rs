use crate::model::{Confidence, Finding, RuleContext, Severity};
use crate::traits::Rule;
use tb_diff::FileKind;
use tb_parse::ParsedSource;

pub struct Tb003TautologicalAssertion;

impl Rule for Tb003TautologicalAssertion {
    fn id(&self) -> &'static str {
        "TB003"
    }

    fn name(&self) -> &'static str {
        "tautological-assertion"
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

        let nodes = new_parsed.find_all_descendants(new_parsed.root_node(), &|node| {
            matches!(
                node.kind(),
                "call_expression" | "call" | "assert_statement" | "macro_invocation"
            )
        });

        for node in nodes {
            if !ParsedSource::node_overlaps_ranges(&node, &changed_ranges) {
                continue;
            }

            let text = new_parsed.node_text(&node).trim();

            let is_tautology = match node.kind() {
                "assert_statement" => {
                    // Python assert statement
                    text == "assert True"
                        || text.starts_with("assert True,")
                        || text == "assert 1 == 1"
                        || text.starts_with("assert 1 == 1,")
                        || if let Some(stripped) = text.strip_prefix("assert ") {
                            let expr = stripped.split(',').next().unwrap_or("").trim();
                            if let Some((left, right)) = expr.split_once("==") {
                                let l = left.trim();
                                let r = right.trim();
                                !l.is_empty() && l == r
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                }
                "macro_invocation" => {
                    // Rust assert! or assert_eq!
                    text.starts_with("assert!(true)")
                        || text.starts_with("assert!(1 == 1)")
                        || if text.starts_with("assert_eq!(") {
                            if let Some(inner) = text
                                .strip_prefix("assert_eq!(")
                                .and_then(|s| s.strip_suffix(')'))
                            {
                                let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
                                parts.len() >= 2 && !parts[0].is_empty() && parts[0] == parts[1]
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                }
                "call" | "call_expression" => {
                    let standard_tautology = text.starts_with("expect(true).toBe(true)")
                        || text.starts_with("expect(false).toBe(false)")
                        || text.starts_with("expect(true).toEqual(true)")
                        || text.starts_with("expect(false).toEqual(false)")
                        || text == "assert(true)"
                        || text.starts_with("assert(1 === 1)")
                        || text.starts_with("assert.strictEqual(true, true)")
                        || text.starts_with("self.assertTrue(True)")
                        || text.starts_with("self.assertFalse(False)")
                        || text.starts_with("assert.True(t, true)")
                        || text.starts_with("assert.False(t, false)");

                    let self_comp = if text.starts_with("expect(") {
                        if let Some(rest) = text.strip_prefix("expect(") {
                            if let Some(close_paren) = rest.find(')') {
                                let arg1 = rest[..close_paren].trim();
                                if let Some(matcher_start) = rest.find(".toBe(") {
                                    let m_rest = &rest[matcher_start + 6..];
                                    if let Some(m_end) = m_rest.find(')') {
                                        let arg2 = m_rest[..m_end].trim();
                                        !arg1.is_empty() && arg1 == arg2
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else if text.starts_with("self.assertEqual(") {
                        if let Some(inner) = text
                            .strip_prefix("self.assertEqual(")
                            .and_then(|s| s.strip_suffix(')'))
                        {
                            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
                            parts.len() >= 2 && !parts[0].is_empty() && parts[0] == parts[1]
                        } else {
                            false
                        }
                    } else if text.starts_with("assert.Equal(") {
                        if let Some(inner) = text
                            .strip_prefix("assert.Equal(")
                            .and_then(|s| s.strip_suffix(')'))
                        {
                            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
                            // In Go testify: assert.Equal(t, expected, actual)
                            parts.len() >= 3 && !parts[1].is_empty() && parts[1] == parts[2]
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    standard_tautology || self_comp
                }
                _ => false,
            };

            if is_tautology {
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
                    format!("Tautological assertion detected: `{}` always evaluates to true.", text.lines().next().unwrap_or(text)),
                    Some("Assert the actual variable against expected system state, not a constant against itself.".to_string()),
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
    fn test_tb003_detects_python_assert_true() {
        let code = "def test_something():\n    assert True\n".to_string();
        let parsed = ParsedSource::parse(&PathBuf::from("test_something.py"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("test_something.py"),
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
                    content: "    assert True".to_string(),
                }],
            }],
        };

        let rule = Tb003TautologicalAssertion;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB003");
    }

    #[test]
    fn test_tb003_detects_go_assert_equal_same() {
        let code = "package main\n\nfunc TestSame(t *testing.T) {\n    assert.Equal(t, actual, actual)\n}\n".to_string();
        let parsed = ParsedSource::parse(&PathBuf::from("main_test.go"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("main_test.go"),
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
                    content: "    assert.Equal(t, actual, actual)".to_string(),
                }],
            }],
        };

        let rule = Tb003TautologicalAssertion;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB003");
    }

    #[test]
    fn test_tb003_detects_rust_assert_true() {
        let code = "#[test]\nfn test_rust() {\n    assert!(true);\n}\n".to_string();
        let parsed = ParsedSource::parse(&PathBuf::from("tests/logic_test.rs"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("tests/logic_test.rs"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Test,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 2,
                old_lines: 0,
                new_start: 3,
                new_lines: 1,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(3),
                    content: "    assert!(true);".to_string(),
                }],
            }],
        };

        let rule = Tb003TautologicalAssertion;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB003");
    }
}
