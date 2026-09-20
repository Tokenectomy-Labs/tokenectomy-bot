use crate::model::{Confidence, Finding, RuleContext, Severity};
use crate::traits::Rule;
use tb_diff::FileKind;
use tb_parse::ParsedSource;

pub struct Tb203RawSqlInterpolation;

impl Rule for Tb203RawSqlInterpolation {
    fn id(&self) -> &'static str {
        "TB203"
    }

    fn name(&self) -> &'static str {
        "raw-sql-interpolation"
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

            let is_sql_call = text.contains("$queryRawUnsafe(")
                || text.contains(".query(`")
                || text.contains(".execute(`");

            if !is_sql_call {
                continue;
            }

            if text.contains("${") {
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
                    "Raw SQL query with string interpolation `${...}` creates direct SQL injection vulnerability.",
                    Some("Use parameterized queries (`$queryRaw`, `$1`, `?`) instead of string interpolation.".to_string()),
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
    fn test_tb203_detects_sql_interpolation() {
        let code = r#"
            export async function search(name: string) {
                return await prisma.$queryRawUnsafe(`SELECT * FROM users WHERE name = '${name}'`);
            }
        "#
        .to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("repo.ts"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("repo.ts"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 3,
                old_lines: 0,
                new_start: 3,
                new_lines: 1,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(3),
                    content: "                return await prisma.$queryRawUnsafe(`SELECT * FROM users WHERE name = '${name}'`);".to_string(),
                }],
            }],
        };

        let rule = Tb203RawSqlInterpolation;
        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB203");
    }
}
