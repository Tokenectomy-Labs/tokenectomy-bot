use crate::model::{Confidence, Finding, RuleContext, Severity};
use crate::traits::Rule;
use tb_parse::ParsedSource;

pub struct Tb008DomainNarrowing;

impl Rule for Tb008DomainNarrowing {
    fn id(&self) -> &'static str {
        "TB008"
    }

    fn name(&self) -> &'static str {
        "domain-narrowing"
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

        // Find if statements / expressions across languages
        let if_nodes = new_parsed.find_all_descendants(new_parsed.root_node(), &|node| {
            matches!(node.kind(), "if_statement" | "if_expression")
        });

        for if_node in if_nodes {
            if !ParsedSource::node_overlaps_ranges(&if_node, &changed_ranges) {
                continue;
            }

            let text = new_parsed.node_text(&if_node).trim();

            // Check if this if statement is a narrow literal guard
            // e.g.: if (x === "test-case") return mock;
            // or: if input == 100 { return 15; }
            if is_domain_narrowing_guard(&if_node, new_parsed) {
                let (start_line, end_line) = ParsedSource::node_line_range(&if_node);
                let (start_col, _) = ParsedSource::point_to_1indexed(if_node.start_position());
                let (_, end_col) = ParsedSource::point_to_1indexed(if_node.end_position());

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
                        "Detected artificial domain-narrowing guard (`{}`). Hardcoding specific test inputs bypasses real domain logic.",
                        first_line
                    ),
                    Some("Replace artificial equality guard with general business logic that handles arbitrary domain inputs.".to_string()),
                    if_node.kind(),
                    text,
                ));
            }
        }

        findings
    }
}

fn is_domain_narrowing_guard(if_node: &tree_sitter::Node, parsed: &ParsedSource) -> bool {
    let text = parsed.node_text(if_node).trim();

    // Fast check: must contain equality check against literal
    let has_equality = text.contains("===") || text.contains("==");
    if !has_equality {
        return false;
    }

    // Must return or yield a value
    let has_return = text.contains("return ") || text.contains("return;");
    if !has_return && !matches!(parsed.language, tb_parse::SupportedLanguage::Rust) {
        return false;
    }

    // Look for binary expression inside the condition
    let binary_exprs = parsed.find_all_descendants(*if_node, &|n| {
        matches!(n.kind(), "binary_expression" | "comparison_operator")
    });

    for expr in binary_exprs {
        let op_text = parsed.node_text(&expr);
        if !op_text.contains("==") && !op_text.contains("===") {
            continue;
        }

        // Check children of binary expression
        let left = expr.child(0);
        let right = expr.child(2).or_else(|| expr.child(1));

        if let (Some(l), Some(r)) = (left, right) {
            let is_l_literal = is_literal_kind(l.kind());
            let is_r_literal = is_literal_kind(r.kind());

            // One side is a literal and the other is an identifier / access
            if is_l_literal ^ is_r_literal {
                // Now check if the body of the if block is trivial (less than 4 lines / single return)
                let lines_count = text.lines().count();
                if lines_count <= 4 && (text.contains("return ") || text.contains("=>")) {
                    return true;
                }
            }
        }
    }

    false
}

fn is_literal_kind(kind: &str) -> bool {
    matches!(
        kind,
        "string"
            | "string_literal"
            | "number"
            | "integer"
            | "float"
            | "true"
            | "false"
            | "boolean"
            | "null"
            | "int_literal"
            | "string_content"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tb_diff::{DiffLine, DiffStatus, FileDiff, FileKind, Hunk, LineKind};

    #[test]
    fn test_tb008_detects_test_special_casing_ts() {
        let code = r#"
            function calculateTax(amount: number) {
                if (amount === 100) return 15;
                return amount * 0.1;
            }
        "#
        .to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("tax.ts"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("tax.ts"),
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
                    content: "                if (amount === 100) return 15;".to_string(),
                }],
            }],
        };

        let rule = Tb008DomainNarrowing;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB008");
        assert!(findings[0].message.contains("domain-narrowing"));
    }

    #[test]
    fn test_tb008_detects_string_literal_guard() {
        let code = r#"
            function getUser(id: string) {
                if (id === "user_mock_test_123") {
                    return mockUserData;
                }
                return db.find(id);
            }
        "#
        .to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("user.ts"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("user.ts"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 2,
                old_lines: 3,
                new_start: 2,
                new_lines: 5,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(3),
                    content: "                if (id === \"user_mock_test_123\") {".to_string(),
                }],
            }],
        };

        let rule = Tb008DomainNarrowing;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn test_tb008_ignores_normal_null_checks() {
        let code = r#"
            function process(item: any) {
                if (!item || item.length === 0) {
                    return;
                }
                item.run();
            }
        "#
        .to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("item.ts"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("item.ts"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 2,
                old_lines: 4,
                new_start: 2,
                new_lines: 4,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(2),
                    content: "                if (!item || item.length === 0) {".to_string(),
                }],
            }],
        };

        let rule = Tb008DomainNarrowing;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        // Normal empty check should not be flagged as domain narrowing
        assert!(findings.is_empty());
    }
}
