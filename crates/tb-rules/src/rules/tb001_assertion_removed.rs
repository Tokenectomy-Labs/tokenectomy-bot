use crate::model::{Confidence, Finding, RuleContext, Severity};
use crate::traits::Rule;
use tb_diff::{DiffStatus, FileKind};
use tb_parse::ParsedSource;

pub struct Tb001AssertionRemoved;

impl Rule for Tb001AssertionRemoved {
    fn id(&self) -> &'static str {
        "TB001"
    }

    fn name(&self) -> &'static str {
        "assertion-removed"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn needs_old_side(&self) -> bool {
        true
    }

    fn check(&self, ctx: &RuleContext) -> Vec<Finding> {
        let mut findings = Vec::new();

        if ctx.file_diff.kind != FileKind::Test {
            return findings;
        }

        // Only check modified files, not completely deleted test files (which is handled by TB006)
        if matches!(ctx.file_diff.status, DiffStatus::Deleted) {
            return findings;
        }

        let old_parsed = match ctx.old_parsed {
            Some(p) => p,
            None => return findings,
        };

        let new_parsed = match ctx.new_parsed {
            Some(p) => p,
            None => return findings,
        };

        let count_assertions = |parsed: &ParsedSource| -> usize {
            let calls = parsed.find_all_descendants(parsed.root_node(), &|n| {
                n.kind() == "call_expression" || n.kind() == "macro_invocation"
            });
            calls
                .into_iter()
                .filter(|node| {
                    let text = parsed.node_text(node);
                    text.starts_with("expect(")
                        || text.starts_with("assert(")
                        || text.starts_with("assert.")
                        || text.starts_with("assert_eq!")
                        || text.starts_with("assert_ne!")
                })
                .count()
        };

        let old_count = count_assertions(old_parsed);
        let new_count = count_assertions(new_parsed);

        if old_count > new_count {
            let dropped = old_count - new_count;
            // Report on the first modified line in new side
            let (start_line, end_line) = ctx
                .file_diff
                .changed_line_ranges_new()
                .into_iter()
                .next()
                .unwrap_or((1, 1));

            findings.push(Finding::new(
                self.id(),
                self.name(),
                self.default_severity(),
                Confidence::High,
                &ctx.file_diff.path,
                start_line,
                end_line,
                1,
                1,
                format!(
                    "Assertion count decreased from {} to {} (-{} assertions) in test suite.",
                    old_count, new_count, dropped
                ),
                Some("Avoid deleting assertions to pass CI. Ensure test coverage remains intact or update the test properly.".to_string()),
                "assertion_group",
                &format!("old: {}, new: {}", old_count, new_count),
            ));
        }

        findings
    }
}
