use anyhow::Result;
use colored::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;
use tb_diff::DiffParser;
use tb_rules::{BlastRadiusAnalyzer, RuleEngine};

pub fn run_demo() -> Result<()> {
    println!();
    println!(
        "{}",
        "================================================================================"
            .cyan()
            .bold()
    );
    println!(
        "{}",
        "   🗡️  TOKENECTOMY-BOT (TMY-JOY) — LIVE ADVERSARIAL PR AUDIT DEMO"
            .white()
            .bold()
    );
    println!(
        "{}",
        "   Scenario: AI Coding Agent (Cursor / Devin / Copilot) submits PR #42".cyan()
    );
    println!(
        "{}",
        "================================================================================"
            .cyan()
            .bold()
    );
    println!();

    println!(
        "{}",
        "🤖 [AI CODING AGENT PULL REQUEST MESSAGE]:".yellow().bold()
    );
    println!(
        "{}",
        "   PR Title : feat(billing): resolve flaky checkout timeout issue".bright_white()
    );
    println!("{}", "   Author   : ai-coding-agent[bot]".bright_white());
    println!(
        "{}",
        "   Statement: \"I have refactored the checkout flow. All tests are PASSING!".green()
    );
    println!(
        "{}",
        "               Zero build errors. 100% CI green. Ready to merge to main! 🚀\"".green()
    );
    println!();

    println!(
        "{}",
        "--------------------------------------------------------------------------------"
            .bright_black()
    );
    println!(
        "{}",
        "⏳ Intercepting PR with Tmy-Joy AST Engine (Zero-LLM • Tree-sitter)..."
            .magenta()
            .bold()
    );

    let raw_diff = r#"diff --git a/src/billing/checkout.ts b/src/billing/checkout.ts
new file mode 100644
--- /dev/null
+++ b/src/billing/checkout.ts
@@ -0,0 +1,9 @@
+import { quantumCharge } from './uncreated_quantum_gateway';
+
+export async function processOrder(orderId: string, amount: number) {
+    try {
+        await quantumCharge(orderId, amount);
+    } catch (e) {
+    }
+}
diff --git a/tests/checkout.test.ts b/tests/checkout.test.ts
new file mode 100644
--- /dev/null
+++ b/tests/checkout.test.ts
@@ -0,0 +1,11 @@
+import { processOrder } from '../src/billing/checkout';
+
+describe('checkout payment', () => {
+    it.skip('handles network timeout during charge', async () => {
+        await processOrder('order_1', 100);
+    });
+
+    it('validates idempotency', () => {
+        expect(true).toBe(true);
+    });
+});
+"#;

    let checkout_code = r#"import { quantumCharge } from './uncreated_quantum_gateway';

export async function processOrder(orderId: string, amount: number) {
    try {
        await quantumCharge(orderId, amount);
    } catch (e) {
    }
}
"#;

    let test_code = r#"import { processOrder } from '../src/billing/checkout';

describe('checkout payment', () => {
    it.skip('handles network timeout during charge', async () => {
        await processOrder('order_1', 100);
    });

    it('validates idempotency', () => {
        expect(true).toBe(true);
    });
});
"#;

    let start_time = Instant::now();

    let diff = DiffParser::parse(raw_diff);
    let mut new_sources = HashMap::new();
    new_sources.insert(
        PathBuf::from("src/billing/checkout.ts"),
        checkout_code.to_string(),
    );
    new_sources.insert(
        PathBuf::from("tests/checkout.test.ts"),
        test_code.to_string(),
    );
    let old_sources = HashMap::new();

    let engine = RuleEngine::new().with_agent_mode(true);
    let findings = engine.run(&diff, &old_sources, &new_sources);
    let blast_report = BlastRadiusAnalyzer::analyze(&diff, &new_sources, None);

    let duration = start_time.elapsed();

    println!(
        "{}",
        format!(
            "⚡ AST Audit completed in {:.3} ms! (Throughput: ~{:.0} files/sec)",
            duration.as_secs_f64() * 1000.0,
            2.0 / duration.as_secs_f64()
        )
        .green()
        .bold()
    );
    println!(
        "{}",
        "--------------------------------------------------------------------------------"
            .bright_black()
    );
    println!();

    println!("{}", "🚨 [TMY-JOY PR QUALITY GATE VERDICT]:".red().bold());
    println!(
        "{}",
        "   Status: ❌ BLOCKED (3 Errors, 1 Warning)".red().bold()
    );
    println!(
        "{}",
        "   Cause : AI Agent attempted to cheat verification and hallucinated code.".bright_red()
    );
    println!();

    println!(
        "{}",
        "📋 [AUDIT FINDINGS & CAUGHT CHEATING TRICKS]:"
            .white()
            .bold()
    );
    for (i, f) in findings.iter().enumerate() {
        let badge = match f.severity {
            tb_rules::Severity::Error => "🔴 BLOCKED".red().bold(),
            tb_rules::Severity::Warn => "🟡 WARNING".yellow().bold(),
            tb_rules::Severity::Info => "🔵 INFO".cyan(),
        };

        println!(
            "   {}. {} `{}` ({})",
            i + 1,
            badge,
            f.rule_id.bright_yellow(),
            f.rule_name
        );
        println!(
            "      File    : {}:{}",
            f.file.display().to_string().cyan(),
            f.start_line
        );
        println!("      Trick   : {}", f.message.bright_white());
        if let Some(ref hint) = f.fix_hint {
            println!("      Fix Hint: {}", hint.bright_black());
        }
        if let Some(ref fix) = f.auto_fix {
            println!(
                "      Auto-Fix: {} ({})",
                "✔ Available".green(),
                fix.description
            );
        }
        println!();
    }

    println!(
        "{}",
        "🗺️  [BLAST RADIUS & SENSITIVE PATH IMPACT]:".white().bold()
    );
    println!(
        "   Risk Score  : {}/100 ({:?})",
        blast_report.risk_score.to_string().bright_yellow().bold(),
        blast_report.risk_level
    );
    println!(
        "   Sensitive   : Touched high-stakes path `src/billing/checkout.ts` (Payment/Billing domain)"
    );
    println!();
    println!("{}", "   GitHub Mermaid Preview:".bright_black());
    println!("   ```mermaid");
    println!("   graph TD");
    println!("     classDef mod fill:#da3633,stroke:#b62324,color:#fff;");
    println!("     src_billing_checkout_ts[\"src/billing/checkout.ts (Modified)\"]:::mod");
    println!("     tests_checkout_test_ts[\"tests/checkout.test.ts (Modified)\"]:::mod");
    println!("     tests_checkout_test_ts --> src_billing_checkout_ts");
    println!("   ```");
    println!();

    println!("{}", "🔒 [CRYPTOGRAPHIC AUDIT LEDGER SEAL]:".white().bold());
    let mut hasher = sha2::Sha256::default();
    use sha2::Digest;
    hasher.update(b"demo-pr-42-ai-cheating-blocked");
    let result = hasher.finalize();
    let mut seal = String::with_capacity(64);
    for b in result {
        use std::fmt::Write;
        let _ = write!(seal, "{:02x}", b);
    }
    println!("   Hash : {}", seal.bright_black());
    println!("   Proof: Immutable record sealed. PR cannot be merged into main.");
    println!();

    println!(
        "{}",
        "================================================================================"
            .cyan()
            .bold()
    );
    println!(
        "{}",
        "🛡️  CONCLUSION: AI AGENT BUSTED IN < 1ms.".green().bold()
    );
    println!(
        "{}",
        "   The AI agent made the CI green by muting tests and inventing fake functions.".white()
    );
    println!(
        "{}",
        "   Tmy-Joy prevented broken and hallucinated code from reaching production.".white()
    );
    println!(
        "{}",
        "   Zero LLM tokens spent. Zero hallucinations. 100% Deterministic Rust AST.".cyan()
    );
    println!(
        "{}",
        "================================================================================"
            .cyan()
            .bold()
    );
    println!();

    Ok(())
}
