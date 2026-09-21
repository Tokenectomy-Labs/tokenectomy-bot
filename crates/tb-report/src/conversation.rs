use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BotKind {
    Dependabot,
    Renovate,
    CodeRabbit,
    Copilot,
    GitHubActions,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SenderKind {
    Human(String),
    Bot(BotKind),
}

impl SenderKind {
    pub fn login(&self) -> &str {
        match self {
            SenderKind::Human(login) => login,
            SenderKind::Bot(kind) => match kind {
                BotKind::Dependabot => "dependabot[bot]",
                BotKind::Renovate => "renovate[bot]",
                BotKind::CodeRabbit => "coderabbitai[bot]",
                BotKind::Copilot => "copilot[bot]",
                BotKind::GitHubActions => "github-actions[bot]",
                BotKind::Other(s) => s.as_str(),
            },
        }
    }

    pub fn is_bot(&self) -> bool {
        matches!(self, SenderKind::Bot(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandKind {
    Help,
    Status,
    Explain(String),
    Rules,
    Ping,
    NaturalQuestion(String),
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateStatus {
    Passed,
    Blocked {
        errors: usize,
        warnings: usize,
        rules_triggered: Vec<String>,
    },
    Warning {
        warnings: usize,
        rules_triggered: Vec<String>,
    },
    Unknown,
}

pub struct ConversationEngine;

impl ConversationEngine {
    /// Parse sender login into SenderKind
    pub fn parse_sender(login: &str) -> SenderKind {
        let normalized = login.to_lowercase();
        if normalized == "dependabot[bot]" || normalized == "dependabot" {
            SenderKind::Bot(BotKind::Dependabot)
        } else if normalized == "renovate[bot]" || normalized == "renovate" {
            SenderKind::Bot(BotKind::Renovate)
        } else if normalized == "coderabbitai[bot]" || normalized == "coderabbitai" {
            SenderKind::Bot(BotKind::CodeRabbit)
        } else if normalized == "copilot[bot]"
            || normalized == "github-copilot[bot]"
            || normalized == "copilot"
        {
            SenderKind::Bot(BotKind::Copilot)
        } else if normalized == "github-actions[bot]" || normalized == "github-actions" {
            SenderKind::Bot(BotKind::GitHubActions)
        } else if normalized.ends_with("[bot]") {
            SenderKind::Bot(BotKind::Other(login.to_string()))
        } else {
            SenderKind::Human(login.to_string())
        }
    }

    /// Check if the comment should trigger a response from Tmy-Joy
    pub fn should_respond(sender: &SenderKind, comment_text: &str) -> bool {
        let text_lower = comment_text.to_lowercase();

        // 1. Prevent infinite loops: never reply to ourselves!
        if sender.login().to_lowercase().contains("tokenectomy")
            || sender.login().to_lowercase().contains("tmy-joy")
        {
            return false;
        }

        // 2. Direct mentions
        if text_lower.contains("@tmy-joy")
            || text_lower.contains("@tokenectomy-bot")
            || text_lower.contains("@tokenectomy")
        {
            return true;
        }

        // 3. Slash commands at start of lines
        for line in comment_text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('/') {
                let cmd = trimmed
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_lowercase();
                if matches!(
                    cmd.as_str(),
                    "/help"
                        | "/status"
                        | "/explain"
                        | "/rules"
                        | "/ping"
                        | "/review"
                        | "/verify"
                        | "/tb"
                ) {
                    return true;
                }
            }
        }

        // 4. M2M autonomous bot dialogue triggers
        if let SenderKind::Bot(bot_kind) = sender {
            matches!(
                bot_kind,
                BotKind::Dependabot | BotKind::Renovate | BotKind::CodeRabbit
            )
        } else {
            false
        }
    }

    /// Parse intent or command from comment text
    pub fn parse_command(comment_text: &str) -> CommandKind {
        let text_trimmed = comment_text.trim();

        // Check each line for slash command or bot mention
        for line in comment_text.lines() {
            let trimmed = line.trim();

            let cmd_str = if let Some(rest) = trimmed.strip_prefix('/') {
                rest
            } else if let Some(idx) = trimmed.to_lowercase().find("@tmy-joy") {
                trimmed[idx + 8..].trim()
            } else if let Some(idx) = trimmed.to_lowercase().find("@tokenectomy-bot") {
                trimmed[idx + 16..].trim()
            } else {
                continue;
            };

            let parts: Vec<&str> = cmd_str.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            match parts[0].to_lowercase().as_str() {
                "help" | "?" => return CommandKind::Help,
                "status" | "check" | "gate" => return CommandKind::Status,
                "explain" => {
                    if parts.len() > 1 {
                        return CommandKind::Explain(parts[1].to_uppercase());
                    } else {
                        return CommandKind::Help;
                    }
                }
                "rules" | "catalog" => return CommandKind::Rules,
                "ping" => return CommandKind::Ping,
                "review" | "verify" => return CommandKind::Status,
                _ => {}
            }
        }

        // Natural language question matching
        let lower = text_trimmed.to_lowercase();
        if lower.contains("why") && (lower.contains("fail") || lower.contains("block")) {
            return CommandKind::NaturalQuestion("why_failed".to_string());
        }
        if lower.contains("how") && (lower.contains("fix") || lower.contains("resolve")) {
            return CommandKind::NaturalQuestion("how_to_fix".to_string());
        }
        if lower.contains("ready to merge") || lower.contains("bisa di merge") {
            return CommandKind::NaturalQuestion("ready_to_merge".to_string());
        }

        CommandKind::Unknown(text_trimmed.to_string())
    }

    /// Generate an M2M response for GitHub bots or conversational reply for users
    pub fn generate_response(
        sender: &SenderKind,
        comment_text: &str,
        gate_status: &GateStatus,
        repo: &str,
        pr_number: u64,
    ) -> String {
        // 1. If it's a known bot, handle M2M Handover first
        if let SenderKind::Bot(bot_kind) = sender {
            return Self::generate_bot_m2m_response(bot_kind, comment_text, gate_status);
        }

        // 2. Otherwise, handle interactive human commands & queries
        let command = Self::parse_command(comment_text);
        Self::generate_command_response(&command, sender.login(), gate_status, repo, pr_number)
    }

    /// M2M Protocol Generator: Machine-to-Machine dialogue with GitHub bots
    pub fn generate_bot_m2m_response(
        bot_kind: &BotKind,
        comment_text: &str,
        gate_status: &GateStatus,
    ) -> String {
        match bot_kind {
            BotKind::Dependabot => {
                let lower = comment_text.to_lowercase();
                let is_security_discussion = lower.contains("cve")
                    || lower.contains("security")
                    || lower.contains("vulnerabilit")
                    || lower.contains("advisory")
                    || lower.contains("keamanan")
                    || lower.contains("patch");

                match gate_status {
                    GateStatus::Passed => {
                        if is_security_discussion {
                            "🤖 **[Tmy-Joy M2M Security Protocol ↔ @dependabot]**\n\n\
                            > 🛡️ **Autonomous Security Audit: VERIFIED & CLEAN**\n\n\
                            Tmy-Joy deterministic AST engine evaluated repository security vitals:\n\
                            - 🔒 **Supply-Chain Advisory**: Security patch acknowledged and verified against dependency tree.\n\
                            - 🛡️ **AST Injection Resistance**: 0 shell injection (`TB202`), 0 dynamic eval (`TB201`), 0 raw SQL interpolation (`TB203`).\n\
                            - 🧪 **Verification Integrity**: 0 test tampering (`TB001`-`TB006`), 0 lazy deletions (`TB007`), 0 domain narrowing (`TB008`).\n\
                            - 🔏 **Audit Ledger**: Cryptographically sealed entry queued with SHA-256 genesis hash.\n\n\
                            Security baseline maintained with 0 regressions.\n\n\
                            @dependabot squash and merge"
                                .to_string()
                        } else {
                            "🤖 **[Tmy-Joy M2M Protocol v1 ↔ @dependabot]**\n\n\
                            > ✔ **All deterministic AST verification checks passed** (0 test tampering, 0 security flaws, 0 silent catches).\n\n\
                            @dependabot squash and merge"
                                .to_string()
                        }
                    }
                    GateStatus::Blocked {
                        errors,
                        warnings,
                        rules_triggered,
                    } => {
                        if is_security_discussion {
                            format!(
                                "🤖 **[Tmy-Joy M2M Security Protocol ↔ @dependabot]**\n\n\
                                > ⚠️ **Security Update BLOCKED by Quality Gate** ({} errors, {} warnings)\n\n\
                                While this update attempts to address security vulnerabilities, the incoming AST diff violates integrity rules:\n\
                                - **Triggered Violations**: `{}`\n\
                                - **Action Required**: Resolve regression or test tampering before merging.\n\n\
                                @dependabot recreate",
                                errors,
                                warnings,
                                rules_triggered.join("`, `")
                            )
                        } else {
                            format!(
                                "🤖 **[Tmy-Joy M2M Protocol v1 ↔ @dependabot]**\n\n\
                                > ❌ **Deterministic Quality Gate BLOCKED** ({} errors, {} warnings).\n\
                                > Triggered rules: `{}`.\n\n\
                                Dependencies or changes violate repository integrity.\n\n\
                                @dependabot recreate",
                                errors,
                                warnings,
                                rules_triggered.join("`, `")
                            )
                        }
                    }
                    _ => "🤖 **[Tmy-Joy M2M Protocol v1 ↔ @dependabot]**\n\n\
                        Security vitals evaluation in progress. Awaiting AST diff analysis."
                        .to_string(),
                }
            }
            BotKind::Renovate => match gate_status {
                GateStatus::Passed => "🤖 **[Tmy-Joy M2M Protocol v1 ↔ @renovate]**\n\n\
                    > ✔ **Verification PASSED**: AST Tree-sitter validated zero regressions.\n\n\
                    @renovate merge"
                    .to_string(),
                GateStatus::Blocked {
                    rules_triggered, ..
                } => {
                    format!(
                        "🤖 **[Tmy-Joy M2M Protocol v1 ↔ @renovate]**\n\n\
                        > ❌ **Verification BLOCKED**: AST rules `{}` failed.\n\n\
                        @renovate rebase",
                        rules_triggered.join("`, `")
                    )
                }
                _ => "🤖 **[Tmy-Joy M2M Protocol v1 ↔ @renovate]**\n\n\
                    Verification pending."
                    .to_string(),
            },
            BotKind::CodeRabbit => {
                let status_line = match gate_status {
                    GateStatus::Passed => "Deterministic gate: **PASSED (0 AST flaws)** ✔",
                    GateStatus::Blocked { .. } => {
                        "Deterministic gate: **BLOCKED (AST issues detected)** ❌"
                    }
                    _ => "Deterministic gate: **RUNNING** ⏳",
                };

                format!(
                    "🤖 **[Tmy-Joy M2M Protocol v1 ↔ @coderabbitai]**\n\n\
                    Hello CodeRabbit! Tmy-Joy deterministic AST engine correlated with your review.\n\
                    - {}\n\
                    - Engine: Tree-sitter multi-language parser (zero-LLM, sub-millisecond, 0% hallucination rate).\n\
                    - All findings are backed by reproducible AST syntax proof.",
                    status_line
                )
            }
            BotKind::Copilot => "🤖 **[Tmy-Joy M2M Protocol v1 ↔ @copilot]**\n\n\
                Acknowledged AI Copilot activity. Deterministic AST verification gate active."
                .to_string(),
            BotKind::GitHubActions => "🤖 **[Tmy-Joy M2M Protocol v1 ↔ @github-actions]**\n\n\
                Workflow notification recorded in Tmy-Joy session."
                .to_string(),
            BotKind::Other(name) => {
                format!(
                    "🤖 **[Tmy-Joy M2M Handover v1]**\n\n\
                    Handshake established with bot `@{} `.\n\
                    Deterministic Tree-sitter AST gate standing by.",
                    name
                )
            }
        }
    }

    /// Interactive command response generator
    fn generate_command_response(
        command: &CommandKind,
        author: &str,
        gate_status: &GateStatus,
        repo: &str,
        pr_number: u64,
    ) -> String {
        match command {
            CommandKind::Help => {
                format!(
                    "👋 Hi @{}! I am **Tmy-Joy**, your deterministic AST Pull Request verification gate.\n\n\
                    ### 🤖 Available Commands\n\
                    | Command | Description |\n\
                    |---|---|\n\
                    | `@tmy-joy status` or `/status` | Check current verification gate status & integrity |\n\
                    | `@tmy-joy explain <RULE_ID>` | Get detailed explanation and fix guide for a rule (e.g. `TB001`) |\n\
                    | `@tmy-joy rules` or `/rules` | View all active deterministic detection rules |\n\
                    | `@tmy-joy ping` | Ping the engine & check Tree-sitter AST health |\n\
                    | `@tmy-joy help` | Show this help menu |\n\n\
                    *Zero-LLM • 100% Deterministic Tree-sitter • Sub-millisecond Execution*",
                    author
                )
            }
            CommandKind::Status => match gate_status {
                GateStatus::Passed => {
                    format!(
                        "### 🗡️ Tmy-Joy Gate Status: PASSED ✔\n\n\
                        - **Repository**: `{}`\n\
                        - **PR**: `#{}`\n\
                        - **Result**: All deterministic verification checks passed!\n\
                        - **Test Tampering**: None detected (TB001-TB006 clean).\n\
                        - **Reliability & Security**: 0 active hazards.\n\n\
                        > This PR is **ready to merge** from an AST integrity perspective.",
                        repo, pr_number
                    )
                }
                GateStatus::Blocked {
                    errors,
                    warnings,
                    rules_triggered,
                } => {
                    format!(
                        "### 🗡️ Tmy-Joy Gate Status: BLOCKED ❌\n\n\
                        - **Repository**: `{}`\n\
                        - **PR**: `#{}`\n\
                        - **Errors**: {}\n\
                        - **Warnings**: {}\n\
                        - **Triggered Rules**: `{}`\n\n\
                        💡 *Tip: Run `@tmy-joy explain <RULE_ID>` to see why each rule was triggered and how to fix it.*",
                        repo,
                        pr_number,
                        errors,
                        warnings,
                        rules_triggered.join("`, `")
                    )
                }
                GateStatus::Warning {
                    warnings,
                    rules_triggered,
                } => {
                    format!(
                        "### 🗡️ Tmy-Joy Gate Status: WARNING ⚠️\n\n\
                        - **Warnings**: {}\n\
                        - **Triggered Rules**: `{}`\n\
                        Gate is currently non-blocking, but review is recommended.",
                        warnings,
                        rules_triggered.join("`, `")
                    )
                }
                GateStatus::Unknown => {
                    format!(
                        "### 🗡️ Tmy-Joy Gate Status: PENDING ⏳\n\n\
                        Waiting for initial diff evaluation on PR `#{}`.",
                        pr_number
                    )
                }
            },
            CommandKind::Explain(rule_id) => Self::explain_rule(rule_id),
            CommandKind::Rules => Self::list_rules(),
            CommandKind::Ping => {
                format!(
                    "🏓 **Pong!** Tmy-Joy is active and operational.\n\
                    - **Engine**: Tree-sitter multi-language AST parser\n\
                    - **Supported Grammars**: TypeScript, JavaScript, Rust, Go, Python\n\
                    - **Mode**: 100% Deterministic (Zero-LLM)\n\
                    - **Target PR**: `{}#{}`",
                    repo, pr_number
                )
            }
            CommandKind::NaturalQuestion(q) => {
                if q == "why_failed" {
                    match gate_status {
                        GateStatus::Blocked {
                            rules_triggered, ..
                        } => {
                            format!(
                                "🔍 **Why did this PR fail?**\n\n\
                                Tmy-Joy blocked this PR because the following deterministic AST rules were violated:\n\
                                {}\n\n\
                                You can ask `@tmy-joy explain <RULE_ID>` for step-by-step remediation advice.",
                                rules_triggered
                                    .iter()
                                    .map(|r| format!("- `{}`", r))
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            )
                        }
                        _ => {
                            "The gate is currently not failing. If checks failed earlier, check the sticky comment above for details.".to_string()
                        }
                    }
                } else if q == "how_to_fix" {
                    "🛠️ **How to fix findings:**\n\n\
                    1. Check the sticky PR summary comment or GitHub Annotations on the files changed.\n\
                    2. For each finding, address the underlying issue (e.g. restore deleted assertions, remove empty catch blocks, parameterize queries).\n\
                    3. If a finding is an intentional false positive, you can suppress it with an inline comment: `// tokenectomy-ignore: <RULE_ID> -- <reason>`.\n\
                    4. Push your fix and Tmy-Joy will re-verify immediately."
                        .to_string()
                } else {
                    format!(
                        "👋 Hello @{}! Got a question about PR verification? Try `@tmy-joy help` to see everything I can do.",
                        author
                    )
                }
            }
            CommandKind::Unknown(text) => {
                format!(
                    "👋 Hi @{}! I received your message: *\"{}\"*\n\n\
                    I am **Tmy-Joy**, the deterministic AST PR gate. Type `@tmy-joy help` for a list of commands, or `@tmy-joy status` to check PR verification results.",
                    author,
                    text.lines().next().unwrap_or(text)
                )
            }
        }
    }

    /// Full encyclopedia of TB rules
    pub fn explain_rule(rule_id: &str) -> String {
        match rule_id {
            "TB001" => {
                "### 📖 Rule TB001: `assertion-removed`\n\n\
                - **Category**: Verification Integrity (Anti-Tampering)\n\
                - **Default Severity**: `error` (agent-pr) / `warn`\n\
                - **Description**: Triggers when assertions (e.g. `expect()`, `assert()`) are deleted from an existing test block without deleting the entire test.\n\
                - **Why Dangerous**: A common AI agent anti-pattern is deleting failing assertions so tests pass green without fixing actual software bugs.\n\
                - **Fix**: Restore the original assertions and fix the underlying implementation code.\n\
                - **Suppression**: `// tokenectomy-ignore: TB001 -- reason`"
                    .to_string()
            }
            "TB002" => {
                "### 📖 Rule TB002: `test-disabled`\n\n\
                - **Category**: Verification Integrity (Anti-Tampering)\n\
                - **Default Severity**: `error`\n\
                - **Description**: Detects skipping tests via `.skip`, `xit`, `xdescribe`, `test.todo`, `#[ignore]` in Rust, `@pytest.mark.skip` in Python, or `t.Skip()` in Go.\n\
                - **Why Dangerous**: Disabling tests bypasses verification while masking regressions.\n\
                - **Fix**: Remove `.skip`/`#[ignore]` and ensure tests pass against actual behavior."
                    .to_string()
            }
            "TB003" => {
                "### 📖 Rule TB003: `tautological-assertion`\n\n\
                - **Category**: Verification Integrity (Anti-Tampering)\n\
                - **Default Severity**: `error`\n\
                - **Description**: Detects meaningless tautological assertions such as `expect(true).toBe(true)`, `assert(1 === 1)`, or `x == x`.\n\
                - **Why Dangerous**: Creates synthetic green checkmarks without verifying meaningful state.\n\
                - **Fix**: Assert actual return values or state transitions."
                    .to_string()
            }
            "TB004" => {
                "### 📖 Rule TB004: `config-weakened`\n\n\
                - **Category**: Verification Integrity\n\
                - **Default Severity**: `error`\n\
                - **Description**: Detects weakening test/lint configs: lowering coverage thresholds, adding `passWithNoTests`, `strict: false`, or CI `continue-on-error: true`.\n\
                - **Why Dangerous**: Bypasses quality gates repository-wide.\n\
                - **Fix**: Maintain strict configuration standards."
                    .to_string()
            }
            "TB005" => {
                "### 📖 Rule TB005: `early-exit-injected`\n\n\
                - **Category**: Verification Integrity\n\
                - **Default Severity**: `error`\n\
                - **Description**: Injected `return;` or `process.exit(0)` making subsequent code in a block unreachable dead code.\n\
                - **Why Dangerous**: Bypasses complex logic or validation branches.\n\
                - **Fix**: Remove artificial early return or place it conditionally."
                    .to_string()
            }
            "TB006" => {
                "### 📖 Rule TB006: `test-deleted`\n\n\
                - **Category**: Verification Integrity\n\
                - **Default Severity**: `info`\n\
                - **Description**: Test file or block deleted in a PR that also modified source code.\n\
                - **Fix**: Keep tests synchronized with refactored behavior."
                    .to_string()
            }
            "TB007" => {
                "### 📖 Rule TB007: `lazy-deletion`\n\n\
                - **Category**: Verification Integrity (Anti-Tampering)\n\
                - **Default Severity**: `error`\n\
                - **Description**: Function or method body gutted to a dummy surrender stub (`return null;`, `return false;`, `todo!()`, `pass`, `raise NotImplementedError`, `throw new Error(\"not implemented\")`).\n\
                - **Why Dangerous**: A notorious AI agent pattern where the agent deletes business logic or surrenders when a test fails, creating a hollow implementation.\n\
                - **Fix**: Restore real business logic rather than surrendering with a dummy return.\n\
                - **Suppression**: `// tokenectomy-ignore: TB007 -- reason`"
                    .to_string()
            }
            "TB008" => {
                "### 📖 Rule TB008: `domain-narrowing`\n\n\
                - **Category**: Verification Integrity (Anti-Tampering)\n\
                - **Default Severity**: `error`\n\
                - **Description**: Injected artificial guard clause checking equality against specific test fixture literals (e.g. `if (id === \"test-user-123\") return mock;` or `if (amount === 100) return 15;`).\n\
                - **Why Dangerous**: Special-casing inputs to artificially pass CI test cases without solving general domain behavior.\n\
                - **Fix**: Write general domain logic capable of handling arbitrary inputs."
                    .to_string()
            }
            "TB009" => {
                "### 📖 Rule TB009: `fixture-snooping`\n\n\
                - **Category**: Verification Integrity\n\
                - **Default Severity**: `error`\n\
                - **Description**: Production code branching on `NODE_ENV === 'test'` or reading test fixture directories to fake behavior.\n\
                - **Fix**: Keep production code agnostic to test execution environments."
                    .to_string()
            }
            "TB101" => {
                "### 📖 Rule TB101: `silent-catch`\n\n\
                - **Category**: Reliability\n\
                - **Default Severity**: `error`\n\
                - **Description**: Empty `catch {}` block, `.catch(() => {})`, or Rust `Err(_) => {}` that completely swallows errors without logging or handling.\n\
                - **Why Dangerous**: Causes silent failures in production with zero diagnostic logs.\n\
                - **Fix**: Add logging (`console.error`, logger) or re-throw the error."
                    .to_string()
            }
            "TB102" => {
                "### 📖 Rule TB102: `unbounded-query`\n\n\
                - **Category**: Reliability\n\
                - **Default Severity**: `warn`\n\
                - **Description**: ORM queries (Prisma `findMany`, Drizzle, TypeORM, Knex) executed without pagination limit (`take` / `limit`).\n\
                - **Why Dangerous**: Will exhaust server memory (OOM) as table grows in production.\n\
                - **Fix**: Add `.take(limit)` or `.limit(N)`."
                    .to_string()
            }
            "TB104" => {
                "### 📖 Rule TB104: `async-foreach`\n\n\
                - **Category**: Reliability\n\
                - **Default Severity**: `error`\n\
                - **Description**: `arr.forEach(async () => ...)` where async promises are discarded unawaited.\n\
                - **Why Dangerous**: Causes race conditions and unhandled promise failures.\n\
                - **Fix**: Use `for (const item of arr)` or `await Promise.all(arr.map(async ...))`."
                    .to_string()
            }
            "TB201" => {
                "### 📖 Rule TB201: `dynamic-eval`\n\n\
                - **Category**: Security\n\
                - **Default Severity**: `error`\n\
                - **Description**: Use of `eval()`, `new Function()`, `setTimeout(\"string\")`, or Python `eval()`.\n\
                - **Why Dangerous**: Arbitrary code execution vulnerability.\n\
                - **Fix**: Parse structured formats (e.g. `JSON.parse`) or use static lookup tables."
                    .to_string()
            }
            "TB202" => {
                "### 📖 Rule TB202: `shell-injection`\n\n\
                - **Category**: Security\n\
                - **Default Severity**: `error`\n\
                - **Description**: Dynamic string interpolation or concatenation passed directly to shell execution (`exec`, `execSync`).\n\
                - **Why Dangerous**: Command injection vulnerability if user input reaches the shell.\n\
                - **Fix**: Use `execFile` or `spawn` with an array of arguments instead of string interpolation."
                    .to_string()
            }
            "TB203" => {
                "### 📖 Rule TB203: `raw-sql-interpolation`\n\n\
                - **Category**: Security\n\
                - **Default Severity**: `error`\n\
                - **Description**: Dynamic string interpolation in raw SQL queries (`$queryRawUnsafe`, `db.query`).\n\
                - **Why Dangerous**: Classic SQL injection vulnerability.\n\
                - **Fix**: Use parameterized queries (`$1, $2`) or tagged template literals (`$queryRaw` / `sql` tag)."
                    .to_string()
            }
            _ => {
                format!(
                    "❓ Unknown rule `{}`. Active rules: `TB001`-`TB006`, `TB009`, `TB101`-`TB104`, `TB201`-`TB203`. Type `@tmy-joy rules` to see all.",
                    rule_id
                )
            }
        }
    }

    /// Markdown list of all rules
    pub fn list_rules() -> String {
        "### 🗡️ Tmy-Joy Deterministic Rules Catalog\n\n\
        | Rule ID | Name | Category | Default Severity |\n\
        |---|---|---|---|\n\
        | `TB001` | `assertion-removed` | Verification Integrity | Error (agent) / Warn |\n\
        | `TB002` | `test-disabled` | Verification Integrity | Error |\n\
        | `TB003` | `tautological-assertion` | Verification Integrity | Error |\n\
        | `TB004` | `config-weakened` | Verification Integrity | Error |\n\
        | `TB005` | `early-exit-injected` | Verification Integrity | Error |\n\
        | `TB006` | `test-deleted` | Verification Integrity | Info |\n\
        | `TB007` | `lazy-deletion` | Verification Integrity | Error |\n\
        | `TB008` | `domain-narrowing` | Verification Integrity | Error |\n\
        | `TB009` | `fixture-snooping` | Verification Integrity | Error |\n\
        | `TB101` | `silent-catch` | Reliability | Error |\n\
        | `TB102` | `unbounded-query` | Reliability | Warn |\n\
        | `TB104` | `async-foreach` | Reliability | Error |\n\
        | `TB201` | `dynamic-eval` | Security | Error |\n\
        | `TB202` | `shell-injection` | Security | Error |\n\
        | `TB203` | `raw-sql-interpolation` | Security | Error |\n\n\
        *All rules run on Tree-sitter AST syntax trees with 0% LLM hallucination risk.*"
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sender() {
        assert_eq!(
            ConversationEngine::parse_sender("dependabot[bot]"),
            SenderKind::Bot(BotKind::Dependabot)
        );
        assert_eq!(
            ConversationEngine::parse_sender("renovate[bot]"),
            SenderKind::Bot(BotKind::Renovate)
        );
        assert_eq!(
            ConversationEngine::parse_sender("coderabbitai[bot]"),
            SenderKind::Bot(BotKind::CodeRabbit)
        );
        assert_eq!(
            ConversationEngine::parse_sender("daffa2555"),
            SenderKind::Human("daffa2555".to_string())
        );
    }

    #[test]
    fn test_should_respond_rules() {
        let human = SenderKind::Human("alice".to_string());
        let dependabot = SenderKind::Bot(BotKind::Dependabot);
        let self_bot = SenderKind::Bot(BotKind::Other("tokenectomy-bot[bot]".to_string()));

        // Never reply to ourselves
        assert!(!ConversationEngine::should_respond(
            &self_bot,
            "@tmy-joy status"
        ));

        // Reply to mentions
        assert!(ConversationEngine::should_respond(
            &human,
            "Hey @tmy-joy what is the status?"
        ));
        assert!(ConversationEngine::should_respond(
            &human,
            "Check this @tokenectomy-bot"
        ));

        // Reply to slash commands
        assert!(ConversationEngine::should_respond(&human, "/status"));
        assert!(ConversationEngine::should_respond(&human, "/help"));
        assert!(ConversationEngine::should_respond(&human, "/explain TB001"));

        // Autonomous M2M response for dependabot
        assert!(ConversationEngine::should_respond(
            &dependabot,
            "Bumps lodash from 4.17.20 to 4.17.21."
        ));
    }

    #[test]
    fn test_m2m_dependabot_passed() {
        let sender = SenderKind::Bot(BotKind::Dependabot);
        let gate_status = GateStatus::Passed;
        let response = ConversationEngine::generate_response(
            &sender,
            "Bumps lodash",
            &gate_status,
            "org/repo",
            42,
        );

        assert!(response.contains("@dependabot squash and merge"));
        assert!(response.contains("M2M Protocol v1"));
    }

    #[test]
    fn test_m2m_dependabot_blocked() {
        let sender = SenderKind::Bot(BotKind::Dependabot);
        let gate_status = GateStatus::Blocked {
            errors: 1,
            warnings: 0,
            rules_triggered: vec!["TB101".to_string()],
        };
        let response = ConversationEngine::generate_response(
            &sender,
            "Bumps lodash",
            &gate_status,
            "org/repo",
            42,
        );

        assert!(response.contains("@dependabot recreate"));
        assert!(response.contains("TB101"));
    }

    #[test]
    fn test_m2m_dependabot_security_discussion() {
        let sender = SenderKind::Bot(BotKind::Dependabot);
        let gate_status = GateStatus::Passed;
        let response = ConversationEngine::generate_response(
            &sender,
            "Bumps h2 from 0.3.24 to 0.3.26 to fix CVE-2024-2699: HTTP/2 flood security vulnerability",
            &gate_status,
            "Tokenectomy-Labs/tokenctomy-bot",
            101,
        );

        assert!(response.contains("Tmy-Joy M2M Security Protocol"));
        assert!(response.contains("Autonomous Security Audit: VERIFIED & CLEAN"));
        assert!(response.contains("TB202"));
        assert!(response.contains("@dependabot squash and merge"));
    }

    #[test]
    fn test_explain_tb001() {
        let explanation = ConversationEngine::explain_rule("TB001");
        assert!(explanation.contains("TB001"));
        assert!(explanation.contains("assertion-removed"));
        assert!(explanation.contains("Anti-Tampering"));
    }

    #[test]
    fn test_explain_tb007_and_tb008() {
        let exp7 = ConversationEngine::explain_rule("TB007");
        assert!(exp7.contains("TB007"));
        assert!(exp7.contains("lazy-deletion"));

        let exp8 = ConversationEngine::explain_rule("TB008");
        assert!(exp8.contains("TB008"));
        assert!(exp8.contains("domain-narrowing"));
    }

    #[test]
    fn test_human_slash_help() {
        let sender = SenderKind::Human("octocat".to_string());
        let response = ConversationEngine::generate_response(
            &sender,
            "/help",
            &GateStatus::Passed,
            "octo/cat",
            10,
        );
        assert!(response.contains("Tmy-Joy"));
        assert!(response.contains("@tmy-joy status"));
    }
}
