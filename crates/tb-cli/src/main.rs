use std::collections::HashMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use colored::*;

mod benchmark;
mod ledger_cmd;
mod mcp;
mod rules_cmd;
mod server;

use tb_diff::{DiffParser, GitExtractor};
use tb_report::{
    AuditLedger, FailOn, GatePolicy, GitHubAnnotationReporter, JsonReporter, ReviewBatchGenerator,
    SarifReporter, StickySummaryReporter, TextReporter, WebhookNotification, WebhookType,
};
use tb_rules::{Config, CustomRule, RuleEngine};

#[derive(Parser)]
#[command(
    name = "tokenectomy-bot",
    author = "Tokenectomy Labs",
    version = "0.1.0",
    about = "Deterministic, AST-based Pull Request verification gate in the AI agent era."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Review a PR diff or git revision range
    Review(Box<ReviewArgs>),

    /// Initialize default tokenectomy.json configuration
    Init {
        #[arg(short, long, default_value = "tokenectomy.json")]
        output: PathBuf,
    },

    /// Run stress & precision benchmark on hardware
    Benchmark {
        /// Number of test cases to benchmark
        #[arg(short, long, default_value_t = 1000)]
        count: usize,
    },

    /// Start autonomous Model Context Protocol (MCP) JSON-RPC stdio server
    Mcp,

    /// Validate, test, or manage custom Tree-sitter SCM rules
    #[command(subcommand)]
    Rules(RulesCommands),

    /// Manage and verify SHA-256 sealed audit ledger
    #[command(subcommand)]
    Ledger(LedgerCommands),

    /// Start autonomous GitHub App Webhook server
    Serve(ServeArgs),
}

