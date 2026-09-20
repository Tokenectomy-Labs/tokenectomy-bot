use std::collections::HashMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use colored::*;

use tb_diff::{DiffParser, GitExtractor};
use tb_report::{
    FailOn, GatePolicy, GitHubAnnotationReporter, JsonReporter, SarifReporter, TextReporter,
};
use tb_rules::RuleEngine;

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
    Review(ReviewArgs),

    /// Initialize default tokenectomy.json configuration
    Init {
        #[arg(short, long, default_value = "tokenectomy.json")]
        output: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
    Sarif,
    Github,
}

#[derive(Parser)]
pub struct ReviewArgs {
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
    #[arg(long, default_value_t = true)]
    fail_open: bool,
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
        Commands::Review(args) => {
            let fail_open = args.fail_open;
            match run_review(args) {
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
        anyhow::bail!("File {:?} already exists. Skipping initialization.", output);
    }

    let template = serde_json::json!({
        "$schema": "https://raw.githubusercontent.com/Tokenectomy-Labs/tokenectomy-bot/main/schema.json",
        "extends": "recommended",
        "fail_on": "error",
        "rules": {
            "TB001": { "enabled": true, "severity": "error" },
            "TB002": { "enabled": true, "severity": "error" },
            "TB003": { "enabled": true, "severity": "error" },
            "TB101": { "enabled": true, "severity": "warn" }
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
    let repo_dir = fs::canonicalize(&args.repo_dir).unwrap_or_else(|_| args.repo_dir.clone());

    let (diff, old_sources, new_sources) = if let Some(ref diff_source) = args.diff_file {
        // Read diff from stdin or file
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
            if let Ok(content) = fs::read_to_string(&full_path) {
                new_map.insert(file.path.clone(), content);
            }
            let old_lookup = file.old_path.as_ref().unwrap_or(&file.path);
            if let Ok(Some(old_content)) = GitExtractor::get_blob(&repo_dir, &args.base, old_lookup) {
                old_map.insert(file.path.clone(), old_content);
            }
        }

        (diff, old_map, new_map)
    } else {
        // Extract diff from Git
        let diff = GitExtractor::extract_diff(&repo_dir, &args.base, &args.head)?;
        let mut old_map = HashMap::new();
        let mut new_map = HashMap::new();

        for file in diff.scannable_files() {
            // Fetch old blob
            let old_lookup = file.old_path.as_ref().unwrap_or(&file.path);
            if let Ok(Some(old_content)) = GitExtractor::get_blob(&repo_dir, &args.base, old_lookup)
            {
                old_map.insert(file.path.clone(), old_content);
            }

            // Fetch new blob
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
            };

            if let Some(content) = new_content {
                new_map.insert(file.path.clone(), content);
            }
        }

        (diff, old_map, new_map)
    };

    let engine = RuleEngine::default();
    let findings = engine.run(&diff, &old_sources, &new_sources);

    let output_str = match args.format {
        OutputFormat::Text => TextReporter::format(&findings),
        OutputFormat::Json => JsonReporter::format(&findings),
        OutputFormat::Sarif => SarifReporter::format(&findings),
        OutputFormat::Github => GitHubAnnotationReporter::format(&findings),
    };

    if let Some(ref out_path) = args.output {
        fs::write(out_path, &output_str)
            .with_context(|| format!("Failed to write report to {:?}", out_path))?;
    } else {
        println!("{}", output_str);
    }

    let exit_code = GatePolicy::evaluate_exit_code(&findings, args.fail_on);
    Ok(exit_code)
}
