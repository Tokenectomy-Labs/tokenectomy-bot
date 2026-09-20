use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use colored::*;
use tb_parse::SupportedLanguage;
use tb_rules::CustomRule;

pub fn run_rules_validate(
    rule_file: &Path,
    fixture: Option<&Path>,
    language: Option<&str>,
) -> Result<()> {
    let content = fs::read_to_string(rule_file)
        .with_context(|| format!("Failed to read rule file at {:?}", rule_file))?;
    let rule = CustomRule::parse_from_scm(&content)
        .with_context(|| format!("Failed to parse rule file at {:?}", rule_file))?;

    println!("{} Custom rule is valid!", "✔".green().bold());
    println!("  Rule ID:     {}", rule.id.cyan().bold());
    println!("  Name:        {}", rule.name);
    println!("  Severity:    {:?}", rule.severity);
    println!("  Message:     {}", rule.message);
    if let Some(ref hint) = rule.fix_hint {
        println!("  Fix Hint:    {}", hint);
    }
    if !rule.languages.is_empty() {
        println!("  Languages:   {:?}", rule.languages);
    } else {
        println!("  Languages:   [All Supported]");
    }

    if let Some(fix_path) = fixture {
        let fix_content = fs::read_to_string(fix_path)
            .with_context(|| format!("Failed to read fixture file at {:?}", fix_path))?;

        let lang = if let Some(l_str) = language {
            SupportedLanguage::from_name(l_str)
                .ok_or_else(|| anyhow::anyhow!("Unknown language '{}'", l_str))?
        } else if let Some(l) = SupportedLanguage::from_path(fix_path) {
            l
        } else {
            SupportedLanguage::TypeScript
        };

        let hits = rule.test_fixture(&fix_content, lang)?;
        println!(
            "  Fixture:     {} (matched {} finding(s))",
            if hits > 0 {
                "✔ PASSED".green()
            } else {
                "✘ NO HITS".red()
            },
            hits
        );
        if hits == 0 {
            bail!(
                "Fixture at {:?} did not trigger any findings for rule {}",
                fix_path,
                rule.id
            );
        }
    }

    Ok(())
}
