use serde::Serialize;
use tb_rules::{Finding, Severity};

#[derive(Serialize)]
pub struct ReportSummary {
    pub total: usize,
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub passed: bool,
}

#[derive(Serialize)]
pub struct JsonReport<'a> {
    pub version: &'static str,
    pub summary: ReportSummary,
    pub findings: &'a [Finding],
}

pub struct JsonReporter;

impl JsonReporter {
    pub fn format(findings: &[Finding]) -> String {
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

        let report = JsonReport {
            version: "2.0.0",
            summary: ReportSummary {
                total: findings.len(),
                errors,
                warnings,
                infos,
                passed: errors == 0,
            },
            findings,
        };

        serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string())
    }
}
