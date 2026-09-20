use crate::model::{Confidence, Finding, RuleContext, Severity};
use crate::traits::Rule;
use tb_diff::FileKind;
use tb_parse::ParsedSource;

pub struct Tb202ShellInjection;

impl Rule for Tb202ShellInjection {
    fn id(&self) -> &'static str {
        "TB202"
    }

    fn name(&self) -> &'static str {
        "shell-injection"
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
            node.kind() == "call_expression"
        });

        for node in calls {
            if !ParsedSource::node_overlaps_ranges(&node, &changed_ranges) {
                continue;
            }

            let text = new_parsed.node_text(&node).trim();

            let is_shell_exec = text.starts_with("exec(")
                || text.starts_with("execSync(")
                || text.contains("child_process.exec(")
                || text.contains("child_process.execSync(");

            if !is_shell_exec {
                continue;
            }

            // Check if argument contains template literal interpolation `${...}` or non-literal concatenation
            let has_interpolation = text.contains("`${")
                || (text.contains("`") && text.contains("${"))
                || (text.contains("exec(") && text.contains(" + "))
                || (text.contains("execSync(") && text.contains(" + "));

            if has_interpolation {
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
                    "Dynamic shell command execution with interpolated input introduces shell command injection vulnerability.",
                    Some("Use `execFile` or `spawn` with an array of arguments, never string interpolation in a shell.".to_string()),
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
    fn test_tb202_detects_interpolated_exec() {
        let code = r#"
            import { execSync } from "child_process";

            function backup(filename: string) {
                execSync(`tar -czf backup.tar.gz ${filename}`);
            }
        "#
        .to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("backup.ts"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("backup.ts"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 5,
                old_lines: 0,
                new_start: 5,
                new_lines: 1,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(5),
                    content: "                execSync(`tar -czf backup.tar.gz ${filename}`);"
                        .to_string(),
                }],
            }],
        };

        let rule = Tb202ShellInjection;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB202");
    }
}
