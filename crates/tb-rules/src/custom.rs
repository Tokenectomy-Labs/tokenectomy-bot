use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use tb_parse::{ParsedSource, SupportedLanguage};
use tree_sitter::{Query, QueryCursor, StreamingIterator};

use crate::model::{Confidence, Finding, RuleContext, Severity};
use crate::traits::Rule;

#[derive(Debug, Clone)]
pub struct CustomRule {
    pub id: String,
    pub name: String,
    pub severity: Severity,
    pub message: String,
    pub fix_hint: Option<String>,
    pub languages: Vec<SupportedLanguage>,
    pub query_str: String,
}

impl CustomRule {
    pub fn parse_from_scm(content: &str) -> Result<Self> {
        let mut id = None;
        let mut name = None;
        let mut severity = Severity::Error;
        let mut message = None;
        let mut fix_hint = None;
        let mut languages = Vec::new();
        let mut query_lines = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with(";;") {
                let directive = trimmed.trim_start_matches(';').trim();
                if let Some(val) = directive.strip_prefix("@id:") {
                    id = Some(val.trim().to_string());
                } else if let Some(val) = directive.strip_prefix("@name:") {
                    name = Some(val.trim().to_string());
                } else if let Some(val) = directive.strip_prefix("@severity:") {
                    match val.trim().to_lowercase().as_str() {
                        "warn" | "warning" => severity = Severity::Warn,
                        "info" => severity = Severity::Info,
                        _ => severity = Severity::Error,
                    }
                } else if let Some(val) = directive.strip_prefix("@message:") {
                    message = Some(val.trim().to_string());
                } else if let Some(val) = directive.strip_prefix("@fix_hint:") {
                    fix_hint = Some(val.trim().to_string());
                } else if let Some(val) = directive.strip_prefix("@languages:") {
                    for lang_str in val.split(',') {
                        if let Some(lang) = SupportedLanguage::from_name(lang_str.trim()) {
                            languages.push(lang);
                        }
                    }
                }
            } else {
                query_lines.push(line);
            }
        }

        let id =
            id.ok_or_else(|| anyhow::anyhow!("Custom rule missing required ';; @id:' header"))?;
        let name = name.unwrap_or_else(|| id.clone());
        let message = message.unwrap_or_else(|| format!("Violation of custom rule {}", id));
        let query_str = query_lines.join("\n").trim().to_string();

        if query_str.is_empty() {
            bail!(
                "Custom rule '{}' contains an empty Tree-sitter query pattern",
                id
            );
        }

        let rule = Self {
            id,
            name,
            severity,
            message,
            fix_hint,
            languages,
            query_str,
        };

        rule.validate()?;
        Ok(rule)
    }

    pub fn validate(&self) -> Result<()> {
        let test_langs = if self.languages.is_empty() {
            vec![
                SupportedLanguage::TypeScript,
                SupportedLanguage::Python,
                SupportedLanguage::Rust,
            ]
        } else {
            self.languages.clone()
        };

        let mut compiled_any = false;
        let mut last_err = None;

        for lang in test_langs {
            let ts_lang = lang.tree_sitter_language();
            match Query::new(&ts_lang, &self.query_str) {
                Ok(_) => compiled_any = true,
                Err(e) => last_err = Some(format!("{:?}", e)),
            }
        }

        if !compiled_any {
            bail!(
                "Failed to compile Tree-sitter query for custom rule '{}': {}",
                self.id,
                last_err.unwrap_or_default()
            );
        }

        Ok(())
    }

    pub fn load_dir(dir: &Path) -> Result<Vec<Self>> {
        let mut rules = Vec::new();
        if !dir.exists() {
            return Ok(rules);
        }

        let entries = fs::read_dir(dir)
            .with_context(|| format!("Failed to read custom rules directory at {:?}", dir))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("scm") {
                let content = fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read custom rule file at {:?}", path))?;
                let rule = Self::parse_from_scm(&content)
                    .with_context(|| format!("Failed to parse custom rule at {:?}", path))?;
                rules.push(rule);
            }
        }

        Ok(rules)
    }

    /// Test this custom rule against a code fixture to ensure it correctly detects patterns
    pub fn test_fixture(&self, code: &str, lang: SupportedLanguage) -> Result<usize> {
        let fake_path = std::path::PathBuf::from(format!("test.{}", lang.extension()));
        let parsed = ParsedSource::parse(&fake_path, code.to_string())
            .with_context(|| format!("Failed to parse fixture code as {:?}", lang))?;

        let mut diff_lines = Vec::new();
        for (idx, line) in code.lines().enumerate() {
            diff_lines.push(tb_diff::DiffLine {
                kind: tb_diff::LineKind::Added,
                content: line.to_string(),
                old_lineno: None,
                new_lineno: Some(idx + 1),
            });
        }

        let total_lines = diff_lines.len();
        let file_diff = tb_diff::FileDiff {
            path: fake_path,
            old_path: None,
            status: tb_diff::DiffStatus::Modified,
            kind: tb_diff::FileKind::Source,
            is_binary: false,
            hunks: vec![tb_diff::Hunk {
                old_start: 0,
                old_lines: 0,
                new_start: 1,
                new_lines: total_lines,
                lines: diff_lines,
            }],
        };

        let ctx = RuleContext {
            file_diff: &file_diff,
            old_parsed: None,
            new_parsed: Some(&parsed),
            logger_names: None,
            orm_modules: None,
            all_sources: None,
            repo_dir: None,
        };

        let findings = self.check(&ctx);
        Ok(findings.len())
    }
}

