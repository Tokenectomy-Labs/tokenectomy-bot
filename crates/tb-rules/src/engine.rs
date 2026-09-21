use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tb_diff::DiffResult;
use tb_parse::ParsedSource;

use crate::config::{Config, Preset};
use crate::model::{Finding, RuleContext, Severity};
use crate::rules::{
    Tb001AssertionRemoved, Tb002TestDisabled, Tb003TautologicalAssertion, Tb004ConfigWeakened,
    Tb005EarlyExitInjected, Tb006TestDeleted, Tb007LazyDeletion, Tb008DomainNarrowing,
    Tb009FixtureSnooping, Tb101SilentCatch, Tb102UnboundedQuery, Tb104AsyncForeach,
    Tb201DynamicEval, Tb202ShellInjection, Tb203RawSqlInterpolation,
};
use crate::traits::Rule;

pub struct RuleEngine {
    rules: Vec<Box<dyn Rule>>,
    baseline: HashSet<String>,
    agent_mode: bool,
    config: Config,
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
            Box::new(Tb004ConfigWeakened),
            Box::new(Tb005EarlyExitInjected),
            Box::new(Tb006TestDeleted),
            Box::new(Tb007LazyDeletion),
            Box::new(Tb008DomainNarrowing),
            Box::new(Tb009FixtureSnooping),
            Box::new(Tb101SilentCatch),
            Box::new(Tb102UnboundedQuery),
            Box::new(Tb104AsyncForeach),
            Box::new(Tb201DynamicEval),
            Box::new(Tb202ShellInjection),
            Box::new(Tb203RawSqlInterpolation),
        ];

        Self {
            rules,
            baseline: HashSet::new(),
            agent_mode: false,
            config: Config::default(),
        }
    }

    pub fn with_baseline(mut self, baseline: HashSet<String>) -> Self {
        self.baseline = baseline;
        self
    }

    pub fn with_agent_mode(mut self, agent_mode: bool) -> Self {
        self.agent_mode = agent_mode;
        self
    }

    pub fn with_config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn rules(&self) -> &[Box<dyn Rule>] {
        &self.rules
    }

    pub fn add_rule(&mut self, rule: Box<dyn Rule>) {
        self.rules.push(rule);
    }

    pub fn with_custom_rules(mut self, rules: Vec<crate::custom::CustomRule>) -> Self {
        for rule in rules {
            self.rules.push(Box::new(rule));
        }
        self
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
            // Check if file is ignored by tokenectomy.json ignore patterns
            if self.config.is_path_ignored(&file_diff.path) {
                continue;
            }

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
                logger_names: self.config.logger_names.as_deref(),
                orm_modules: self.config.orm_modules.as_deref(),
            };

            for rule in &self.rules {
                let (enabled, configured_severity) = self.config.effective_rule_setting(
                    rule.id(),
                    Some(&file_diff.path),
                    rule.default_severity(),
                );

                if !enabled {
                    continue;
                }

                if rule.needs_old_side() && old_parsed.is_none() {
                    continue;
                }

                let findings = rule.check(&ctx);
                for mut finding in findings {
                    // Apply configured severity override
                    finding.severity = configured_severity;

                    // Check inline suppression // tokenectomy-ignore: TBxxx
                    if new_content
                        .is_some_and(|src| is_suppressed(src, finding.start_line, &finding.rule_id))
                    {
                        continue;
                    }

                    // Check baseline (.tokenectomy-baseline.json)
                    if self.baseline.contains(&finding.fingerprint) {
                        continue;
                    }

                    // Agent PR mode upgrades TB001-TB005 to Error
                    if (self.agent_mode || self.config.extends == Some(Preset::AgentPr))
                        && finding.rule_id.starts_with("TB00")
                    {
                        finding.severity = Severity::Error;
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

    #[test]
    fn test_baseline_filtering() {
        let mut baseline = HashSet::new();
        baseline.insert("deadbeef1234".to_string());
        let engine = RuleEngine::new().with_baseline(baseline);
        assert!(engine.baseline.contains("deadbeef1234"));
    }
}
