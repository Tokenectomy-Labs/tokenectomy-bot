# tokenectomy-bot 🗡️

> **Gerbang verifikasi Pull Request deterministik berbasis AST di era AI coding agent.**

Menangkap manipulasi test oleh AI agent, *silent catch*, tautologi assertion, dan celah reliabilitas/keamanan sebelum review manusia.

---

## Fitur Utama

- ⚡ **Zero-LLM Core**: Keputusan *pass/fail* 100% deterministik dan bebas halusinasi berbasis Tree-sitter AST traversal dalam hitungan milidetik.
- 🛡️ **Anti-Tampering Gate (TB0xx)**:
  - `TB001`: **assertion-removed** — assertion test berkurang bersih pada test suite yang masih aktif.
  - `TB002`: **test-disabled** — pendeteksian `.skip`, `xit`, `it.todo`, `test.fixme` pada baris yang dimodifikasi.
  - `TB003`: **tautological-assertion** — assertion palsu seperti `expect(true).toBe(true)` atau variabel yang membandingkan dirinya sendiri.
- 🔍 **Reliabilitas (TB1xx)**:
  - `TB101`: **silent-catch** — blok `catch` kosong atau `.catch(() => {})` yang menelan error tanpa penanganan/logging.
- 📦 **Multi-format Reporter**:
  - `text`: Terminal output berwarna dengan ringkasan dan petunjuk perbaikan (*fix hint*).
  - `json`: Keluaran terstruktur untuk integrasi M2M / MCP.
  - `sarif`: Standar SARIF 2.1.0 untuk integrasi GitHub Code Scanning.
  - `github`: Anotasi alur kerja bawaan GitHub Actions (`::error file=...::`).
- 🛑 **Exit Codes**:
  - `0`: Bersih / Lolos verifikasi.
  - `1`: Diblokir oleh temuan (terdapat pelanggaran severity error/warn).
  - `2`: Galat internal (tersedia opsi `--fail-open` agar tidak memblokir merge saat terjadi kendala infrastruktur).

---

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
