use crate::model::{Confidence, Finding, RuleContext, Severity};
use crate::traits::Rule;
use tb_diff::FileKind;
use tb_parse::ParsedSource;

pub struct Tb201DynamicEval;

impl Rule for Tb201DynamicEval {
    fn id(&self) -> &'static str {
        "TB201"
    }

    fn name(&self) -> &'static str {
        "dynamic-eval"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, ctx: &RuleContext) -> Vec<Finding> {
        let mut findings = Vec::new();

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

        let calls = new_parsed.find_all_descendants(new_parsed.root_node(), &|node| {
            node.kind() == "call_expression" || node.kind() == "new_expression"
        });

        for node in calls {
            if !ParsedSource::node_overlaps_ranges(&node, &changed_ranges) {
                continue;
            }

            let text = new_parsed.node_text(&node).trim();

            let is_eval = text.starts_with("eval(")
                || text.starts_with("new Function(")
                || text.contains("vm.runInThisContext(")
                || text.contains("vm.runInNewContext(")
                || text.contains("vm.runInContext(");

            if is_eval {
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
                    "Dynamic code evaluation (`eval` / `new Function`) detected. This introduces critical RCE vulnerabilities.",
                    Some("Refactor into static typed expressions or safe data-driven lookups without eval.".to_string()),
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
    fn test_tb201_detects_eval() {
        let code = r#"
            function runUserScript(script: string) {
                return eval(script);
            }
        "#
        .to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("script.ts"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("script.ts"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 2,
                old_lines: 0,
                new_start: 2,
                new_lines: 2,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(2),
                    content: "                return eval(script);".to_string(),
                }],
            }],
        };

        let rule = Tb201DynamicEval;
        let findings = rule.check(&RuleContext {
            file_diff: &file_diff,
            old_parsed: None,
            new_parsed: Some(&parsed),
        });

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB201");
    }
}
