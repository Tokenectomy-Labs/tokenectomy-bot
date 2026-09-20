use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use tb_rules::{Finding, Severity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FailOn {
    Error,
    Warn,
    None,
}

pub struct GatePolicy;

impl GatePolicy {
    pub fn evaluate_exit_code(findings: &[Finding], policy: FailOn) -> i32 {
        match policy {
            FailOn::None => 0,
            FailOn::Error => {
                if findings.iter().any(|f| f.severity == Severity::Error) {
                    1
                } else {
                    0
                }
            }
            FailOn::Warn => {
                if findings
                    .iter()
                    .any(|f| f.severity == Severity::Error || f.severity == Severity::Warn)
                {
                    1
                } else {
                    0
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tb_rules::Confidence;

    #[test]
    fn test_gate_policy() {
        let findings = vec![Finding::new(
            "TB101",
            "silent-catch",
            Severity::Warn,
            Confidence::High,
            PathBuf::from("test.ts"),
            1,
            2,
            1,
            1,
            "silent catch",
            None,
            "catch_clause",
            "catch {}",
        )];

        assert_eq!(GatePolicy::evaluate_exit_code(&findings, FailOn::Error), 0);
        assert_eq!(GatePolicy::evaluate_exit_code(&findings, FailOn::Warn), 1);
        assert_eq!(GatePolicy::evaluate_exit_code(&findings, FailOn::None), 0);
    }
}
