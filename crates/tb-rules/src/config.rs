use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use globset::{Glob, GlobSetBuilder};
use serde::{Deserialize, Deserializer, Serialize};

use crate::model::Severity;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    #[default]
    Recommended,
    Strict,
    AgentPr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuleSetting {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<Severity>,
}

impl<'de> Deserialize<'de> for RuleSetting {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum RuleSettingHelper {
            Str(String),
            Obj {
                enabled: bool,
                severity: Option<Severity>,
            },
        }

        match RuleSettingHelper::deserialize(deserializer)? {
            RuleSettingHelper::Str(s) => match s.to_lowercase().as_str() {
                "off" | "disable" | "disabled" | "false" => Ok(RuleSetting {
                    enabled: false,
                    severity: None,
                }),
                "warn" | "warning" => Ok(RuleSetting {
                    enabled: true,
                    severity: Some(Severity::Warn),
                }),
                "error" => Ok(RuleSetting {
                    enabled: true,
                    severity: Some(Severity::Error),
                }),
                "info" => Ok(RuleSetting {
                    enabled: true,
                    severity: Some(Severity::Info),
                }),
                other => Err(serde::de::Error::custom(format!(
                    "Unknown rule setting '{}'. Expected 'off', 'warn', 'error', or 'info'",
                    other
                ))),
            },
            RuleSettingHelper::Obj { enabled, severity } => Ok(RuleSetting { enabled, severity }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OverrideConfig {
    pub files: Vec<String>,
    pub rules: HashMap<String, RuleSetting>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    #[serde(default)]
    pub extends: Option<Preset>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fail_on: Option<String>,

    #[serde(default)]
    pub rules: HashMap<String, RuleSetting>,

    #[serde(default)]
    pub ignore: Vec<String>,

    #[serde(default)]
    pub logger_names: Option<Vec<String>>,

    #[serde(default)]
    pub orm_modules: Option<Vec<String>>,

    #[serde(default)]
    pub overrides: Vec<OverrideConfig>,

    #[serde(default)]
    pub custom_rules_dir: Option<String>,

    #[serde(default)]
    pub org_policy: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: Some(
                "https://raw.githubusercontent.com/Tokenectomy-Labs/tokenectomy-bot/main/schema.json"
                    .to_string(),
            ),
            extends: Some(Preset::Recommended),
            fail_on: Some("error".to_string()),
            rules: HashMap::new(),
            ignore: vec![
                "**/vendor/**".to_string(),
                "**/dist/**".to_string(),
                "**/node_modules/**".to_string(),
                "**/*.generated.*".to_string(),
            ],
            logger_names: None,
            orm_modules: None,
            overrides: Vec::new(),
            custom_rules_dir: Some(".tokenectomy/rules".to_string()),
            org_policy: None,
        }
    }
}

impl Config {
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file at {:?}", path))?;
        Self::from_json_str(&content)
            .with_context(|| format!("Failed to parse config file at {:?}", path))
    }

    pub fn from_json_str(content: &str) -> Result<Self> {
        let config: Config = serde_json::from_str(content)
            .map_err(|e| anyhow::anyhow!("Invalid tokenectomy.json format: {}", e))?;
        Ok(config)
    }

    /// Check if path matches any configured ignore globs
    pub fn is_path_ignored(&self, path: &Path) -> bool {
        if self.ignore.is_empty() {
            return false;
        }

        let mut builder = GlobSetBuilder::new();
        for pat in &self.ignore {
            if let Ok(glob) = Glob::new(pat) {
                builder.add(glob);
            }
        }

        if let Ok(set) = builder.build() {
            set.is_match(path)
        } else {
            false
        }
    }

    /// Resolve effective enabled state and severity for a given rule and optional file path
    pub fn effective_rule_setting(
        &self,
        rule_id: &str,
        file_path: Option<&Path>,
        default_severity: Severity,
    ) -> (bool, Severity) {
        let preset = self.extends.unwrap_or(Preset::Recommended);

        // 1. Base preset severity
        let mut enabled = true;
        let mut severity = match preset {
            Preset::Strict => Severity::Error,
            Preset::AgentPr => {
                if rule_id.starts_with("TB00") {
                    Severity::Error
                } else {
                    default_severity
                }
            }
            Preset::Recommended => default_severity,
        };

        // 2. Global rules configuration
        if let Some(setting) = self.rules.get(rule_id) {
            enabled = setting.enabled;
            if let Some(sev) = setting.severity {
                severity = sev;
            }
        }

        // 3. Path overrides (applied in order, later entries override earlier ones)
        if let Some(path) = file_path {
            for ov in &self.overrides {
                let mut builder = GlobSetBuilder::new();
                for pat in &ov.files {
                    if let Ok(glob) = Glob::new(pat) {
                        builder.add(glob);
                    }
                }
                if let Ok(set) = builder.build()
                    && set.is_match(path)
                    && let Some(setting) = ov.rules.get(rule_id)
                {
                    enabled = setting.enabled;
                    if let Some(sev) = setting.severity {
                        severity = sev;
                    }
                }
            }
        }

        (enabled, severity)
    }

