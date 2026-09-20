use serde::Serialize;
use tb_rules::{Finding, Severity};

#[derive(Serialize)]
struct SarifMessage {
    text: String,
}

#[derive(Serialize)]
struct SarifArtifactLocation {
    uri: String,
}

#[derive(Serialize)]
struct SarifRegion {
    #[serde(rename = "startLine")]
    start_line: usize,
    #[serde(rename = "endLine")]
    end_line: usize,
    #[serde(rename = "startColumn")]
    start_column: usize,
    #[serde(rename = "endColumn")]
    end_column: usize,
}

#[derive(Serialize)]
struct SarifPhysicalLocation {
    #[serde(rename = "artifactLocation")]
    artifact_location: SarifArtifactLocation,
    region: SarifRegion,
}

#[derive(Serialize)]
struct SarifLocation {
    #[serde(rename = "physicalLocation")]
    physical_location: SarifPhysicalLocation,
}

#[derive(Serialize)]
struct SarifResult {
    #[serde(rename = "ruleId")]
    rule_id: String,
    level: &'static str,
    message: SarifMessage,
    locations: Vec<SarifLocation>,
}

#[derive(Serialize)]
struct SarifDriver {
    name: &'static str,
    version: &'static str,
    #[serde(rename = "informationUri")]
    information_uri: &'static str,
}

#[derive(Serialize)]
struct SarifTool {
    driver: SarifDriver,
}

#[derive(Serialize)]
struct SarifRun {
    tool: SarifTool,
    results: Vec<SarifResult>,
}

#[derive(Serialize)]
struct SarifDocument {
    version: &'static str,
    #[serde(rename = "$schema")]
    schema: &'static str,
    runs: Vec<SarifRun>,
}

pub struct SarifReporter;

impl SarifReporter {
    pub fn format(findings: &[Finding]) -> String {
        let results = findings
            .iter()
            .map(|f| {
                let level = match f.severity {
                    Severity::Error => "error",
                    Severity::Warn => "warning",
                    Severity::Info => "note",
                };

                SarifResult {
                    rule_id: f.rule_id.clone(),
                    level,
                    message: SarifMessage {
                        text: f.message.clone(),
                    },
                    locations: vec![SarifLocation {
                        physical_location: SarifPhysicalLocation {
                            artifact_location: SarifArtifactLocation {
                                uri: f.file.to_string_lossy().replace('\\', "/"),
                            },
                            region: SarifRegion {
                                start_line: f.start_line,
                                end_line: f.end_line,
                                start_column: f.start_col,
                                end_column: f.end_col,
                            },
                        },
                    }],
                }
            })
            .collect();

        let doc = SarifDocument {
            version: "2.1.0",
            schema: "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
            runs: vec![SarifRun {
                tool: SarifTool {
                    driver: SarifDriver {
                        name: "tokenectomy-bot",
                        version: "0.1.0",
                        information_uri: "https://github.com/Tokenectomy-Labs/tokenectomy-bot",
                    },
                },
                results,
            }],
        };

        serde_json::to_string_pretty(&doc).unwrap_or_else(|_| "{}".to_string())
    }
}
