use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use tb_rules::{Finding, Severity};

pub const STICKY_MARKER: &str = "<!-- tokenectomy-bot:sticky-summary -->";

pub struct StickySummaryReporter;

impl StickySummaryReporter {
    pub fn format(findings: &[Finding], total_scanned: usize, total_files: usize) -> String {
        Self::format_with_blast_radius(findings, total_scanned, total_files, None)
    }

    pub fn format_with_blast_radius(
        findings: &[Finding],
        total_scanned: usize,
        total_files: usize,
        blast_radius: Option<&tb_rules::BlastRadiusReport>,
    ) -> String {
        let mut out = String::new();
        out.push_str(STICKY_MARKER);
        out.push_str("\n\n");

        let errors = findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count();
        let warnings = findings
            .iter()
            .filter(|f| f.severity == Severity::Warn)
            .count();
        let infos = findings
            .iter()
            .filter(|f| f.severity == Severity::Info)
            .count();

        if findings.is_empty() {
            out.push_str("## 🗡️ Tmy-Joy: PR Verification PASSED ✔\n\n");
            out.push_str("> **All deterministic verification checks passed.** No test tampering, silent catch, or security flaws detected.\n\n");
            if let Some(br) = blast_radius {
                Self::append_blast_radius_section(&mut out, br);
            }
            out.push_str(&format!(
                "*Dipindai {} dari {} berkas. Engine AST Tree-sitter zero-LLM.*\n",
                total_scanned, total_files
            ));
            return out;
        }

        let status_header = if errors > 0 {
            format!(
                "## 🗡️ Tmy-Joy: BLOCKED ({} errors, {} warnings, {} info) ❌",
                errors, warnings, infos
            )
        } else if warnings > 0 {
            format!(
                "## 🗡️ Tmy-Joy: WARNING ({} warnings, {} info) ⚠️",
                warnings, infos
            )
        } else {
            format!("## 🗡️ Tmy-Joy: NOTICE ({} info) ℹ️", infos)
        };

        out.push_str(&status_header);
        out.push_str("\n\n");

        if total_scanned < total_files {
            out.push_str(&format!(
                "> ⚠️ **Laporan Terpotong**: Dipindai {} dari {} berkas (PR melebihi batas konfigurasi).\n\n",
                total_scanned, total_files
            ));
        }

        out.push_str("| Severity | Rule | Lokasi | Pesan |\n");
        out.push_str("|:---:|:---|:---|:---|\n");

        for f in findings {
            let badge = match f.severity {
                Severity::Error => "🔴 **ERROR**",
                Severity::Warn => "🟡 **WARN**",
                Severity::Info => "🔵 **INFO**",
            };
            let loc = format!("`{}:{}`", f.file.display(), f.start_line);
            let rule = format!("`{}` ({})", f.rule_id, f.rule_name);
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                badge, rule, loc, f.message
            ));
        }

        out.push_str("\n<details>\n<summary>🔎 <b>Rincian Perbaikan & Fix Hints</b></summary>\n\n");

        for f in findings {
            out.push_str(&format!(
                "### `{}` — {} (`{}:{}`)\n",
                f.rule_id,
                f.rule_name,
                f.file.display(),
                f.start_line
            ));
            out.push_str(&format!("* **Masalah**: {}\n", f.message));
            if let Some(ref hint) = f.fix_hint {
                out.push_str(&format!("* **Saran Perbaikan**: {}\n", hint));
            }
            out.push_str(&format!("* **Fingerprint**: `{}`\n\n", f.fingerprint));
        }

        out.push_str("</details>\n\n");

        if let Some(br) = blast_radius {
            Self::append_blast_radius_section(&mut out, br);
        }

        out.push_str(&format!(
            "---\n*Laporan dihasilkan secara deterministik oleh [tokenectomy-bot](https://github.com/Tokenectomy-Labs/tokenectomy-bot) ({}/{} berkas dipindai).*\n",
            total_scanned, total_files
        ));

        out
    }

    fn append_blast_radius_section(out: &mut String, br: &tb_rules::BlastRadiusReport) {
        if br.direct_dependents.is_empty() && br.modified_files.is_empty() {
            return;
        }

        let badge = match br.risk_level {
            tb_rules::RiskLevel::Low => "🟢 LOW",
            tb_rules::RiskLevel::Medium => "🟡 MEDIUM",
            tb_rules::RiskLevel::High => "🟠 HIGH",
            tb_rules::RiskLevel::Critical => "🔴 CRITICAL",
        };

        out.push_str(&format!(
            "### 🗺️ Blast Radius & Impact Architecture\n\n\
            > **Risk Level**: {} (Score: **{}/100**)\n\
            > *Modifikasi ini berdampak pada {} berkas pemanggil langsung dan {} berkas turunan.*\n\n",
            badge,
            br.risk_score,
            br.direct_dependents.len(),
            br.indirect_dependents.len()
        ));

        if !br.sensitive_paths_affected.is_empty() {
            let sensitive_str = br
                .sensitive_paths_affected
                .iter()
                .map(|p| format!("`{}`", p.display()))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!(
                "🔒 **Perhatian Modul Kritis**: Perubahan ini menyentuh jalur sensitif: {}\n\n",
                sensitive_str
            ));
        }

        out.push_str(&br.mermaid_diagram);
        out.push_str("\n\n");
    }

    /// Automatically appends the summary to $GITHUB_STEP_SUMMARY if running in GitHub Actions
    pub fn write_to_step_summary(content: &str) -> std::io::Result<()> {
        if let Some(summary_file) = env::var("GITHUB_STEP_SUMMARY")
            .ok()
            .filter(|s| !s.trim().is_empty())
        {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(summary_file)?;
            writeln!(file, "{}", content)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tb_rules::Confidence;

    #[test]
    fn test_sticky_summary_format() {
        let findings = vec![Finding::new(
            "TB002",
            "test-disabled",
            Severity::Error,
            Confidence::High,
            PathBuf::from("src/auth.test.ts"),
            10,
            10,
            1,
            20,
            "Test disabled via .skip",
            Some("Fix the test instead of skipping".to_string()),
            "call_expression",
            "it.skip",
        )];

        let summary = StickySummaryReporter::format(&findings, 10, 10);
        assert!(summary.contains(STICKY_MARKER));
        assert!(summary.contains("TB002"));
        assert!(summary.contains("BLOCKED"));
        assert!(summary.contains("Fix the test instead of skipping"));
    }

    #[test]
    fn test_sticky_summary_passed() {
        let summary = StickySummaryReporter::format(&[], 5, 5);
        assert!(summary.contains(STICKY_MARKER));
        assert!(summary.contains("PASSED"));
    }
}
