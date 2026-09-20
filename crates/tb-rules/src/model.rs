use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::{Path, PathBuf};
use tb_diff::FileDiff;
use tb_parse::ParsedSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warn => write!(f, "warn"),
            Severity::Info => write!(f, "info"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

impl fmt::Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Confidence::High => write!(f, "high"),
            Confidence::Medium => write!(f, "medium"),
            Confidence::Low => write!(f, "low"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub rule_id: String,
    pub rule_name: String,
    pub severity: Severity,
    pub confidence: Confidence,
    pub file: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub message: String,
    pub fix_hint: Option<String>,
    pub fingerprint: String,
}

impl Finding {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        rule_id: impl Into<String>,
        rule_name: impl Into<String>,
        severity: Severity,
        confidence: Confidence,
        file: impl Into<PathBuf>,
        start_line: usize,
        end_line: usize,
        start_col: usize,
        end_col: usize,
        message: impl Into<String>,
        fix_hint: Option<String>,
        node_kind: &str,
        snippet: &str,
    ) -> Self {
        let rule_id = rule_id.into();
        let rule_name = rule_name.into();
        let file = file.into();
        let message = message.into();
        let fingerprint = calculate_fingerprint(&rule_id, &file, node_kind, snippet);

        Self {
            rule_id,
            rule_name,
            severity,
            confidence,
            file,
            start_line,
            end_line,
            start_col,
            end_col,
            message,
            fix_hint,
            fingerprint,
        }
    }
}

pub fn calculate_fingerprint(rule_id: &str, file: &Path, node_kind: &str, snippet: &str) -> String {
    let mut hasher = Sha256::new();
    let norm_path = file.to_string_lossy().replace('\\', "/");
    hasher.update(rule_id.as_bytes());
    hasher.update(b":");
    hasher.update(norm_path.as_bytes());
    hasher.update(b":");
    hasher.update(node_kind.as_bytes());
    hasher.update(b":");
    hasher.update(snippet.trim().as_bytes());
    let result = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in result {
        use std::fmt::Write;
        let _ = write!(hex, "{:02x}", b);
    }
    hex
}

pub struct RuleContext<'a> {
    pub file_diff: &'a FileDiff,
    pub old_parsed: Option<&'a ParsedSource>,
    pub new_parsed: Option<&'a ParsedSource>,
    pub logger_names: Option<&'a [String]>,
    pub orm_modules: Option<&'a [String]>,
}

impl<'a> RuleContext<'a> {
    pub fn new(
        file_diff: &'a FileDiff,
        old_parsed: Option<&'a ParsedSource>,
        new_parsed: Option<&'a ParsedSource>,
    ) -> Self {
        Self {
            file_diff,
            old_parsed,
            new_parsed,
            logger_names: None,
            orm_modules: None,
        }
    }

    pub fn is_logger_name(&self, name: &str) -> bool {
        if let Some(custom) = self.logger_names
            && custom.iter().any(|c| c == name)
        {
            return true;
        }
        matches!(
            name,
            "console" | "logger" | "log" | "tracing" | "slog" | "env_logger"
        )
    }

    pub fn is_orm_module(&self, name: &str) -> bool {
        if let Some(custom) = self.orm_modules
            && custom.iter().any(|c| name.contains(c.as_str()))
        {
            return true;
        }
        name.contains("@prisma/client")
            || name.contains("prisma")
            || name.contains("drizzle-orm")
            || name.contains("typeorm")
            || name.contains("mongoose")
            || name.contains("sequelize")
            || name.contains("knex")
            || name.contains("kysely")
    }
}
