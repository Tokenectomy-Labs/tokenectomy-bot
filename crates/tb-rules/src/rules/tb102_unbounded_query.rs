use crate::model::{Confidence, Finding, RuleContext, Severity};
use crate::traits::Rule;
use tb_diff::FileKind;
use tb_parse::ParsedSource;

pub struct Tb102UnboundedQuery;

impl Rule for Tb102UnboundedQuery {
    fn id(&self) -> &'static str {
        "TB102"
    }

    fn name(&self) -> &'static str {
        "unbounded-query"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warn
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

        // 1. Detect if ORM client is imported
        let imports = new_parsed.find_all_descendants(new_parsed.root_node(), &|node| {
            node.kind() == "import_statement" || node.kind() == "import_declaration"
        });

        let has_orm_import = imports.iter().any(|imp| {
            let text = new_parsed.node_text(imp);
            text.contains("@prisma/client")
                || text.contains("drizzle-orm")
                || text.contains("typeorm")
                || text.contains("mongoose")
                || text.contains("sequelize")
                || text.contains("knex")
        });

        // 2. Scan call expressions
        let calls = new_parsed.find_all_descendants(new_parsed.root_node(), &|node| {
            node.kind() == "call_expression"
        });

        for node in calls {
            if !ParsedSource::node_overlaps_ranges(&node, &changed_ranges) {
                continue;
            }

            let text = new_parsed.node_text(&node).trim();

            let is_unbounded = if text.contains(".findMany(") {
                !text.contains("take:") && !text.contains("limit:")
            } else if text.contains(".find(") && has_orm_import {
                !text.contains(".limit(") && !text.contains("limit:") && !text.contains("take:")
            } else {
                false
            };

            if is_unbounded {
                let (start_line, end_line) = ParsedSource::node_line_range(&node);
                let (start_col, _) = ParsedSource::point_to_1indexed(node.start_position());
                let (_, end_col) = ParsedSource::point_to_1indexed(node.end_position());

                let confidence = if has_orm_import {
                    Confidence::High
                } else {
                    Confidence::Medium
                };

                findings.push(Finding::new(
                    self.id(),
                    self.name(),
                    self.default_severity(),
                    confidence,
                    &ctx.file_diff.path,
                    start_line,
                    end_line,
                    start_col,
                    end_col,
                    "Database query without pagination (`limit` / `take`) can cause OOM or full-table scans.",
                    Some("Add `take: <number>` or `.limit(<number>)` to restrict maximum query rows.".to_string()),
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
    fn test_tb102_detects_unbounded_find_many() {
        let code = r#"
            import { prisma } from "@prisma/client";

            export async function getUsers() {
                return await prisma.user.findMany({
                    where: { active: true }
                });
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
                old_start: 5,
                old_lines: 0,
                new_start: 5,
                new_lines: 3,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(5),
                    content: "                return await prisma.user.findMany({".to_string(),
                }],
            }],
        };

        let rule = Tb102UnboundedQuery;
        let findings = rule.check(&RuleContext {
            file_diff: &file_diff,
            old_parsed: None,
            new_parsed: Some(&parsed),
        });

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB102");
        assert_eq!(findings[0].confidence, Confidence::High);
    }
}