#[derive(Subcommand)]
pub enum RulesCommands {
    /// Validate custom .scm rule syntax and optionally execute against fixture code
    Validate {
        /// Path to custom .scm rule file
        rule_file: PathBuf,

        /// Optional test fixture source file to verify rule match
        #[arg(short, long)]
        fixture: Option<PathBuf>,

        /// Language for fixture testing (e.g. typescript, python, rust, go)
        #[arg(short, long)]
        language: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum LedgerCommands {
    /// Verify cryptographic hash chain integrity of a ledger file
    Verify {
        /// Path to audit ledger file
        ledger_file: PathBuf,
    },

    /// Display aggregated compliance and security metrics dashboard
    Metrics {
        /// Path to audit ledger file
        ledger_file: PathBuf,
    },
}

#[derive(Args)]
pub struct ServeArgs {
    /// Port to listen on
    #[arg(short, long, env = "PORT", default_value_t = 8080)]
    port: u16,

    /// GitHub webhook secret for HMAC-SHA256 signature verification
    #[arg(short, long, env = "GITHUB_WEBHOOK_SECRET")]
    secret: Option<String>,

    /// Path to record sealed audit ledger entries
    #[arg(long, env = "TOKENECTOMY_LEDGER")]
    ledger: Option<PathBuf>,

    /// Path or preset for organization policy enforcement
    #[arg(long, env = "TOKENECTOMY_ORG_POLICY")]
    org_policy: Option<String>,

    /// Directory containing custom .scm rules
    #[arg(long, default_value = ".tokenectomy/rules")]
    custom_rules_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
    Sarif,
    Github,
    Summary,
    ReviewBatch,
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum WebhookProvider {
    #[default]
    Auto,
    Slack,
    Discord,
    Teams,
}

impl From<WebhookProvider> for WebhookType {
    fn from(p: WebhookProvider) -> Self {
        match p {
            WebhookProvider::Auto => WebhookType::Auto,
            WebhookProvider::Slack => WebhookType::Slack,
            WebhookProvider::Discord => WebhookType::Discord,
            WebhookProvider::Teams => WebhookType::Teams,
        }
    }
}

#[derive(Parser)]
pub struct ReviewArgs {
    /// Path to custom tokenectomy.json configuration file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Path or string for organization-wide policy enforcement
    #[arg(long, env = "TOKENECTOMY_ORG_POLICY")]
    org_policy: Option<String>,

    /// Allow local repo config to downgrade or disable organization policy rules
    #[arg(long)]
    allow_policy_downgrade: bool,

    /// Directory containing custom .scm rules
    #[arg(long)]
    custom_rules_dir: Option<PathBuf>,

    /// Path to SHA-256 sealed audit ledger file to append PR verification record
    #[arg(long)]
    ledger: Option<PathBuf>,

    /// Webhook URL to dispatch notification upon review completion (Slack / Discord / Teams)
    #[arg(long, env = "TOKENECTOMY_WEBHOOK_URL")]
    webhook_url: Option<String>,

    /// Webhook provider type
    #[arg(long, value_enum, default_value = "auto")]
    webhook_type: WebhookProvider,

    /// Repository full name (e.g. owner/repo)
    #[arg(long, env = "GITHUB_REPOSITORY")]
    repo_name: Option<String>,

    /// Pull request number
    #[arg(long)]
    pr_number: Option<u64>,

    /// Base git reference (e.g. main, origin/main, HEAD~1)
    #[arg(short, long, default_value = "origin/main")]
    base: String,

    /// Head git reference (e.g. HEAD, feature-branch)
    #[arg(long, default_value = "HEAD")]
    head: String,

    /// Path to git repository directory
    #[arg(short, long, default_value = ".")]
    repo_dir: PathBuf,

    /// Read unified diff directly from a file or stdin ("-")
    #[arg(long)]
    diff_file: Option<String>,

    /// Report output format
    #[arg(short, long, value_enum, default_value = "text")]
    format: OutputFormat,

    /// Gate failure threshold
    #[arg(long, value_enum, default_value = "error")]
    fail_on: FailOn,

    /// Save output report to a file instead of stdout
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Fail open (exit 0) if internal git or tool error occurs
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    fail_open: bool,

    /// Path to baseline JSON file containing fingerprints to ignore
    #[arg(long)]
    baseline: Option<PathBuf>,

    /// Force Agent PR mode (upgrades TB001-TB005 anti-tampering rules to error)
    #[arg(long)]
    agent_pr: bool,

    /// Maximum number of files to scan (resilience against gigantic PRs)
    #[arg(long, default_value_t = 1000)]
    max_files: usize,

    /// Maximum file size in bytes to scan
    #[arg(long, default_value_t = 1_048_576)]
    max_file_size: u64,

    /// Automatically append sticky summary to $GITHUB_STEP_SUMMARY
    #[arg(long, default_value_t = true)]
    step_summary: bool,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { output } => {
            if let Err(e) = run_init(&output) {
                eprintln!("{}: {}", "Error".red().bold(), e);
                process::exit(2);
            }
        }
        Commands::Benchmark { count } => {
            benchmark::run_benchmark(count);
        }
        Commands::Mcp => {
            if let Err(e) = mcp::run_mcp_server() {
                eprintln!("{}: {}", "MCP Server Error".red().bold(), e);
                process::exit(1);
            }
        }
        Commands::Rules(cmd) => match cmd {
            RulesCommands::Validate {
                rule_file,
                fixture,
                language,
            } => {
                if let Err(e) = rules_cmd::run_rules_validate(
                    &rule_file,
                    fixture.as_deref(),
                    language.as_deref(),
                ) {
                    eprintln!("{}: {:#}", "Rule Validation Error".red().bold(), e);
                    process::exit(1);
                }
            }
        },
        Commands::Ledger(cmd) => match cmd {
            LedgerCommands::Verify { ledger_file } => {
                if let Err(e) = ledger_cmd::run_ledger_verify(&ledger_file) {
                    eprintln!("{}: {:#}", "Ledger Verification Error".red().bold(), e);
                    process::exit(1);
                }
            }
            LedgerCommands::Metrics { ledger_file } => {
                if let Err(e) = ledger_cmd::run_ledger_metrics(&ledger_file) {
                    eprintln!("{}: {:#}", "Ledger Metrics Error".red().bold(), e);
                    process::exit(1);
                }
            }
        },
        Commands::Serve(args) => {
            let server_cfg = server::WebhookServerConfig {
                port: args.port,
                secret: args.secret,
                ledger_path: args.ledger,
                org_policy: args.org_policy,
                custom_rules_dir: Some(args.custom_rules_dir),
            };
            let srv = server::WebhookServer::new(server_cfg);
            if let Err(e) = srv.run() {
                eprintln!("{}: {:#}", "Webhook Server Error".red().bold(), e);
                process::exit(1);
            }
        }
        Commands::Review(args) => {
            let fail_open = args.fail_open;
            match run_review(*args) {
                Ok(exit_code) => process::exit(exit_code),
                Err(err) => {
                    eprintln!("{}: {:#}", "Internal Error".red().bold(), err);
                    if fail_open {
                        eprintln!(
                            "{}",
                            "Warning: fail_open is enabled, exiting with code 0 to not block pipeline."
                                .yellow()
                        );
                        process::exit(0);
                    } else {
                        process::exit(2);
                    }
                }
            }
        }
    }
}

fn run_init(output: &Path) -> Result<()> {
    if output.exists() {
        bail!("File {:?} already exists. Skipping initialization.", output);
    }

    let template = serde_json::json!({
        "$schema": "https://raw.githubusercontent.com/Tokenectomy-Labs/tokenectomy-bot/main/schema.json",
        "extends": "recommended",
        "fail_on": "error",
        "rules": {
            "TB001": { "enabled": true, "severity": "error" },
            "TB002": { "enabled": true, "severity": "error" },
            "TB003": { "enabled": true, "severity": "error" },
            "TB004": { "enabled": true, "severity": "error" },
            "TB005": { "enabled": true, "severity": "error" },
            "TB101": { "enabled": true, "severity": "warn" },
            "TB102": { "enabled": true, "severity": "warn" },
            "TB104": { "enabled": true, "severity": "error" },
            "TB201": { "enabled": true, "severity": "error" },
            "TB202": { "enabled": true, "severity": "error" },
            "TB203": { "enabled": true, "severity": "error" }
        },
        "ignore": [
            "**/vendor/**",
            "**/dist/**",
            "**/*.generated.*"
        ]
    });

    fs::write(output, serde_json::to_string_pretty(&template)? + "\n")
        .with_context(|| format!("Failed to write to {:?}", output))?;

    println!("✔ Initialized {:?}", output);
    Ok(())
}

fn run_review(args: ReviewArgs) -> Result<i32> {
    let start_time = Instant::now();
    let repo_dir = fs::canonicalize(&args.repo_dir).unwrap_or_else(|_| args.repo_dir.clone());

    let (mut diff, old_sources, new_sources) = if let Some(ref diff_source) = args.diff_file {
        let raw_diff = if diff_source == "-" {
            let mut buffer = String::new();
            io::stdin()
                .read_to_string(&mut buffer)
                .context("Failed to read diff from stdin")?;
            buffer
        } else {
            fs::read_to_string(diff_source)
                .with_context(|| format!("Failed to read diff file: {}", diff_source))?
        };

        let diff = DiffParser::parse(&raw_diff);
        let mut old_map = HashMap::new();
        let mut new_map = HashMap::new();

        for file in diff.scannable_files() {
            let full_path = repo_dir.join(&file.path);
            let new_content = fs::read_to_string(&full_path)
                .ok()
                .or_else(|| file.reconstruct_synthetic_new_source());
            if let Some(content) = new_content {
                new_map.insert(file.path.clone(), content);
            }

            let old_lookup = file.old_path.as_ref().unwrap_or(&file.path);
            let old_content = GitExtractor::get_blob(&repo_dir, &args.base, old_lookup)
                .ok()
                .flatten()
                .or_else(|| file.reconstruct_synthetic_old_source());
            if let Some(content) = old_content {
                old_map.insert(file.path.clone(), content);
            }
        }

        (diff, old_map, new_map)
    } else {
        let diff = GitExtractor::extract_diff(&repo_dir, &args.base, &args.head)?;
        let mut old_map = HashMap::new();
        let mut new_map = HashMap::new();

        for file in diff.scannable_files() {
            let old_lookup = file.old_path.as_ref().unwrap_or(&file.path);
            let old_content = GitExtractor::get_blob(&repo_dir, &args.base, old_lookup)
                .ok()
                .flatten()
                .or_else(|| file.reconstruct_synthetic_old_source());
            if let Some(old_c) = old_content {
                old_map.insert(file.path.clone(), old_c);
            }

            let new_content = if args.head == "HEAD" || args.head.is_empty() {
                let p = repo_dir.join(&file.path);
                if p.exists() {
                    fs::read_to_string(&p).ok()
                } else {
                    GitExtractor::get_blob(&repo_dir, &args.head, &file.path)
                        .ok()
                        .flatten()
                }
            } else {
                GitExtractor::get_blob(&repo_dir, &args.head, &file.path)
                    .ok()
                    .flatten()
            }
            .or_else(|| file.reconstruct_synthetic_new_source());

            if let Some(content) = new_content {
                new_map.insert(file.path.clone(), content);
            }
        }

        (diff, old_map, new_map)
    };

    let total_files = diff.files.len();
    // Enforce large PR file cap resilience
    if diff.files.len() > args.max_files {
        diff.files.truncate(args.max_files);
    }
    let total_scanned = diff.files.len();

    let baseline_set = {
        let path = args.baseline.or_else(|| {
            let default_p = repo_dir.join(".tokenectomy-baseline.json");
            if default_p.exists() {
                Some(default_p)
            } else {
                None
            }
        });

        let mut set = std::collections::HashSet::new();
        if let Some(content) = path.and_then(|p| fs::read_to_string(p).ok()) {
            if let Ok(list) = serde_json::from_str::<Vec<String>>(&content) {
                set.extend(list);
            } else if let Some(arr) = serde_json::from_str::<serde_json::Value>(&content)
                .ok()
                .and_then(|v| v.get("fingerprints").cloned())
                .and_then(|v| v.as_array().cloned())
            {
                for item in arr {
                    if let Some(s) = item.as_str() {
                        set.insert(s.to_string());
                    }
                }
            }
        }
        set
    };

    let is_agent = args.agent_pr
        || args.head.starts_with("agent/")
        || args.head.starts_with("bot/")
        || args.head.starts_with("cursor/")
        || args.head.starts_with("cline/")
        || args.head.starts_with("copilot/");

    let mut config = if let Some(ref config_path) = args.config {
        Config::load_from_file(config_path)
            .with_context(|| format!("Failed to load configuration file at {:?}", config_path))?
    } else {
        let default_cfg = repo_dir.join("tokenectomy.json");
        let dot_cfg = repo_dir.join(".tokenectomy.json");
        if default_cfg.exists() {
            Config::load_from_file(&default_cfg)
                .with_context(|| format!("Failed to load configuration from {:?}", default_cfg))?
        } else if dot_cfg.exists() {
            Config::load_from_file(&dot_cfg)
                .with_context(|| format!("Failed to load configuration from {:?}", dot_cfg))?
        } else {
            Config::default()
        }
    };

    // Apply organization policy if configured
    if let Some(ref org_source) = args.org_policy.or_else(|| config.org_policy.clone()) {
        let org_cfg = if Path::new(org_source).exists() {
            Config::load_from_file(Path::new(org_source))?
        } else if org_source.trim().starts_with('{') {
            Config::from_json_str(org_source)?
        } else {
            let json_preset = format!(r#"{{"extends": "{}"}}"#, org_source.trim());
            Config::from_json_str(&json_preset)?
        };
        config
            .apply_org_policy(&org_cfg, args.allow_policy_downgrade)
            .with_context(|| "Failed to enforce organization-wide policy")?;
    }

    let effective_fail_on = if args.fail_on == FailOn::Error {
        if let Some(ref cf) = config.fail_on {
            match cf.to_lowercase().as_str() {
                "warn" | "warning" => FailOn::Warn,
                "none" | "info" => FailOn::None,
                _ => FailOn::Error,
            }
        } else {
            args.fail_on
        }
    } else {
        args.fail_on
    };

    let mut engine = RuleEngine::new()
        .with_baseline(baseline_set)
        .with_agent_mode(is_agent)
        .with_config(config.clone());

    // Load custom .scm rules
    let custom_rules_path = args
        .custom_rules_dir
        .or_else(|| config.custom_rules_dir.map(PathBuf::from))
        .map(|p| if p.is_absolute() { p } else { repo_dir.join(p) });

    if let Some(dir) = custom_rules_path
        && dir.exists()
    {
        let custom_rules = CustomRule::load_dir(&dir)
            .with_context(|| format!("Failed to load custom rules from {:?}", dir))?;
        engine = engine.with_custom_rules(custom_rules);
    }

    let findings = engine.run(&diff, &old_sources, &new_sources);

    let output_str = match args.format {
        OutputFormat::Text => TextReporter::format(&findings),
        OutputFormat::Json => JsonReporter::format(&findings),
        OutputFormat::Sarif => SarifReporter::format(&findings),
        OutputFormat::Github => GitHubAnnotationReporter::format(&findings),
        OutputFormat::Summary => {
            StickySummaryReporter::format(&findings, total_scanned, total_files)
        }
        OutputFormat::ReviewBatch => {
            let payload = ReviewBatchGenerator::build_payload(&diff, &findings);
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
        }
    };

    // If running in GitHub Actions, automatically append sticky summary to $GITHUB_STEP_SUMMARY
    if args.step_summary {
        let summary_md = StickySummaryReporter::format(&findings, total_scanned, total_files);
        let _ = StickySummaryReporter::write_to_step_summary(&summary_md);
    }

    if let Some(ref out_path) = args.output {
        fs::write(out_path, &output_str)
            .with_context(|| format!("Failed to write report to {:?}", out_path))?;
    } else {
        println!("{}", output_str);
    }

    let exit_code = GatePolicy::evaluate_exit_code(&findings, effective_fail_on);

    // If audit ledger configured, append cryptographically sealed entry
    if let Some(ref ledger_path) = args.ledger {
        let repo_name = args.repo_name.clone().unwrap_or_else(|| {
            std::env::var("GITHUB_REPOSITORY").unwrap_or_else(|_| "local/repo".to_string())
        });
        let timestamp = server::chrono_or_simple_timestamp();
        let gate_passed = exit_code == 0;
        let entry = AuditLedger::build_entry(
            ledger_path,
            &repo_name,
            &args.base,
            &args.head,
            total_scanned,
            &findings,
            gate_passed,
            &timestamp,
        )?;
        AuditLedger::append(ledger_path, &entry)?;
        eprintln!(
            "{}",
            format!(
                "✔ Cryptographically sealed audit entry #{} recorded to {:?}",
                entry.index, ledger_path
            )
            .green()
        );
    }

    // If webhook notification configured, dispatch card payload
    if let Some(ref webhook_url) = args.webhook_url {
        let repo_name = args.repo_name.unwrap_or_else(|| {
            std::env::var("GITHUB_REPOSITORY").unwrap_or_else(|_| "local/repo".to_string())
        });
        let pr_num = args.pr_number.or_else(|| {
            std::env::var("GITHUB_EVENT_PULL_REQUEST_NUMBER")
                .ok()
                .and_then(|s| s.parse().ok())
        });
        let pr_url = std::env::var("GITHUB_EVENT_PULL_REQUEST_HTML_URL").ok();
        let tamper_count = findings
            .iter()
            .filter(|f| f.rule_id.starts_with("TB00"))
            .count();
        let error_count = findings
            .iter()
            .filter(|f| f.severity == tb_rules::Severity::Error)
            .count();
        let warning_count = findings
            .iter()
            .filter(|f| f.severity == tb_rules::Severity::Warn)
            .count();
        let duration_ms = start_time.elapsed().as_millis();

        let notif = WebhookNotification {
            repo_name,
            pr_number: pr_num,
            head_ref: args.head.clone(),
            gate_passed: exit_code == 0,
            total_findings: findings.len(),
            error_count,
            warning_count,
            tamper_count,
            duration_ms,
            pr_url,
        };

        if let Err(e) = notif.send(webhook_url, Some(args.webhook_type.into())) {
            eprintln!("{}: Webhook dispatch failed: {:#}", "Warning".yellow(), e);
        } else {
            eprintln!(
                "{}",
                "✔ Webhook notification successfully dispatched".green()
            );
        }
    }

    Ok(exit_code)
}
