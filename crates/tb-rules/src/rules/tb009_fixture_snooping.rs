use crate::model::{Confidence, Finding, RuleContext, Severity};
use crate::traits::Rule;
use tb_diff::FileKind;
use tb_parse::ParsedSource;

pub struct Tb009FixtureSnooping;

impl Rule for Tb009FixtureSnooping {
    fn id(&self) -> &'static str {
        "TB009"
    }

    fn name(&self) -> &'static str {
        "fixture-snooping"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, ctx: &RuleContext) -> Vec<Finding> {
        let mut findings = Vec::new();

        // Only scan production source files, NOT test or config files
        if ctx.file_diff.kind != FileKind::Source {
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

        // Look for string literals and binary expressions
        let string_nodes = new_parsed.find_all_descendants(new_parsed.root_node(), &|node| {
            node.kind() == "string"
                || node.kind() == "string_fragment"
                || node.kind() == "template_string"
                || node.kind() == "binary_expression"
        });

        for node in string_nodes {
            if !ParsedSource::node_overlaps_ranges(&node, &changed_ranges) {
                continue;
            }

            let text = new_parsed.node_text(&node);

            let mut issue = None;
            if text.contains("NODE_ENV === 'test'")
                || text.contains("NODE_ENV === \"test\"")
                || text.contains("NODE_ENV == 'test'")
                || text.contains("NODE_ENV == \"test\"")
            {
                issue = Some((
                    "Production code branches on `NODE_ENV === 'test'`. This creates test-only behavior divergence.",
                    "Do not alter business logic specifically for test environments. Mock dependencies externally.",
                ));
            } else if text.contains("/fixtures/")
                || text.contains("../fixtures")
                || text.contains("./fixtures")
                || text.contains("/__tests__/")
                || text.contains("../__tests__")
            {
                issue = Some((
                    "Production code accesses `fixtures/` or `__tests__/` path directly.",
                    "Production logic must not depend on test fixtures or test directories.",
                ));
            }

            if let Some((msg, hint)) = issue {
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
                    msg,
                    Some(hint.to_string()),
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
    fn test_tb009_detects_node_env_branch_in_source() {
        let code = r#"
            export function authenticateUser(user: User) {
                if (process.env.NODE_ENV === 'test') {
                    return true;
                }
                return verifyWithLdap(user);
            }
        "#
        .to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("auth.ts"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("auth.ts"),
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
                    content: "                if (process.env.NODE_ENV === 'test') {".to_string(),
                }],
            }],
        };

        let rule = Tb009FixtureSnooping;
        let findings = rule.check(&RuleContext {
            file_diff: &file_diff,
            old_parsed: None,
            new_parsed: Some(&parsed),
        });

        assert!(!findings.is_empty());
        assert_eq!(findings[0].rule_id, "TB009");
    }
}
