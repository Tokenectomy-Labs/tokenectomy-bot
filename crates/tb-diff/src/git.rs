use crate::parser::DiffParser;
use crate::types::DiffResult;
use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

pub struct GitExtractor;

impl GitExtractor {
    pub fn extract_diff(repo_dir: &Path, base: &str, head: &str) -> Result<DiffResult> {
        let diff_range = format!("{}...{}", base, head);
        let output = Command::new("git")
            .current_dir(repo_dir)
            .args(["diff", "-U0", "--no-color", "--find-renames", &diff_range])
            .output()
            .with_context(|| format!("Failed to run git diff in {:?}", repo_dir))?;

        if !output.status.success() {
            // Check fallback for two-dot diff if merge-base is unavailable
            let stderr = String::from_utf8_lossy(&output.stderr);
            let two_dot = format!("{}..{}", base, head);
            let fallback_output = Command::new("git")
                .current_dir(repo_dir)
                .args(["diff", "-U0", "--no-color", "--find-renames", &two_dot])
                .output()
                .with_context(|| format!("Fallback git diff failed in {:?}", repo_dir))?;

            if !fallback_output.status.success() {
                let fb_err = String::from_utf8_lossy(&fallback_output.stderr);
                anyhow::bail!(
                    "git diff failed: {}\nFallback failed: {}",
                    stderr.trim(),
                    fb_err.trim()
                );
            }

            let raw_diff = String::from_utf8_lossy(&fallback_output.stdout);
            let mut res = DiffParser::parse(&raw_diff);
            res.base_ref = Some(base.to_string());
            res.head_ref = Some(head.to_string());
            return Ok(res);
        }

        let raw_diff = String::from_utf8_lossy(&output.stdout);
        let mut res = DiffParser::parse(&raw_diff);
        res.base_ref = Some(base.to_string());
        res.head_ref = Some(head.to_string());
        Ok(res)
    }

    pub fn get_blob(repo_dir: &Path, git_ref: &str, file_path: &Path) -> Result<Option<String>> {
        let path_str = file_path.to_string_lossy().replace('\\', "/");
        let obj_spec = format!("{}:{}", git_ref, path_str);

        let output = Command::new("git")
            .current_dir(repo_dir)
            .args(["show", &obj_spec])
            .output()
            .with_context(|| format!("Failed to run git show for {}", obj_spec))?;

        if output.status.success() {
            Ok(Some(String::from_utf8_lossy(&output.stdout).to_string()))
        } else {
            Ok(None)
        }
    }
}
