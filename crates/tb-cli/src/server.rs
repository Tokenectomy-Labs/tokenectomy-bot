use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use colored::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tb_report::{AuditLedger, ConversationEngine, GateStatus};
use tb_rules::RuleEngine;

/// Verify GitHub X-Hub-Signature-256 HMAC
pub fn verify_github_signature(secret: &[u8], body: &[u8], signature_header: &str) -> bool {
    let sig_hex = signature_header
        .strip_prefix("sha256=")
        .unwrap_or(signature_header)
        .trim();
    let expected_hex = compute_hmac_sha256(secret, body);

    if sig_hex.len() != expected_hex.len() {
        return false;
    }

    // Constant-time byte comparison
    let mut diff = 0u8;
    for (a, b) in sig_hex.bytes().zip(expected_hex.bytes()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// RFC 2104 compliant HMAC-SHA256
pub fn compute_hmac_sha256(key: &[u8], data: &[u8]) -> String {
    let block_size = 64;
    let mut k = [0u8; 64];
    if key.len() > block_size {
        let mut hasher = Sha256::new();
        hasher.update(key);
        let digest = hasher.finalize();
        k[..32].copy_from_slice(&digest);
    } else {
        k[..key.len()].copy_from_slice(key);
    }

    let mut k_ipad = [0u8; 64];
    let mut k_opad = [0u8; 64];
    for i in 0..64 {
        k_ipad[i] = k[i] ^ 0x36;
        k_opad[i] = k[i] ^ 0x5c;
    }

    let mut inner = Sha256::new();
    inner.update(k_ipad);
    inner.update(data);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(k_opad);
    outer.update(inner_hash);
    let outer_hash = outer.finalize();

    let mut hex = String::with_capacity(64);
    for b in outer_hash {
        use std::fmt::Write;
        let _ = write!(hex, "{:02x}", b);
    }
    hex
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookServerConfig {
    pub port: u16,
    pub secret: Option<String>,
    pub ledger_path: Option<PathBuf>,
    pub org_policy: Option<String>,
    pub custom_rules_dir: Option<PathBuf>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookResponse {
    pub status: String,
    pub message: String,
    pub pr_number: Option<u64>,
    pub repository: Option<String>,
    pub findings_count: Option<usize>,
    pub gate_passed: Option<bool>,
}

pub struct WebhookServer {
    config: WebhookServerConfig,
    running: Arc<AtomicBool>,
}

impl WebhookServer {
    pub fn new(config: WebhookServerConfig) -> Self {
        Self {
            config,
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    #[allow(dead_code)]
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn run(&self) -> Result<()> {
        let addr = format!("0.0.0.0:{}", self.config.port);
        let listener = TcpListener::bind(&addr)
            .with_context(|| format!("Failed to bind Webhook HTTP server on {}", addr))?;

        println!(
            "{} Tokenectomy GitHub App Webhook server listening on http://{}",
            "✔".green().bold(),
            addr.cyan()
        );
        if self.config.secret.is_some() {
            println!("  HMAC-SHA256 signature verification: {}", "ACTIVE".green());
        } else {
            println!(
                "  HMAC-SHA256 signature verification: {}",
                "DISABLED (insecure)".yellow()
            );
        }

        // Set non-blocking or timeout to allow clean shutdown checks
        listener
            .set_nonblocking(false)
            .context("Failed to configure listener socket")?;

        for stream in listener.incoming() {
            if !self.running.load(Ordering::SeqCst) {
                break;
            }

            match stream {
                Ok(mut socket) => {
                    let _ = socket.set_read_timeout(Some(Duration::from_secs(5)));
                    let _ = socket.set_write_timeout(Some(Duration::from_secs(5)));
                    if let Err(e) = self.handle_connection(&mut socket) {
                        eprintln!("{}: Connection error: {:#}", "Warning".yellow(), e);
                    }
                }
                Err(e) => {
                    eprintln!("{}: Socket accept error: {}", "Warning".yellow(), e);
                }
            }
        }

        println!("Webhook server stopped cleanly.");
        Ok(())
    }

    pub fn handle_connection(&self, stream: &mut TcpStream) -> Result<()> {
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut request_line = String::new();
        reader.read_line(&mut request_line)?;

        let parts: Vec<&str> = request_line.split_whitespace().collect();
        if parts.len() < 2 {
            self.send_response(
                stream,
                400,
                "Bad Request",
                &serde_json::json!({"error": "Bad request"}),
            )?;
            return Ok(());
        }

        let method = parts[0];
        let path = parts[1];

        // Parse headers
        let mut content_length = 0usize;
        let mut signature = None;
        let mut github_event = None;

        loop {
            let mut header_line = String::new();
            let bytes_read = reader.read_line(&mut header_line)?;
            if bytes_read == 0 || header_line == "\r\n" || header_line == "\n" {
                break;
            }

            let trimmed = header_line.trim();
            if let Some((k, v)) = trimmed.split_once(':') {
                let key = k.trim().to_lowercase();
                let val = v.trim();
                if key == "content-length" {
                    content_length = val.parse().unwrap_or(0);
                } else if key == "x-hub-signature-256" {
                    signature = Some(val.to_string());
                } else if key == "x-github-event" {
                    github_event = Some(val.to_string());
                }
            }
        }

        // Endpoint routing
        if method == "GET" && path == "/health" {
            self.send_response(
                stream,
                200,
                "OK",
                &serde_json::json!({
                    "status": "healthy",
                    "version": env!("CARGO_PKG_VERSION"),
                    "service": "tokenectomy-bot"
                }),
            )?;
            return Ok(());
        }

        if method == "POST" && (path == "/webhook" || path == "/") {
            let mut body = vec![0u8; content_length];
            if content_length > 0 {
                reader.read_exact(&mut body)?;
            }

            // HMAC validation
            if let Some(ref secret) = self.config.secret {
                let sig = signature.as_deref().unwrap_or("");
                if !verify_github_signature(secret.as_bytes(), &body, sig) {
                    self.send_response(
                        stream,
                        401,
                        "Unauthorized",
                        &serde_json::json!({
                            "error": "Invalid HMAC-SHA256 signature",
                            "hint": "Check X-Hub-Signature-256 header and webhook secret"
                        }),
                    )?;
                    return Ok(());
                }
            }

            // Handle event
            let event_name = github_event.as_deref().unwrap_or("pull_request");
            let response = self.process_github_payload(event_name, &body)?;

            self.send_response(stream, 200, "OK", &response)?;
            return Ok(());
        }

        self.send_response(
            stream,
            404,
            "Not Found",
            &serde_json::json!({"error": "Endpoint not found"}),
        )?;
        Ok(())
    }

    fn process_github_payload(&self, event: &str, body: &[u8]) -> Result<serde_json::Value> {
        let payload: serde_json::Value = match serde_json::from_slice(body) {
            Ok(v) => v,
            Err(_) => {
                return Ok(serde_json::json!({
                    "status": "ignored",
                    "message": "Payload was not valid JSON"
                }));
            }
        };

        if event == "ping" {
            return Ok(serde_json::json!({
                "status": "pong",
                "zen": payload.get("zen").and_then(|z| z.as_str()).unwrap_or("Keep it deterministic")
            }));
        }

        if event == "pull_request" {
            let action = payload
                .get("action")
                .and_then(|a| a.as_str())
                .unwrap_or("unknown");

            if action != "opened" && action != "synchronize" && action != "reopened" {
                return Ok(serde_json::json!({
                    "status": "ignored",
                    "action": action,
                    "message": "Only pull_request opened, synchronize, and reopened actions are evaluated"
                }));
            }

            let pr_number = payload
                .get("pull_request")
                .and_then(|pr| pr.get("number"))
                .and_then(|n| n.as_u64());

            let repo_full_name = payload
                .get("repository")
                .and_then(|r| r.get("full_name"))
                .and_then(|n| n.as_str())
                .unwrap_or("unknown/repo");

            let head_sha = payload
                .get("pull_request")
                .and_then(|pr| pr.get("head"))
                .and_then(|h| h.get("sha"))
                .and_then(|s| s.as_str())
                .unwrap_or("HEAD");

            let base_ref = payload
                .get("pull_request")
                .and_then(|pr| pr.get("base"))
                .and_then(|b| b.get("ref"))
                .and_then(|r| r.as_str())
                .unwrap_or("main");

            // If a direct diff is embedded in payload (e.g. testing or proxy)
            let mut findings_count = 0;
            let mut gate_passed = true;

            if let Some(diff_str) = payload.get("diff").and_then(|d| d.as_str()) {
                let parsed_diff = tb_diff::DiffParser::parse(diff_str);
                let mut engine = RuleEngine::new();

                // Load custom rules if directory specified
                if let Some(ref custom_dir) = self.config.custom_rules_dir
                    && let Ok(custom_rules) = tb_rules::CustomRule::load_dir(custom_dir)
                {
                    engine = engine.with_custom_rules(custom_rules);
                }

                let findings = engine.run(
                    &parsed_diff,
                    &std::collections::HashMap::new(),
                    &std::collections::HashMap::new(),
                );
                findings_count = findings.len();
                gate_passed = findings
                    .iter()
                    .all(|f| f.severity != tb_rules::Severity::Error);

                // If ledger path configured, record to sealed ledger
                if let Some(ref ledger_path) = self.config.ledger_path {
                    let timestamp = chrono_or_simple_timestamp();
                    if let Ok(entry) = AuditLedger::build_entry(
                        ledger_path,
                        repo_full_name,
                        base_ref,
                        head_sha,
                        parsed_diff.files.len(),
                        &findings,
                        gate_passed,
                        &timestamp,
                    ) {
                        let _ = AuditLedger::append(ledger_path, &entry);
                    }
                }
            }

            return Ok(serde_json::json!({
                "status": "processed",
                "repository": repo_full_name,
                "pr_number": pr_number,
                "head_sha": head_sha,
                "action": action,
                "findings_count": findings_count,
                "gate_passed": gate_passed,
            }));
        }

        if event == "issue_comment" || event == "pull_request_review_comment" {
            let action = payload
                .get("action")
                .and_then(|a| a.as_str())
                .unwrap_or("unknown");

            if action != "created" {
                return Ok(serde_json::json!({
                    "status": "ignored",
                    "action": action,
                    "message": "Only created comment actions are evaluated"
                }));
            }

            let is_pr = payload
                .get("issue")
                .and_then(|i| i.get("pull_request"))
                .is_some()
                || event == "pull_request_review_comment";

            if !is_pr {
                return Ok(serde_json::json!({
                    "status": "ignored",
                    "message": "Comment is not associated with a Pull Request"
                }));
            }

            let comment_body = payload
                .get("comment")
                .and_then(|c| c.get("body"))
                .and_then(|b| b.as_str())
                .unwrap_or("");

            let author_login = payload
                .get("comment")
                .and_then(|c| c.get("user"))
                .and_then(|u| u.get("login"))
                .and_then(|l| l.as_str())
                .unwrap_or("anonymous");

            let pr_number = if event == "pull_request_review_comment" {
                payload
                    .get("pull_request")
                    .and_then(|pr| pr.get("number"))
                    .and_then(|n| n.as_u64())
                    .unwrap_or(0)
            } else {
                payload
                    .get("issue")
                    .and_then(|i| i.get("number"))
                    .and_then(|n| n.as_u64())
                    .unwrap_or(0)
            };

            let repo_full_name = payload
                .get("repository")
                .and_then(|r| r.get("full_name"))
                .and_then(|n| n.as_str())
                .unwrap_or("unknown/repo");

            let sender = ConversationEngine::parse_sender(author_login);

            if !ConversationEngine::should_respond(&sender, comment_body) {
                return Ok(serde_json::json!({
                    "status": "ignored",
                    "author": author_login,
                    "message": "Comment does not trigger Tmy-Joy response"
                }));
            }

            let gate_status = GateStatus::Passed;
            let response_text = ConversationEngine::generate_response(
                &sender,
                comment_body,
                &gate_status,
                repo_full_name,
                pr_number,
            );

            return Ok(serde_json::json!({
                "status": "replied",
                "repository": repo_full_name,
                "pr_number": pr_number,
                "author": author_login,
                "is_bot": sender.is_bot(),
                "response": response_text,
            }));
        }

        Ok(serde_json::json!({
            "status": "ignored",
            "event": event,
            "message": "Event is not handled by Tokenectomy PR Gate"
        }))
    }

    fn send_response(
        &self,
        stream: &mut TcpStream,
        status_code: u16,
        status_text: &str,
        json_body: &serde_json::Value,
    ) -> Result<()> {
        let body_str = serde_json::to_string_pretty(json_body)?;
        let response = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            status_code,
            status_text,
            body_str.len(),
            body_str
        );
        stream.write_all(response.as_bytes())?;
        stream.flush()?;
        Ok(())
    }
}

pub fn chrono_or_simple_timestamp() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}-epoch", now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hmac_sha256_rfc2104_vector() {
        let key = b"key";
        let data = b"The quick brown fox jumps over the lazy dog";
        let hmac = compute_hmac_sha256(key, data);
        assert_eq!(
            hmac,
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    #[test]
    fn test_verify_github_signature() {
        let secret = b"my_super_secret_webhook_token";
        let payload = b"{\"action\": \"opened\", \"number\": 12}";
        let hmac = compute_hmac_sha256(secret, payload);
        let sig_header = format!("sha256={}", hmac);

        assert!(verify_github_signature(secret, payload, &sig_header));
        assert!(!verify_github_signature(
            b"wrong_secret",
            payload,
            &sig_header
        ));
        assert!(!verify_github_signature(
            secret,
            b"altered payload",
            &sig_header
        ));
    }

    #[test]
    fn test_webhook_server_http_health_and_post() {
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let secret = "test_webhook_secret_key".to_string();
        let config = WebhookServerConfig {
            port,
            secret: Some(secret.clone()),
            ledger_path: None,
            org_policy: None,
            custom_rules_dir: None,
        };

        let server = Arc::new(WebhookServer::new(config));
        let server_clone = server.clone();
        let handle = thread::spawn(move || {
            let _ = server_clone.run();
        });

        thread::sleep(Duration::from_millis(50));

        // 1. Test GET /health
        {
            let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
            stream
                .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n")
                .unwrap();
            let mut res = String::new();
            let mut reader = BufReader::new(stream);
            reader.read_line(&mut res).unwrap();
            assert!(res.contains("200 OK"));
        }

        // 2. Test POST /webhook with valid HMAC
        {
            let payload = serde_json::json!({
                "action": "opened",
                "pull_request": { "number": 7, "head": { "sha": "abc1234" }, "base": { "ref": "main" } },
                "repository": { "full_name": "Org/TestRepo" }
            });
            let payload_str = serde_json::to_string(&payload).unwrap();
            let hmac = compute_hmac_sha256(secret.as_bytes(), payload_str.as_bytes());

            let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
            let req = format!(
                "POST /webhook HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nX-Hub-Signature-256: sha256={}\r\nX-GitHub-Event: pull_request\r\n\r\n{}",
                payload_str.len(),
                hmac,
                payload_str
            );
            stream.write_all(req.as_bytes()).unwrap();
            let mut res = String::new();
            let mut reader = BufReader::new(stream);
            reader.read_line(&mut res).unwrap();
            assert!(res.contains("200 OK"));
        }

        // 3. Test POST /webhook with invalid HMAC (should return 401)
        {
            let payload_str = "{}";
            let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
            let req = format!(
                "POST /webhook HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nX-Hub-Signature-256: sha256=invalid\r\nX-GitHub-Event: pull_request\r\n\r\n{}",
                payload_str.len(),
                payload_str
            );
            stream.write_all(req.as_bytes()).unwrap();
            let mut res = String::new();
            let mut reader = BufReader::new(stream);
            reader.read_line(&mut res).unwrap();
            assert!(res.contains("401 Unauthorized"));
        }

        // 4. Test POST /webhook with issue_comment from dependabot (M2M Handover)
        {
            let comment_payload = serde_json::json!({
                "action": "created",
                "issue": { "number": 42, "pull_request": {} },
                "comment": {
                    "body": "Bumps tokio from 1.0 to 1.1",
                    "user": { "login": "dependabot[bot]" }
                },
                "repository": { "full_name": "Acme/Web" }
            });
            let payload_str = serde_json::to_string(&comment_payload).unwrap();
            let hmac = compute_hmac_sha256(secret.as_bytes(), payload_str.as_bytes());

            let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
            let req = format!(
                "POST /webhook HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nX-Hub-Signature-256: sha256={}\r\nX-GitHub-Event: issue_comment\r\n\r\n{}",
                payload_str.len(),
                hmac,
                payload_str
            );
            stream.write_all(req.as_bytes()).unwrap();
            let mut res = String::new();
            let mut reader = BufReader::new(stream);
            reader.read_line(&mut res).unwrap();
            assert!(res.contains("200 OK"));
        }

        server.stop();
        let _ = TcpStream::connect(format!("127.0.0.1:{}", port));
        let _ = handle.join();
    }
}
