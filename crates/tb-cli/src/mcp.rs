use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use anyhow::Result;
use serde_json::{Value, json};

use tb_diff::{DiffParser, GitExtractor};
use tb_report::{FailOn, GatePolicy, StickySummaryReporter, TextReporter};
use tb_rules::{Config, RuleEngine};

pub fn run_mcp_server() -> Result<()> {
    eprintln!("[mcp] tokenectomy-bot MCP server starting on stdio...");
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    let mut line = String::new();
    while reader.read_line(&mut line)? > 0 {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }

        let parsed: Result<Value, _> = serde_json::from_str(trimmed);
        match parsed {
            Ok(req) => {
                if let Some(resp) = handle_mcp_request(&req) {
                    let resp_str = serde_json::to_string(&resp)?;
                    writeln!(writer, "{}", resp_str)?;
                    writer.flush()?;
                }
            }
            Err(e) => {
                eprintln!("[mcp] JSON parse error: {}", e);
                let err_resp = json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {}", e)
                    }
                });
                writeln!(writer, "{}", serde_json::to_string(&err_resp)?)?;
                writer.flush()?;
            }
        }
        line.clear();
    }

    eprintln!("[mcp] tokenectomy-bot MCP server stopped.");
    Ok(())
}

fn handle_mcp_request(req: &Value) -> Option<Value> {
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");

    match method {
        "initialize" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "tokenectomy-bot",
                    "version": "0.1.0"
                }
            }
        })),

        "notifications/initialized" => None,

        "ping" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {}
        })),

        "tools/list" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "tools": [
                    {
                        "name": "pre_pr_review",
                        "description": "Pre-PR verification gate for AI coding agents. Inspects local Git diff against target branch for tampering (TB0xx), reliability hazards (TB1xx), and security vulnerabilities (TB2xx) before opening a Pull Request.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "base": {
                                    "type": "string",
                                    "description": "Target base git branch/ref (default: 'origin/main' with fallback to 'main')",
                                    "default": "origin/main"
                                },
                                "repo_dir": {
                                    "type": "string",
                                    "description": "Path to local repository root (default: current working directory)",
                                    "default": "."
                                },
                                "agent_pr": {
                                    "type": "boolean",
                                    "description": "Enforce strict Agent PR verification mode (upgrades TB001-TB005 anti-tampering rules to error)",
                                    "default": true
                                },
                                "fail_on": {
                                    "type": "string",
                                    "enum": ["error", "warn", "info"],
                                    "description": "Gate failure threshold (default: 'error')",
                                    "default": "error"
                                }
                            }
                        }
                    },
                    {
                        "name": "audit_diff",
                        "description": "Zero-LLM deterministic AST code audit for unified diff text. Parses diff hunks, reconstructs synthetic source, and detects tampering, reliability, and security hazards without requiring git repository access.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "diff": {
                                    "type": "string",
                                    "description": "Raw unified diff text (e.g. from 'git diff')"
                                },
                                "agent_pr": {
                                    "type": "boolean",
                                    "description": "Enforce strict Agent PR verification mode",
                                    "default": true
                                },
                                "fail_on": {
                                    "type": "string",
                                    "enum": ["error", "warn", "info"],
                                    "description": "Gate failure threshold",
                                    "default": "error"
                                }
                            },
                            "required": ["diff"]
                        }
                    }
                ]
            }
        })),

        "tools/call" => {
            let params = req.get("params");
            let tool_name = params
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let args = params
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or(json!({}));

            let result = match tool_name {
                "pre_pr_review" => handle_pre_pr_review(&args),
                "audit_diff" => handle_audit_diff(&args),
                unknown => Err(anyhow::anyhow!("Unknown tool: {}", unknown)),
            };

            match result {
                Ok((text, is_error)) => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [
                            {
                                "type": "text",
                                "text": text
                            }
                        ],
                        "isError": is_error
                    }
                })),
                Err(err) => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [
                            {
                                "type": "text",
                                "text": format!("Error executing {}: {:#}", tool_name, err)
                            }
                        ],
                        "isError": true
                    }
                })),
            }
        }

        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": format!("Method not found: {}", method)
            }
        })),
    }
}