    pub fn effective_logger_names(&self) -> Vec<String> {
        let mut names = vec![
            "console".to_string(),
            "logger".to_string(),
            "log".to_string(),
            "tracing".to_string(),
            "slog".to_string(),
            "env_logger".to_string(),
        ];
        if let Some(ref custom) = self.logger_names {
            for name in custom {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
        names
    }

    pub fn effective_orm_modules(&self) -> Vec<String> {
        let mut modules = vec![
            "@prisma/client".to_string(),
            "prisma".to_string(),
            "drizzle-orm".to_string(),
            "typeorm".to_string(),
            "mongoose".to_string(),
            "sequelize".to_string(),
            "knex".to_string(),
            "kysely".to_string(),
        ];
        if let Some(ref custom) = self.orm_modules {
            for module in custom {
                if !modules.contains(module) {
                    modules.push(module.clone());
                }
            }
        }
        modules
    }

    /// Apply an organization-wide policy on top of this repository configuration.
    /// If `allow_downgrade` is false, the local repo configuration cannot disable rules
    /// or reduce rule severity below what the organization policy mandates.
    pub fn apply_org_policy(&mut self, org: &Config, allow_downgrade: bool) -> Result<()> {
        if let Some(org_preset) = org.extends {
            self.extends = Some(org_preset);
        }

        for (rule_id, org_setting) in &org.rules {
            if !allow_downgrade && let Some(local_setting) = self.rules.get(rule_id) {
                if org_setting.enabled && !local_setting.enabled {
                    bail!(
                        "Organization policy forbids disabling rule '{}' without an approved exception",
                        rule_id
                    );
                }
                if org_setting.severity == Some(Severity::Error)
                    && local_setting.severity != Some(Severity::Error)
                {
                    bail!(
                        "Organization policy enforces Error severity for rule '{}'; downgrade rejected without approval",
                        rule_id
                    );
                }
            }
            self.rules
                .entry(rule_id.clone())
                .or_insert_with(|| org_setting.clone());
        }

        for pat in &org.ignore {
            if !self.ignore.contains(pat) {
                self.ignore.push(pat.clone());
            }
        }

        if self.custom_rules_dir.is_none() && org.custom_rules_dir.is_some() {
            self.custom_rules_dir = org.custom_rules_dir.clone();
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_string_rule_settings() {
        let json = r#"{
            "extends": "strict",
            "rules": {
                "TB101": "off",
                "TB102": "warn",
                "TB201": "error"
            },
            "ignore": ["**/tests/**"]
        }"#;

        let config = Config::from_json_str(json).expect("should parse");
        assert_eq!(config.extends, Some(Preset::Strict));
        assert_eq!(
            config.rules.get("TB101"),
            Some(&RuleSetting {
                enabled: false,
                severity: None
            })
        );
        assert_eq!(
            config.rules.get("TB102"),
            Some(&RuleSetting {
                enabled: true,
                severity: Some(Severity::Warn)
            })
        );
        assert!(config.is_path_ignored(Path::new("src/tests/dummy.ts")));
        assert!(!config.is_path_ignored(Path::new("src/main.ts")));
    }

    #[test]
    fn test_path_overrides() {
        let json = r#"{
            "rules": {
                "TB101": "error"
            },
            "overrides": [
                {
                    "files": ["legacy/**"],
                    "rules": {
                        "TB101": "off"
                    }
                }
            ]
        }"#;

        let config = Config::from_json_str(json).expect("should parse");
        let (enabled_normal, sev_normal) = config.effective_rule_setting(
            "TB101",
            Some(Path::new("src/service.ts")),
            Severity::Warn,
        );
        assert!(enabled_normal);
        assert_eq!(sev_normal, Severity::Error);

        let (enabled_legacy, _) = config.effective_rule_setting(
            "TB101",
            Some(Path::new("legacy/old_service.ts")),
            Severity::Warn,
        );
        assert!(!enabled_legacy);
    }

    #[test]
    fn test_org_policy_enforcement() {
        let org_json = r#"{
            "extends": "strict",
            "rules": {
                "TB001": "error",
                "TB002": "error"
            }
        }"#;
        let org_config = Config::from_json_str(org_json).unwrap();

        let mut repo_config = Config::default();
        repo_config.rules.insert(
            "TB001".to_string(),
            RuleSetting {
                enabled: false,
                severity: None,
            },
        );

        // Disabling TB001 should fail without allow_downgrade
        let err = repo_config.apply_org_policy(&org_config, false);
        assert!(err.is_err());
        assert!(
            err.unwrap_err()
                .to_string()
                .contains("forbids disabling rule 'TB001'")
        );

        // With allow_downgrade = true, it should succeed
        let ok = repo_config.apply_org_policy(&org_config, true);
        assert!(ok.is_ok());
    }
}
