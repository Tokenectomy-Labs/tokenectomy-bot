use colored::*;
use tb_rules::{Finding, Severity};

pub struct TextReporter;

impl TextReporter {
    pub fn format(findings: &[Finding]) -> String {
        if findings.is_empty() {
            return format!(
                "\n{}\n  No issues found. PR is clean and deterministic verification passed.\n",
                "✔ Tokenectomy Bot: PASSED".green().bold()
            );
        }

        let mut out = String::new();
        out.push_str(&format!(
            "\n{}\n",
            "🗡️ Tokenectomy Bot — PR Gate Report".bold().underline()
        ));

        let mut errors = 0;
        let mut warns = 0;
        let mut infos = 0;

        for f in findings {
            let badge = match f.severity {
                Severity::Error => {
                    errors += 1;
                    "[ERROR]".red().bold()
                }
                Severity::Warn => {
                    warns += 1;
                    "[WARN]".yellow().bold()
                }
                Severity::Info => {
                    infos += 1;
                    "[INFO]".cyan().bold()
                }
            };

            let loc = format!("{}:{}:{}", f.file.display(), f.start_line, f.start_col);
            out.push_str(&format!(
                "\n{} {}: {} ({})\n",
                badge,
                f.rule_id.bold(),
                f.rule_name,
                loc.dimmed()
            ));
            out.push_str(&format!("   Message:  {}\n", f.message));

            if let Some(ref hint) = f.fix_hint {
                out.push_str(&format!("   Fix Hint: {}\n", hint.bright_blue()));
            }
            out.push_str(&format!("   Fingerprint: {}\n", f.fingerprint.dimmed()));
        }

        out.push_str(&format!(
            "\n{}\n  Total findings: {} ({} errors, {} warnings, {} info)\n",
            "─".repeat(50).dimmed(),
            findings.len(),
            errors.to_string().red().bold(),
            warns.to_string().yellow().bold(),
            infos.to_string().cyan()
        ));

        out
    }
}
