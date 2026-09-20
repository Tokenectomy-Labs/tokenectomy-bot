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

        let calls = new_parsed.find_all_descendants(new_parsed.root_node(), &|node| {
            node.kind() == "call_expression"
        });

        for node in calls {
            if !ParsedSource::node_overlaps_ranges(&node, &changed_ranges) {
                continue;
            }

            let text = new_parsed.node_text(&node).trim();

            if !text.starts_with("expect(") && !text.starts_with("assert(") && !text.starts_with("assert.") {
                continue;
            }

            let is_tautology = text.starts_with("expect(true).toBe(true)")
                || text.starts_with("expect(false).toBe(false)")
                || text.starts_with("expect(true).toEqual(true)")
                || text.starts_with("expect(false).toEqual(false)")
                || text == "assert(true)"
                || text.starts_with("assert(1 === 1)")
                || text.starts_with("assert.strictEqual(true, true)");

            // Also check expect(X).toBe(X)
            let is_self_comparison = if !is_tautology && text.starts_with("expect(") {
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
            } else {
                false
            };

            if is_tautology || is_self_comparison {
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