impl Rule for CustomRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn default_severity(&self) -> Severity {
        self.severity
    }

    fn check(&self, ctx: &RuleContext) -> Vec<Finding> {
        let mut findings = Vec::new();

        let new_parsed = match ctx.new_parsed {
            Some(p) => p,
            None => return findings,
        };

        if !self.languages.is_empty() && !self.languages.contains(&new_parsed.language) {
            return findings;
        }

        let changed_ranges = ctx.file_diff.changed_line_ranges_new();
        if changed_ranges.is_empty() {
            return findings;
        }

        let ts_lang = new_parsed.language.tree_sitter_language();
        let query = match Query::new(&ts_lang, &self.query_str) {
            Ok(q) => q,
            Err(_) => return findings,
        };

        let mut cursor = QueryCursor::new();
        let mut matches =
            cursor.matches(&query, new_parsed.root_node(), new_parsed.source.as_bytes());

        while let Some(m) = matches.next() {
            for capture in m.captures() {
                let node = capture.node;
                if ParsedSource::node_overlaps_ranges(&node, &changed_ranges) {
                    let (start_line, end_line) = ParsedSource::node_line_range(&node);
                    let (start_col, _) = ParsedSource::point_to_1indexed(node.start_position());
                    let (_, end_col) = ParsedSource::point_to_1indexed(node.end_position());
                    let snippet = new_parsed.node_text(&node);

                    findings.push(Finding::new(
                        &self.id,
                        &self.name,
                        self.severity,
                        Confidence::High,
                        &ctx.file_diff.path,
                        start_line,
                        end_line,
                        start_col,
                        end_col,
                        self.message.clone(),
                        self.fix_hint.clone(),
                        node.kind(),
                        snippet,
                    ));
                    break; // Capture matched this occurrence
                }
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
    fn test_custom_rule_parsing_and_execution() {
        let scm = r#"
;; @id: TBCUST01
;; @name: no-hardcoded-secret
;; @severity: error
;; @message: Hardcoded credentials or secrets detected in source code
;; @fix_hint: Move credentials to environment variables
;; @languages: typescript, javascript

(variable_declarator
  name: (identifier) @name
  value: (string) @val) @match
"#;

        let rule = CustomRule::parse_from_scm(scm).expect("must parse custom rule");
        assert_eq!(rule.id, "TBCUST01");
        assert_eq!(rule.severity, Severity::Error);

        let code = "const secretKey = 'my-super-secret-token';\nconst normal = 123;\n".to_string();
        let parsed = ParsedSource::parse(Path::new("auth.ts"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("auth.ts"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: tb_diff::FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 1,
                old_lines: 0,
                new_start: 1,
                new_lines: 1,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(1),
                    content: "const secretKey = 'my-super-secret-token';".to_string(),
                }],
            }],
        };

        let findings = rule.check(&RuleContext::new(&file_diff, None, Some(&parsed)));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TBCUST01");
        assert!(findings[0].message.contains("Hardcoded credentials"));
    }

    #[test]
    fn test_custom_rule_fixture_runner() {
        let scm = r#"
;; @id: TBCUST02
;; @name: no-debugger
;; @severity: warn
;; @message: debugger statement found
;; @languages: typescript

(debugger_statement) @match
"#;
        let rule = CustomRule::parse_from_scm(scm).expect("must parse");
        let count = rule
            .test_fixture(
                "const a = 1;\ndebugger;\nconst b = 2;",
                SupportedLanguage::TypeScript,
            )
            .unwrap();
        assert_eq!(count, 1);

        let count_clean = rule
            .test_fixture("const a = 1;\nconst b = 2;", SupportedLanguage::TypeScript)
            .unwrap();
        assert_eq!(count_clean, 0);
    }
}
