use std::path::Path;

use anyhow::{Result, bail};
use colored::*;
use tb_report::AuditLedger;

pub fn run_ledger_verify(ledger_file: &Path) -> Result<()> {
    let res = AuditLedger::verify_chain(ledger_file)?;
    if res.is_valid {
        println!(
            "{} Cryptographic Ledger Integrity Verified!",
            "✔".green().bold()
        );
        println!("  Total Entries:  {}", res.total_entries);
        println!("  Head Seal:      {}", res.head_seal.cyan());
        Ok(())
    } else {
        println!(
            "{} Cryptographic Ledger Seal TAMPERED / INVALID!",
            "✘".red().bold()
        );
        println!("  Total Entries:      {}", res.total_entries);
        println!("  Compromised Index:  {:?}", res.compromised_index);
        println!(
            "  Error:              {}",
            res.error_message.unwrap_or_default().red()
        );
        bail!("Ledger audit verification failed");
    }
}

pub fn run_ledger_metrics(ledger_file: &Path) -> Result<()> {
    let metrics = AuditLedger::calculate_metrics(ledger_file)?;
    println!(
        "\n{}",
        "── Tokenectomy PR Gate — Ledger Dashboard ──"
            .green()
            .bold()
    );
    println!("Total PRs Reviewed:       {}", metrics.total_reviews);
    println!("PRs Passed:               {}", metrics.passed_reviews);
    println!("PRs Blocked:              {}", metrics.blocked_reviews);
    println!(
        "Pass Rate:                {:.1}%",
        metrics.pass_rate_percent
    );
    println!("Total Findings:           {}", metrics.total_findings);
    println!("Errors:                   {}", metrics.total_errors);
    println!("Warnings:                 {}", metrics.total_warnings);
    println!(
        "Tampering Blocked:        {}",
        if metrics.tampering_attempts_detected > 0 {
            format!("{}", metrics.tampering_attempts_detected)
                .red()
                .bold()
        } else {
            "0".normal()
        }
    );
    println!(
        "Ledger Chain Status:      {}",
        if metrics.is_chain_valid {
            "VERIFIED ✔".green()
        } else {
            "COMPROMISED ✘".red()
        }
    );
    println!("Head Seal:                {}", metrics.head_seal.cyan());

    if !metrics.rule_frequencies.is_empty() {
        println!("\n{}", "Top Triggered Rules:".bold());
        let mut sorted: Vec<_> = metrics.rule_frequencies.into_iter().collect();
        sorted.sort_by_key(|a| std::cmp::Reverse(a.1));
        for (rule, count) in sorted.iter().take(10) {
            println!("  • {:<12} : {} hits", rule.yellow(), count);
        }
    }
    println!();
    Ok(())
}
