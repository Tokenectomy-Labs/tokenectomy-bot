# tokenectomy-bot (Tmy-Joy) 🗡️

[![CI](https://github.com/Tokenectomy-Labs/tokenctomy-bot/actions/workflows/ci.yml/badge.svg)](https://github.com/Tokenectomy-Labs/tokenctomy-bot/actions/workflows/ci.yml)
[![Precision Gate](https://img.shields.io/badge/Precision_Gate-100.00%25-brightgreen?logo=rust)](https://github.com/Tokenectomy-Labs/tokenctomy-bot)
[![Latency](https://img.shields.io/badge/Audit_Latency-0.8ms%2Ffile-blue?logo=speedtest)](https://github.com/Tokenectomy-Labs/tokenctomy-bot)
[![Zero-LLM](https://img.shields.io/badge/Cost-$0_(Zero--LLM)-orange)](https://github.com/Tokenectomy-Labs/tokenctomy-bot)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

> **Deterministic, sub-millisecond AST Pull Request verification gate in the AI coding agent era.**
> Catches test tampering, fake assertions, hallucinated phantom modules, and security vulnerabilities before human review.

---

## ⚡ Try the Live Adversarial Demo

Experience Tmy-Joy intercepting an AI coding agent (Cursor/Devin) attempting to sneak muted tests and hallucinated code past CI:

```bash
cargo run --release -p tb-cli -- demo
```

```text
================================================================================
   🗡️  TOKENECTOMY-BOT (TMY-JOY) — LIVE ADVERSARIAL PR AUDIT DEMO
   Scenario: AI Coding Agent (Cursor / Devin / Copilot) submits PR #42
================================================================================

🤖 [AI CODING AGENT CLAIM]: "All tests passing! 100% CI green. Ready to merge! 🚀"
⚡ AST Audit completed in 0.82 ms! (Throughput: ~1,210 files/sec)

🚨 [TMY-JOY PR QUALITY GATE VERDICT]: ❌ BLOCKED (3 Errors, 1 Warning)

📋 [AUDIT FINDINGS & CAUGHT CHEATING TRICKS]:
   1. 🔴 BLOCKED `TB301` (phantom-symbol)
      Trick   : Phantom module detected (`./uncreated_gateway`). AI agent hallucinated uncreated file.
   2. 🔴 BLOCKED `TB002` (test-disabled)
      Trick   : Test was disabled using: `it.skip('handles network timeout')`
   3. 🔴 BLOCKED `TB003` (tautological-assertion)
      Trick   : Tautological assertion detected: `expect(true).toBe(true)` always evaluates to true.
   4. 🟡 WARNING `TB101` (silent-catch)
      Trick   : Empty catch block swallows fatal production exceptions.
      Auto-Fix: ✔ Available (1-click GitHub PR suggestion)

🗺️  [BLAST RADIUS & SENSITIVE PATH IMPACT]:
   Risk Score  : 43/100 (Medium) — Touched sensitive domain `src/billing/checkout.ts`

🔒 [CRYPTOGRAPHIC AUDIT LEDGER SEAL]:
   Hash : b57adbebfaa9980ed7b7576d5156a5c7a7d273e2d3288b3ab0dbbd7ad0d2cd07
   Proof: Immutable record sealed. PR cannot be merged into main.
```

---

## Fitur Utama

- ⚡ **Zero-LLM Core**: Keputusan *pass/fail* 100% deterministik dan bebas halusinasi berbasis Tree-sitter AST traversal dalam hitungan milidetik (~0.8 ms per berkas).
- 🛡️ **Anti-Tampering Gate (TB0xx)**:
  - `TB001`: **assertion-removed** — assertion test berkurang bersih pada test suite yang masih aktif.
  - `TB002`: **test-disabled** — pendeteksian `.skip`, `xit`, `it.todo`, `test.fixme`, `#[ignore]`.
  - `TB003`: **tautological-assertion** — assertion palsu seperti `expect(true).toBe(true)` atau variabel yang membandingkan dirinya sendiri.
  - `TB004`–`TB009`: **config-weakened**, **early-exit-injected**, **lazy-deletion** (`todo!()`), **domain-narrowing**, **fixture-snooping**.
- 👻 **Hallucination Buster (`TB301: phantom-symbol`)**:
  - Cross-file AST symbol resolver yang memverifikasi apakah impor relatif (`./`, `../`) benar-benar ada dan simbolnya benar-benar diekspor oleh berkas target.
- 🗺️ **Blast Radius & Architecture Visualizer**:
  - Pemetaan graph dependensi 2-tingkat (Direct Callers: depth 1, Indirect Callers: depth 2) melintasi repositori dalam waktu < 5 ms.
  - Penilaian skor risiko kuantitatif (0–100) dan diagram alur visual Mermaid `graph TD` otomatis dirender di PR Summary Sticky Comment.
- 🛠️ **Deterministic Auto-Fix Engine**:
  - Menerapkan perbaikan AST secara instan (`--fix`) tanpa drift nomor baris.
  - Menghasilkan blok ````suggestion```` interaktif di GitHub Review sehingga reviewer/author dapat menerapkan perbaikan dengan 1 klik.
- 🤖 **Autonomous M2M Bot Dialogue**:
  - Berinteraksi otonom dengan Dependabot dan Renovate: otomatis menerbitkan `@dependabot squash and merge` jika bersih, atau `@dependabot recreate` jika rusak.
- 🔒 **Cryptographic SHA-256 Audit Ledger**:
  - Menjaga jejak audit rantai hash anti-pemalsuan untuk kebutuhan audit kepatuhan dan keamanan enterprise.

## Instalasi & Penggunaan

### Inisialisasi Konfigurasi
```bash
tokenectomy-bot init
```
Membuat berkas `tokenectomy.json` default.

### Menjalankan Review Lokal
```bash
# Review terhadap base branch
tokenectomy-bot review --base origin/main --head HEAD

# Review berkas diff langsung
tokenectomy-bot review --diff-file my_change.diff --format text

# Integrasi CI dengan GitHub Annotations
tokenectomy-bot review --base origin/main --format github --fail-on error
```

### Model Context Protocol (MCP) Server for AI Agents
Run as a background MCP stdio server to enable autonomous pre-PR audits in Claude Desktop, Cursor, Antigravity, and Cline:
```bash
tokenectomy-bot mcp
```

### Custom Tree-sitter SCM Rules
Write domain-specific rules as Tree-sitter `.scm` queries in `.tokenectomy/rules/`:
```scheme
;; @id: TB_CUSTOM_001
;; @name: no-hardcoded-secret
;; @severity: error
;; @message: Potential hardcoded secret or token assignment detected in source code
;; @fix_hint: Move credentials to secure environment variables
;; @languages: typescript, javascript

(variable_declarator
  name: (identifier) @id (#match? @id "(?i)(api_?key|secret|token|password)")
  value: (string) @val) @match
```

Validate and test queries against test fixtures:
```bash
tokenectomy-bot rules validate .tokenectomy/rules/no_hardcoded_secrets.scm --fixture src/tests/fixture.ts
```

### Cryptographic SHA-256 Sealed Audit Ledger
Maintain immutable compliance proof for every PR audit:
```bash
# Record sealed entry upon review
tokenectomy-bot review --base origin/main --ledger .tokenectomy/audit-ledger.jsonl

# Verify cryptographic hash chain integrity (detects historical tampering)
tokenectomy-bot ledger verify .tokenectomy/audit-ledger.jsonl

# View executive metrics and compliance dashboard
tokenectomy-bot ledger metrics .tokenectomy/audit-ledger.jsonl
```

### Organization-Wide Policy Enforcement
Enforce uncompromisable rule baselines across repositories:
```bash
tokenectomy-bot review --org-policy /etc/tokenectomy/org-policy.json
```
Repos cannot downgrade or disable mandated rules without explicit approved exception (`--allow-policy-downgrade`).

### GitHub App Webhook Server
Run as an autonomous organization-level GitHub webhook listener without per-repo workflows:
```bash
tokenectomy-bot serve --port 8080 --secret "$GITHUB_WEBHOOK_SECRET" --ledger /var/log/audit.jsonl
```

### Multi-Channel Webhook Notifications
Notify engineering and security teams upon PR evaluation:
```bash
tokenectomy-bot review --base origin/main --webhook-url "https://hooks.slack.com/services/xxx"
```
Supports Slack, Discord, and Microsoft Teams.

### Autonomous PR Discussion & M2M Bot-to-Bot Dialogue (Tmy-Joy)
Tmy-Joy interacts directly in GitHub PR discussions, conversational threads, and machine-to-machine handshakes with other GitHub bots:
```bash
# Answer PR comments or questions locally or via GitHub Actions
tokenectomy-bot chat --author "dependabot[bot]" --comment "Bumps lodash" --gate-status passed
tokenectomy-bot chat --author "alice" --comment "/explain TB001"
```

#### Supported Bot Commands & Queries
- `@tmy-joy status` or `/status`: Returns live deterministic gate status, error count, and ledger proof.
- `@tmy-joy explain <RULE_ID>`: Provides detailed rule rationale, security risks, good/bad code examples, and remediation steps.
- `@tmy-joy rules` or `/rules`: Displays full active AST detection rules catalog.
- `@tmy-joy ping`: Health check, response latency, and Tree-sitter engine status.

#### Machine-to-Machine (M2M) Handover Protocol
- **Dependabot (`@dependabot`)**: Evaluates AST integrity. If passed, autonomously issues `@dependabot squash and merge`. If blocked by test tampering or security flaws, issues `@dependabot recreate`.
- **Renovate (`@renovate`)**: Issues `@renovate merge` on pass or `@renovate rebase` on block.
- **CodeRabbit (`@coderabbitai`)**: Correlates AI suggestions with Tree-sitter AST determinism (0% hallucination rate).

### Mekanisme Penekanan (Suppression)
Gunakan komentar inline jika blok sengaja dilewati secara sah:
```typescript
// tokenectomy-ignore: TB101 -- disengaja untuk fallback
try {
  loadOptionalConfig();
} catch (e) {}
```

---

## Performance & Precision Benchmarks

Rigorously audited with a standalone 1,000-case stress test suite (`stress_1k_benchmark`):
* ⚡ **Speed**: Processes 1,000 PR diff files in **~0.70 seconds** (~**1,428+ files/sec**).
* 🎯 **Precision**: **100.00%** precision gate (0 false positives, 0 false negatives).
* 🛡️ **Stability**: Zero panics or crashes across adversarial payloads, unicode emojis, binary markers, and extreme line numbers.
* 🌐 **Languages**: Native Tree-sitter parsing for TypeScript, JavaScript, Rust, Go, and Python.

For complete reproduction instructions and audit tables, see [BENCHMARK.md](BENCHMARK.md).

---

## Arsitektur Workspace

```
tokenectomy-bot/
├── crates/
│   ├── tb-diff/    # Ekstraktor dan parser Unified Git Diff 2-sisi
│   ├── tb-parse/   # Parser Tree-sitter & pemetaan semantic range overlap
│   ├── tb-rules/   # Mesin aturan, model Finding, dan fingerprint stabil
│   ├── tb-report/  # Reporter Text, JSON, SARIF 2.1.0, & GitHub Annotations
│   └── tb-cli/     # Binary executable CLI (tokenectomy-bot)
├── action.yml      # GitHub Composite Action
└── ROADMAP.md      # Rencana pengembangan v2
```

Lisensi: MIT