fn handle_audit_diff(args: &Value) -> Result<(String, bool)> {
    let diff_text = args
        .get("diff")
        .and_then(|d| d.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required 'diff' argument"))?;

    let is_agent = args
        .get("agent_pr")
        .and_then(|a| a.as_bool())
        .unwrap_or(true);
    let fail_on_str = args
        .get("fail_on")
        .and_then(|f| f.as_str())
        .unwrap_or("error");
    let fail_on = match fail_on_str {
        "warn" | "warning" => FailOn::Warn,
        "none" | "info" => FailOn::None,
        _ => FailOn::Error,
    };

    let diff = DiffParser::parse(diff_text);
    let mut new_sources = HashMap::new();
    for file in diff.scannable_files() {
        if let Some(synthetic) = file.reconstruct_synthetic_new_source() {
            new_sources.insert(file.path.clone(), synthetic);
        }
    }

    let engine = RuleEngine::new().with_agent_mode(is_agent);
    let findings = engine.run(&diff, &HashMap::new(), &new_sources);
    let exit_code = GatePolicy::evaluate_exit_code(&findings, fail_on);

    let summary = StickySummaryReporter::format(&findings, diff.files.len(), diff.files.len());
    let detail = TextReporter::format(&findings);

    let formatted = format!(
        "{}\n\n### Detailed Diagnostics:\n```text\n{}\n```",
        summary, detail
    );
    Ok((formatted, exit_code != 0))
}

fn handle_pre_pr_review(args: &Value) -> Result<(String, bool)> {
    let repo_dir_str = args.get("repo_dir").and_then(|r| r.as_str()).unwrap_or(".");
    let repo_dir = PathBuf::from(repo_dir_str);
    let mut base = args
        .get("base")
        .and_then(|b| b.as_str())
        .unwrap_or("origin/main")
        .to_string();
    let is_agent = args
        .get("agent_pr")
        .and_then(|a| a.as_bool())
        .unwrap_or(true);
    let fail_on_str = args
        .get("fail_on")
        .and_then(|f| f.as_str())
        .unwrap_or("error");
    let fail_on = match fail_on_str {
        "warn" | "warning" => FailOn::Warn,
        "none" | "info" => FailOn::None,
        _ => FailOn::Error,
    };

    // Auto-detect base fallback if origin/main doesn't exist
    let diff_res = GitExtractor::extract_diff(&repo_dir, &base, "HEAD").or_else(|_| {
        if base == "origin/main" {
            base = "main".to_string();
            GitExtractor::extract_diff(&repo_dir, "main", "HEAD")
        } else {
            Err(anyhow::anyhow!("Failed to extract diff for {}", base))
        }
    });

    let diff = diff_res?;
    let mut old_map = HashMap::new();
    let mut new_map = HashMap::new();

    for file in diff.scannable_files() {
        let old_lookup = file.old_path.as_ref().unwrap_or(&file.path);
        let old_c = GitExtractor::get_blob(&repo_dir, &base, old_lookup)
            .ok()
            .flatten()
            .or_else(|| file.reconstruct_synthetic_old_source());
        if let Some(c) = old_c {
            old_map.insert(file.path.clone(), c);
        }

        let p = repo_dir.join(&file.path);
        let new_c = if p.exists() {
            fs::read_to_string(&p).ok()
        } else {
            GitExtractor::get_blob(&repo_dir, "HEAD", &file.path)
                .ok()
                .flatten()
        }
        .or_else(|| file.reconstruct_synthetic_new_source());

        if let Some(c) = new_c {
            new_map.insert(file.path.clone(), c);
        }
    }

    // Load config if present
    let mut engine = RuleEngine::new().with_agent_mode(is_agent);
    let config_path = repo_dir.join("tokenectomy.json");
    if config_path.exists()
        && let Ok(config) = Config::load_from_file(&config_path)
    {
        engine = engine.with_config(config);
    }

    // Load baseline if present
    let baseline_path = repo_dir.join(".tokenectomy-baseline.json");
    if baseline_path.exists()
        && let Ok(content) = fs::read_to_string(baseline_path)
    {
        let mut set = std::collections::HashSet::new();
        if let Ok(list) = serde_json::from_str::<Vec<String>>(&content) {
            set.extend(list);
        }
        engine = engine.with_baseline(set);
    }

    let findings = engine.run(&diff, &old_map, &new_map);
    let exit_code = GatePolicy::evaluate_exit_code(&findings, fail_on);

    let summary = StickySummaryReporter::format(&findings, diff.files.len(), diff.files.len());
    let detail = TextReporter::format(&findings);

    let formatted = format!(
        "{}\n\n### Detailed Diagnostics:\n```text\n{}\n```",
        summary, detail
    );
    Ok((formatted, exit_code != 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_initialize() {
        let req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });

        let resp = handle_mcp_request(&req).expect("should have response");
        assert_eq!(resp["result"]["serverInfo"]["name"], "tokenectomy-bot");
        assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
    }

    #[test]
    fn test_mcp_tools_list() {
        let req = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        });

        let resp = handle_mcp_request(&req).expect("should have response");
        let tools = resp["result"]["tools"].as_array().expect("tools array");
        assert_eq!(tools.len(), 2);
        assert!(tools.iter().any(|t| t["name"] == "pre_pr_review"));
        assert!(tools.iter().any(|t| t["name"] == "audit_diff"));
    }

    #[test]
    fn test_mcp_audit_diff_tool_call() {
        let diff_content = r#"diff --git a/auth.test.ts b/auth.test.ts
--- a/auth.test.ts
+++ b/auth.test.ts
@@ -1,3 +1,3 @@
-describe("Login", () => {
+describe.skip("Login", () => {
   it("works", () => {});
 });
"#;

        let req = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "audit_diff",
                "arguments": {
                    "diff": diff_content,
                    "agent_pr": true
                }
            }
        });

        let resp = handle_mcp_request(&req).expect("should have response");
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("TB002"));
    }
}
