use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tb_diff::DiffResult;
use tb_parse::ParsedSource;

use crate::model::{Finding, RuleContext};
use crate::rules::{
    Tb001AssertionRemoved, Tb002TestDisabled, Tb003TautologicalAssertion, Tb101SilentCatch,
};
use crate::traits::Rule;

pub struct RuleEngine {
    rules: Vec<Box<dyn Rule>>,
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RuleEngine {
    pub fn new() -> Self {
        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(Tb001AssertionRemoved),
            Box::new(Tb002TestDisabled),
            Box::new(Tb003TautologicalAssertion),
            Box::new(Tb101SilentCatch),
        ];

        Self { rules }
    }

    pub fn with_rules(rules: Vec<Box<dyn Rule>>) -> Self {
        Self { rules }
    }

    pub fn rules(&self) -> &[Box<dyn Rule>] {
        &self.rules
    }

    pub fn run(
        &self,
        diff: &DiffResult,
        old_sources: &HashMap<PathBuf, String>,
        new_sources: &HashMap<PathBuf, String>,
    ) -> Vec<Finding> {
        let mut all_findings = Vec::new();
        let mut seen_fingerprints = HashSet::new();

        for file_diff in diff.scannable_files() {
            let old_parsed = old_sources
                .get(&file_diff.path)
                .and_then(|content| ParsedSource::parse(&file_diff.path, content.clone()).ok());

            let new_parsed = new_sources
                .get(&file_diff.path)
                .and_then(|content| ParsedSource::parse(&file_diff.path, content.clone()).ok());

            let new_content = new_sources.get(&file_diff.path);

            let ctx = RuleContext {
                file_diff,
                old_parsed: old_parsed.as_ref(),
                new_parsed: new_parsed.as_ref(),
            };

            for rule in &self.rules {
                if rule.needs_old_side() && old_parsed.is_none() {
                    // Skip if rule requires old side but not available
                    continue;
                }

                let findings = rule.check(&ctx);
                for finding in findings {
                    // Check inline suppression // tokenectomy-ignore: TBxxx
                    if new_content
                        .is_some_and(|src| is_suppressed(src, finding.start_line, &finding.rule_id))
                    {
                        continue;
                    }

                    if seen_fingerprints.insert(finding.fingerprint.clone()) {
                        all_findings.push(finding);
                    }
                }
            }
        }

        all_findings
    }
}

fn is_suppressed(source: &str, target_line: usize, rule_id: &str) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    if target_line == 0 || target_line > lines.len() {
        return false;
    }

    let check_line = |l: &str| -> bool {
        if let Some(idx) = l.find("tokenectomy-ignore:") {
            let after = &l[idx + "tokenectomy-ignore:".len()..];
            after.split_whitespace().any(|token| {
                let clean = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
                clean == rule_id || clean == "all"
            })
        } else {
            false
        }
    };

    // 1. Same line
    if check_line(lines[target_line - 1]) {
        return true;
    }

    // 2. Preceding line (target_line - 2 if 1-indexed)
    if target_line >= 2 && check_line(lines[target_line - 2]) {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suppression() {
        let code = r#"
            // tokenectomy-ignore: TB101 -- intentional empty catch
            try { doSomething(); } catch (e) {}
        "#;
        assert!(is_suppressed(code, 3, "TB101"));
        assert!(!is_suppressed(code, 3, "TB002"));
    }
}
