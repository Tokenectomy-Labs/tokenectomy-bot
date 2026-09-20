use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tb_rules::{Finding, Severity};

pub const GENESIS_SEAL: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerEntry {
    pub index: u64,
    pub timestamp: String,
    pub repo_name: String,
    pub base_ref: String,
    pub head_ref: String,
    pub total_files: usize,
    pub findings_count: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub tamper_count: usize,
    pub gate_passed: bool,
    #[serde(default)]
    pub rule_counts: HashMap<String, usize>,
    pub fingerprints: Vec<String>,
    pub previous_seal: String,
    pub entry_seal: String,
}

impl LedgerEntry {
    #[allow(clippy::too_many_arguments)]
    pub fn calculate_seal(
        index: u64,
        timestamp: &str,
        repo_name: &str,
        base_ref: &str,
        head_ref: &str,
        total_files: usize,
        findings_count: usize,
        error_count: usize,
        warning_count: usize,
        tamper_count: usize,
        gate_passed: bool,
        fingerprints: &[String],
        rule_counts: &HashMap<String, usize>,
        previous_seal: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(index.to_le_bytes());
        hasher.update(b":");
        hasher.update(timestamp.as_bytes());
        hasher.update(b":");
        hasher.update(repo_name.as_bytes());
        hasher.update(b":");
        hasher.update(base_ref.as_bytes());
        hasher.update(b":");
        hasher.update(head_ref.as_bytes());
        hasher.update(b":");
        hasher.update(total_files.to_le_bytes());
        hasher.update(b":");
        hasher.update(findings_count.to_le_bytes());
        hasher.update(b":");
        hasher.update(error_count.to_le_bytes());
        hasher.update(b":");
        hasher.update(warning_count.to_le_bytes());
        hasher.update(b":");
        hasher.update(tamper_count.to_le_bytes());
        hasher.update(b":");
        hasher.update([if gate_passed { 1u8 } else { 0u8 }]);
        hasher.update(b":");
        for fp in fingerprints {
            hasher.update(fp.as_bytes());
            hasher.update(b",");
        }
        hasher.update(b":");
        let mut sorted_rules: Vec<_> = rule_counts.iter().collect();
        sorted_rules.sort_by_key(|(k, _)| *k);
        for (r, cnt) in sorted_rules {
            hasher.update(r.as_bytes());
            hasher.update(b"=");
            hasher.update(cnt.to_le_bytes());
            hasher.update(b",");
        }
        hasher.update(b":");
        hasher.update(previous_seal.as_bytes());

        let result = hasher.finalize();
        let mut hex = String::with_capacity(64);
        for b in result {
            use std::fmt::Write;
            let _ = write!(hex, "{:02x}", b);
        }
        hex
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerVerificationResult {
    pub is_valid: bool,
    pub total_entries: usize,
    pub head_seal: String,
    pub compromised_index: Option<u64>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LedgerMetrics {
    pub total_reviews: usize,
    pub passed_reviews: usize,
    pub blocked_reviews: usize,
    pub pass_rate_percent: f64,
    pub total_findings: usize,
    pub total_errors: usize,
    pub total_warnings: usize,
    pub tampering_attempts_detected: usize,
    pub rule_frequencies: HashMap<String, usize>,
    pub head_seal: String,
    pub is_chain_valid: bool,
}

pub struct AuditLedger;

impl AuditLedger {
    #[allow(clippy::too_many_arguments)]
    pub fn build_entry(
        ledger_file: &Path,
        repo_name: &str,
        base_ref: &str,
        head_ref: &str,
        total_files: usize,
        findings: &[Finding],
        gate_passed: bool,
        timestamp: &str,
    ) -> Result<LedgerEntry> {
        let (index, previous_seal) = if ledger_file.exists() {
            let entries = Self::read_all(ledger_file)?;
            if let Some(last) = entries.last() {
                (last.index + 1, last.entry_seal.clone())
            } else {
                (0, GENESIS_SEAL.to_string())
            }
        } else {
            (0, GENESIS_SEAL.to_string())
        };

        let mut error_count = 0;
        let mut warning_count = 0;
        let mut tamper_count = 0;
        let mut rule_counts = HashMap::new();

        for f in findings {
            if f.severity == Severity::Error {
                error_count += 1;
            } else if f.severity == Severity::Warn {
                warning_count += 1;
            }
            if f.rule_id.starts_with("TB00") {
                tamper_count += 1;
            }
            *rule_counts.entry(f.rule_id.clone()).or_insert(0) += 1;
        }

        let fingerprints: Vec<String> = findings.iter().map(|f| f.fingerprint.clone()).collect();

        let entry_seal = LedgerEntry::calculate_seal(
            index,
            timestamp,
            repo_name,
            base_ref,
            head_ref,
            total_files,
            findings.len(),
            error_count,
            warning_count,
            tamper_count,
            gate_passed,
            &fingerprints,
            &rule_counts,
            &previous_seal,
        );

        Ok(LedgerEntry {
            index,
            timestamp: timestamp.to_string(),
            repo_name: repo_name.to_string(),
            base_ref: base_ref.to_string(),
            head_ref: head_ref.to_string(),
            total_files,
            findings_count: findings.len(),
            error_count,
            warning_count,
            tamper_count,
            gate_passed,
            rule_counts,
            fingerprints,
            previous_seal,
            entry_seal,
        })
    }

    pub fn append(ledger_file: &Path, entry: &LedgerEntry) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(ledger_file)
            .with_context(|| format!("Failed to open ledger file for append: {:?}", ledger_file))?;

        let json_line = serde_json::to_string(entry)
            .with_context(|| "Failed to serialize ledger entry to JSON")?;
        writeln!(file, "{}", json_line)?;
        file.flush()?;
        Ok(())
    }

    pub fn read_all(ledger_file: &Path) -> Result<Vec<LedgerEntry>> {
        if !ledger_file.exists() {
            return Ok(Vec::new());
        }

        let file = fs::File::open(ledger_file)
            .with_context(|| format!("Failed to open ledger file: {:?}", ledger_file))?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();

        for (idx, line_res) in reader.lines().enumerate() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let entry: LedgerEntry = serde_json::from_str(trimmed)
                .with_context(|| format!("Malformed ledger JSON entry at line {}", idx + 1))?;
            entries.push(entry);
        }

        Ok(entries)
    }

    pub fn verify_chain(ledger_file: &Path) -> Result<LedgerVerificationResult> {
        let entries = Self::read_all(ledger_file)?;
        if entries.is_empty() {
            return Ok(LedgerVerificationResult {
                is_valid: true,
                total_entries: 0,
                head_seal: GENESIS_SEAL.to_string(),
                compromised_index: None,
                error_message: None,
            });
        }

        let mut expected_prev_seal = GENESIS_SEAL.to_string();

        for (expected_idx, entry) in entries.iter().enumerate() {
            let exp_u64 = expected_idx as u64;

            if entry.index != exp_u64 {
                return Ok(LedgerVerificationResult {
                    is_valid: false,
                    total_entries: entries.len(),
                    head_seal: entries.last().unwrap().entry_seal.clone(),
                    compromised_index: Some(entry.index),
                    error_message: Some(format!(
                        "Ledger index sequence broken. Expected {}, found {}",
                        exp_u64, entry.index
                    )),
                });
            }

            if entry.previous_seal != expected_prev_seal {
                return Ok(LedgerVerificationResult {
                    is_valid: false,
                    total_entries: entries.len(),
                    head_seal: entries.last().unwrap().entry_seal.clone(),
                    compromised_index: Some(entry.index),
                    error_message: Some(format!(
                        "Hash chain broken at index {}. Previous seal mismatch.",
                        entry.index
                    )),
                });
            }

            let expected_seal = LedgerEntry::calculate_seal(
                entry.index,
                &entry.timestamp,
                &entry.repo_name,
                &entry.base_ref,
                &entry.head_ref,
                entry.total_files,
                entry.findings_count,
                entry.error_count,
                entry.warning_count,
                entry.tamper_count,
                entry.gate_passed,
                &entry.fingerprints,
                &entry.rule_counts,
                &entry.previous_seal,
            );

            if entry.entry_seal != expected_seal {
                return Ok(LedgerVerificationResult {
                    is_valid: false,
                    total_entries: entries.len(),
                    head_seal: entries.last().unwrap().entry_seal.clone(),
                    compromised_index: Some(entry.index),
                    error_message: Some(format!(
                        "Entry cryptographic seal tampered at index {}.",
                        entry.index
                    )),
                });
            }

            expected_prev_seal = entry.entry_seal.clone();
        }

        let head_seal = entries.last().unwrap().entry_seal.clone();
        Ok(LedgerVerificationResult {
            is_valid: true,
            total_entries: entries.len(),
            head_seal,
            compromised_index: None,
            error_message: None,
        })
    }

    pub fn calculate_metrics(ledger_file: &Path) -> Result<LedgerMetrics> {
        let entries = Self::read_all(ledger_file)?;
        let verification = Self::verify_chain(ledger_file)?;
        let total_reviews = entries.len();
        let mut passed_reviews = 0;
        let mut total_findings = 0;
        let mut total_errors = 0;
        let mut total_warnings = 0;
        let mut tampering_attempts_detected = 0;
        let mut rule_frequencies: HashMap<String, usize> = HashMap::new();

        for entry in &entries {
            if entry.gate_passed {
                passed_reviews += 1;
            }
            total_findings += entry.findings_count;
            total_errors += entry.error_count;
            total_warnings += entry.warning_count;
            tampering_attempts_detected += entry.tamper_count;
            for (rule, cnt) in &entry.rule_counts {
                *rule_frequencies.entry(rule.clone()).or_insert(0) += cnt;
            }
        }

        let blocked_reviews = total_reviews.saturating_sub(passed_reviews);
        let pass_rate_percent = if total_reviews > 0 {
            (passed_reviews as f64 / total_reviews as f64) * 100.0
        } else {
            100.0
        };

        Ok(LedgerMetrics {
            total_reviews,
            passed_reviews,
            blocked_reviews,
            pass_rate_percent,
            total_findings,
            total_errors,
            total_warnings,
            tampering_attempts_detected,
            rule_frequencies,
            head_seal: verification.head_seal,
            is_chain_valid: verification.is_valid,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_ledger_append_and_verify() {
        let dir = tempdir().unwrap();
        let ledger_path = dir.path().join("test-ledger.jsonl");

        let entry1 = AuditLedger::build_entry(
            &ledger_path,
            "org/repo",
            "main",
            "feat/1",
            10,
            &[],
            true,
            "2026-09-21T03:00:00Z",
        )
        .unwrap();
        assert_eq!(entry1.index, 0);
        assert_eq!(entry1.previous_seal, GENESIS_SEAL);
        AuditLedger::append(&ledger_path, &entry1).unwrap();

        let entry2 = AuditLedger::build_entry(
            &ledger_path,
            "org/repo",
            "main",
            "feat/2",
            12,
            &[],
            true,
            "2026-09-21T03:05:00Z",
        )
        .unwrap();
        assert_eq!(entry2.index, 1);
        assert_eq!(entry2.previous_seal, entry1.entry_seal);
        AuditLedger::append(&ledger_path, &entry2).unwrap();

        let verify_res = AuditLedger::verify_chain(&ledger_path).unwrap();
        assert!(verify_res.is_valid);
        assert_eq!(verify_res.total_entries, 2);
        assert_eq!(verify_res.head_seal, entry2.entry_seal);

        let metrics = AuditLedger::calculate_metrics(&ledger_path).unwrap();
        assert_eq!(metrics.total_reviews, 2);
        assert_eq!(metrics.passed_reviews, 2);
        assert_eq!(metrics.blocked_reviews, 0);
        assert_eq!(metrics.pass_rate_percent, 100.0);
    }

    #[test]
    fn test_ledger_tamper_detection() {
        let dir = tempdir().unwrap();
        let ledger_path = dir.path().join("tampered-ledger.jsonl");

        let entry1 = AuditLedger::build_entry(
            &ledger_path,
            "org/repo",
            "main",
            "feat/1",
            10,
            &[],
            true,
            "2026-09-21T03:00:00Z",
        )
        .unwrap();
        AuditLedger::append(&ledger_path, &entry1).unwrap();

        let entry2 = AuditLedger::build_entry(
            &ledger_path,
            "org/repo",
            "main",
            "feat/2",
            12,
            &[],
            true,
            "2026-09-21T03:05:00Z",
        )
        .unwrap();
        AuditLedger::append(&ledger_path, &entry2).unwrap();

        // Tamper with entry 1: change total_files from 10 to 999
        let content = fs::read_to_string(&ledger_path).unwrap();
        let tampered = content.replace("\"total_files\":10", "\"total_files\":999");
        fs::write(&ledger_path, tampered).unwrap();

        let verify_res = AuditLedger::verify_chain(&ledger_path).unwrap();
        assert!(!verify_res.is_valid);
        assert_eq!(verify_res.compromised_index, Some(0));
        assert!(verify_res.error_message.unwrap().contains("tampered"));
    }
}
