use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookType {
    #[default]
    Auto,
    Slack,
    Discord,
    Teams,
}

impl WebhookType {
    pub fn detect_from_url(url: &str) -> Self {
        if url.contains("hooks.slack.com") {
            WebhookType::Slack
        } else if url.contains("discord.com/api/webhooks") {
            WebhookType::Discord
        } else if url.contains("webhook.office.com") || url.contains("office.com/webhook") {
            WebhookType::Teams
        } else {
            WebhookType::Auto
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookNotification {
    pub repo_name: String,
    pub pr_number: Option<u64>,
    pub head_ref: String,
    pub gate_passed: bool,
    pub total_findings: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub tamper_count: usize,
    pub duration_ms: u128,
    pub pr_url: Option<String>,
}

impl WebhookNotification {
    pub fn build_slack_payload(&self) -> serde_json::Value {
        let status_emoji = if self.gate_passed { "✅" } else { "🚨" };
        let status_text = if self.gate_passed {
            "PASSED"
        } else {
            "BLOCKED"
        };
        let pr_title = match (self.pr_number, &self.pr_url) {
            (Some(num), Some(url)) => format!("<{}|PR #{}>", url, num),
            (Some(num), None) => format!("PR #{}", num),
            _ => format!("Ref: {}", self.head_ref),
        };

        let summary = format!(
            "{} *Tokenectomy Gate {}* on `{}` ({})",
            status_emoji, status_text, self.repo_name, pr_title
        );

        let fields = vec![
            serde_json::json!({
                "type": "mrkdwn",
                "text": format!("*Status:*\n{}", status_text)
            }),
            serde_json::json!({
                "type": "mrkdwn",
                "text": format!("*Duration:*\n{} ms", self.duration_ms)
            }),
            serde_json::json!({
                "type": "mrkdwn",
                "text": format!("*Errors:*\n{}", self.error_count)
            }),
            serde_json::json!({
                "type": "mrkdwn",
                "text": format!("*Warnings:*\n{}", self.warning_count)
            }),
            serde_json::json!({
                "type": "mrkdwn",
                "text": format!("*Tamper Blocked:*\n{}", self.tamper_count)
            }),
            serde_json::json!({
                "type": "mrkdwn",
                "text": format!("*Total Findings:*\n{}", self.total_findings)
            }),
        ];

        serde_json::json!({
            "text": summary,
            "blocks": [
                {
                    "type": "header",
                    "text": {
                        "type": "plain_text",
                        "text": format!("Tokenectomy Verification Gate: {}", status_text),
                        "emoji": true
                    }
                },
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": summary
                    }
                },
                {
                    "type": "section",
                    "fields": fields
                }
            ]
        })
    }

    pub fn build_discord_payload(&self) -> serde_json::Value {
        let status_text = if self.gate_passed {
            "PASSED"
        } else {
            "BLOCKED"
        };
        let color = if self.gate_passed { 0x00FF00 } else { 0xFF0000 };

        let description = format!(
            "Verification result for **{}** ({})\n\n**Status:** {}\n**Errors:** {}\n**Warnings:** {}\n**Tampering Blocked:** {}\n**Duration:** {} ms",
            self.repo_name,
            self.head_ref,
            status_text,
            self.error_count,
            self.warning_count,
            self.tamper_count,
            self.duration_ms
        );

        serde_json::json!({
            "embeds": [
                {
                    "title": format!("Tokenectomy Verification: {}", status_text),
                    "description": description,
                    "color": color,
                    "url": self.pr_url.as_deref().unwrap_or(""),
                    "footer": {
                        "text": "Tokenectomy Deterministic PR Gate"
                    }
                }
            ]
        })
    }

    pub fn build_teams_payload(&self) -> serde_json::Value {
        let status_text = if self.gate_passed {
            "PASSED"
        } else {
            "BLOCKED"
        };
        let theme_color = if self.gate_passed { "00FF00" } else { "FF0000" };

        serde_json::json!({
            "@type": "MessageCard",
            "@context": "https://schema.org/extensions",
            "summary": format!("Tokenectomy Gate {}", status_text),
            "themeColor": theme_color,
            "title": format!("Tokenectomy Gate {}: {}", status_text, self.repo_name),
            "sections": [
                {
                    "facts": [
                        { "name": "Head Ref:", "value": &self.head_ref },
                        { "name": "Status:", "value": status_text },
                        { "name": "Errors:", "value": format!("{}", self.error_count) },
                        { "name": "Warnings:", "value": format!("{}", self.warning_count) },
                        { "name": "Tamper Detections:", "value": format!("{}", self.tamper_count) },
                        { "name": "Duration:", "value": format!("{} ms", self.duration_ms) }
                    ]
                }
            ]
        })
    }

    pub fn build_payload(&self, webhook_type: WebhookType) -> serde_json::Value {
        match webhook_type {
            WebhookType::Slack => self.build_slack_payload(),
            WebhookType::Discord => self.build_discord_payload(),
            WebhookType::Teams => self.build_teams_payload(),
            WebhookType::Auto => self.build_slack_payload(),
        }
    }

    pub fn send(&self, webhook_url: &str, webhook_type: Option<WebhookType>) -> Result<()> {
        let w_type = match webhook_type {
            Some(t) if t != WebhookType::Auto => t,
            _ => WebhookType::detect_from_url(webhook_url),
        };

        let payload = self.build_payload(w_type);
        let payload_str = serde_json::to_string(&payload)?;

        let output = Command::new("curl")
            .args([
                "-s",
                "-S",
                "-X",
                "POST",
                "-H",
                "Content-Type: application/json",
                "--max-time",
                "5",
                "-d",
                &payload_str,
                webhook_url,
            ])
            .output()
            .with_context(|| "Failed to execute curl to dispatch webhook notification")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Webhook dispatch failed: {}", stderr);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_url_detection() {
        assert_eq!(
            WebhookType::detect_from_url("https://hooks.slack.com/services/xxx"),
            WebhookType::Slack
        );
        assert_eq!(
            WebhookType::detect_from_url("https://discord.com/api/webhooks/123/abc"),
            WebhookType::Discord
        );
        assert_eq!(
            WebhookType::detect_from_url("https://webhook.office.com/webhookb2/xyz"),
            WebhookType::Teams
        );
        assert_eq!(
            WebhookType::detect_from_url("https://example.com/custom"),
            WebhookType::Auto
        );
    }

    #[test]
    fn test_webhook_payload_generation() {
        let notif = WebhookNotification {
            repo_name: "Tokenectomy-Labs/core".to_string(),
            pr_number: Some(42),
            head_ref: "agent/patch-1".to_string(),
            gate_passed: false,
            total_findings: 3,
            error_count: 2,
            warning_count: 1,
            tamper_count: 2,
            duration_ms: 120,
            pr_url: Some("https://github.com/Tokenectomy-Labs/core/pull/42".to_string()),
        };

        let slack = notif.build_slack_payload();
        assert!(slack["text"].as_str().unwrap().contains("BLOCKED"));
        assert!(slack["text"].as_str().unwrap().contains("PR #42"));

        let discord = notif.build_discord_payload();
        let desc = discord["embeds"][0]["description"].as_str().unwrap();
        assert!(desc.contains("BLOCKED"));
        assert!(desc.contains("Tampering Blocked:** 2"));

        let teams = notif.build_teams_payload();
        assert_eq!(teams["themeColor"], "FF0000");
    }
}
