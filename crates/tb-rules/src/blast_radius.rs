use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tb_diff::DiffResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::Low => write!(f, "LOW"),
            RiskLevel::Medium => write!(f, "MEDIUM"),
            RiskLevel::High => write!(f, "HIGH"),
            RiskLevel::Critical => write!(f, "CRITICAL"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastRadiusReport {
    pub risk_score: u32,
    pub risk_level: RiskLevel,
    pub modified_files: Vec<PathBuf>,
    pub direct_dependents: Vec<PathBuf>,
    pub indirect_dependents: Vec<PathBuf>,
    pub sensitive_paths_affected: Vec<PathBuf>,
    pub mermaid_diagram: String,
}

pub struct BlastRadiusAnalyzer;

impl BlastRadiusAnalyzer {
    pub fn analyze(
        diff: &DiffResult,
        all_sources: &HashMap<PathBuf, String>,
        repo_dir: Option<&Path>,
    ) -> BlastRadiusReport {
        let mut merged_sources = all_sources.clone();
        if let Some(r) = repo_dir {
            let scanned = Self::scan_repo_sources(r, 2000);
            for (k, v) in scanned {
                merged_sources.entry(k).or_insert(v);
            }
        }
        let all_sources = &merged_sources;

        let mut modified_files = Vec::new();
        for file in diff.scannable_files() {
            modified_files.push(file.path.clone());
        }

        let mut direct_dependents_set = HashSet::new();
        let mut callers_map: HashMap<PathBuf, HashSet<PathBuf>> = HashMap::new();

        // 1. Map direct dependents for each modified file
        for mod_path in &modified_files {
            let mod_stem = mod_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let mod_norm = mod_path.to_string_lossy().replace('\\', "/");

            for (other_path, other_content) in all_sources {
                if other_path == mod_path {
                    continue;
                }

                // Check if other file references or imports the modified file
                let is_dependent = Self::references_file(other_content, mod_stem, &mod_norm);
                if is_dependent {
                    direct_dependents_set.insert(other_path.clone());
                    callers_map
                        .entry(mod_path.clone())
                        .or_default()
                        .insert(other_path.clone());
                }
            }
        }

        // 2. Map indirect (transitive) dependents
        let mut indirect_dependents_set = HashSet::new();
        for direct_path in &direct_dependents_set {
            let direct_stem = direct_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let direct_norm = direct_path.to_string_lossy().replace('\\', "/");

            for (trans_path, trans_content) in all_sources {
                if trans_path == direct_path
                    || direct_dependents_set.contains(trans_path)
                    || modified_files.contains(trans_path)
                {
                    continue;
                }

                if Self::references_file(trans_content, direct_stem, &direct_norm) {
                    indirect_dependents_set.insert(trans_path.clone());
                    callers_map
                        .entry(direct_path.clone())
                        .or_default()
                        .insert(trans_path.clone());
                }
            }
        }

        // 3. Sensitive path auditing
        let mut sensitive_paths_affected = Vec::new();
        let all_affected: Vec<&PathBuf> = modified_files
            .iter()
            .chain(direct_dependents_set.iter())
            .chain(indirect_dependents_set.iter())
            .collect();

        for p in all_affected {
            let norm = p.to_string_lossy().to_lowercase();
            let is_sensitive = norm.contains("auth")
                || norm.contains("payment")
                || norm.contains("billing")
                || norm.contains("crypto")
                || norm.contains("security")
                || norm.contains("wallet")
                || norm.contains("core");

            if is_sensitive && !sensitive_paths_affected.contains(p) {
                sensitive_paths_affected.push(p.clone());
            }
        }

        // 4. Calculate Risk Score (0 - 100)
        let base_score = direct_dependents_set.len() * 18 + indirect_dependents_set.len() * 8;
        let sensitive_bonus = if !sensitive_paths_affected.is_empty() {
            25
        } else {
            0
        };
        let risk_score = ((base_score + sensitive_bonus) as u32).min(100);

        let risk_level = match risk_score {
            0..=25 => RiskLevel::Low,
            26..=60 => RiskLevel::Medium,
            61..=85 => RiskLevel::High,
            _ => RiskLevel::Critical,
        };

        // 5. Generate Mermaid Diagram
        let direct_list: Vec<PathBuf> = direct_dependents_set.into_iter().collect();
        let indirect_list: Vec<PathBuf> = indirect_dependents_set.into_iter().collect();

        let mermaid_diagram = Self::generate_mermaid(
            &modified_files,
            &direct_list,
            &indirect_list,
            &callers_map,
            risk_score,
            risk_level,
        );

        BlastRadiusReport {
            risk_score,
            risk_level,
            modified_files,
            direct_dependents: direct_list,
            indirect_dependents: indirect_list,
            sensitive_paths_affected,
            mermaid_diagram,
        }
    }

    fn scan_repo_sources(repo_dir: &Path, limit: usize) -> HashMap<PathBuf, String> {
        let mut map = HashMap::new();
        let mut queue = vec![repo_dir.to_path_buf()];
        while let Some(dir) = queue.pop() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    if file_name.starts_with('.')
                        || file_name == "node_modules"
                        || file_name == "target"
                        || file_name == "dist"
                        || file_name == "build"
                    {
                        continue;
                    }
                    if path.is_dir() {
                        queue.push(path);
                    } else if path.is_file() {
                        let is_source_ext =
                            path.extension()
                                .and_then(|e| e.to_str())
                                .is_some_and(|ext| {
                                    matches!(ext, "ts" | "tsx" | "js" | "jsx" | "py" | "rs" | "go")
                                });

                        if !is_source_ext {
                            continue;
                        }

                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let rel = path.strip_prefix(repo_dir).unwrap_or(&path).to_path_buf();
                            map.insert(rel, content);
                            if map.len() >= limit {
                                return map;
                            }
                        }
                    }
                }
            }
        }
        map
    }

    fn references_file(content: &str, file_stem: &str, full_path: &str) -> bool {
        if file_stem.is_empty() {
            return false;
        }

        for line in content.lines() {
            let trimmed = line.trim();
            if (trimmed.starts_with("import ")
                || trimmed.starts_with("from ")
                || trimmed.contains("require("))
                && (trimmed.contains(file_stem) || trimmed.contains(full_path))
            {
                return true;
            }
        }
        false
    }

    fn sanitize_node_id(path: &Path) -> String {
        path.to_string_lossy()
            .replace(['/', '\\', '.', '-', '@', ':'], "_")
    }

    fn generate_mermaid(
        modified: &[PathBuf],
        direct: &[PathBuf],
        indirect: &[PathBuf],
        callers: &HashMap<PathBuf, HashSet<PathBuf>>,
        risk_score: u32,
        risk_level: RiskLevel,
    ) -> String {
        let mut out = String::new();
        out.push_str("```mermaid\n");
        out.push_str("graph TD\n");
        out.push_str("  classDef mod fill:#da3633,stroke:#b62324,color:#fff,stroke-width:2px;\n");
        out.push_str("  classDef dir fill:#d29922,stroke:#bb8009,color:#fff,stroke-width:2px;\n");
        out.push_str("  classDef ind fill:#58a6ff,stroke:#388bfd,color:#fff,stroke-width:2px;\n\n");

        out.push_str(&format!(
            "  subgraph BlastRadius [\"Blast Radius: {} Risk (Score: {}/100)\"]\n",
            risk_level, risk_score
        ));

        // Define nodes
        for m in modified {
            let id = Self::sanitize_node_id(m);
            let label = m.to_string_lossy();
            out.push_str(&format!("    {}[\"{} (Modified)\"]:::mod\n", id, label));
        }

        for d in direct {
            let id = Self::sanitize_node_id(d);
            let label = d.to_string_lossy();
            out.push_str(&format!("    {}[\"{} (Direct)\"]:::dir\n", id, label));
        }

        for i in indirect {
            let id = Self::sanitize_node_id(i);
            let label = i.to_string_lossy();
            out.push_str(&format!("    {}[\"{} (Indirect)\"]:::ind\n", id, label));
        }

        // Define edges
        for (source, targets) in callers {
            let src_id = Self::sanitize_node_id(source);
            for tgt in targets {
                let tgt_id = Self::sanitize_node_id(tgt);
                out.push_str(&format!("    {} --> {}\n", src_id, tgt_id));
            }
        }

        out.push_str("  end\n");
        out.push_str("```\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tb_diff::{DiffStatus, FileDiff, FileKind, Hunk};

    #[test]
    fn test_blast_radius_calculation_and_diagram() {
        let mut sources = HashMap::new();
        let payment_path = PathBuf::from("src/payment.ts");
        let order_path = PathBuf::from("src/order.ts");
        let api_path = PathBuf::from("src/api/checkout.ts");
        let unrelated_path = PathBuf::from("src/readme.txt");

        sources.insert(
            payment_path.clone(),
            "export function processPayment() {}".to_string(),
        );
        sources.insert(
            order_path.clone(),
            "import { processPayment } from './payment';\nexport function createOrder() {}"
                .to_string(),
        );
        sources.insert(
            api_path.clone(),
            "import { createOrder } from '../order';\nexport function checkoutApi() {}".to_string(),
        );
        sources.insert(unrelated_path.clone(), "Documentation".to_string());

        let diff = DiffResult {
            base_ref: None,
            head_ref: None,
            files: vec![FileDiff {
                path: payment_path.clone(),
                old_path: None,
                status: DiffStatus::Modified,
                kind: FileKind::Source,
                is_binary: false,
                hunks: vec![Hunk {
                    old_start: 1,
                    old_lines: 1,
                    new_start: 1,
                    new_lines: 1,
                    lines: vec![],
                }],
            }],
        };

        let report = BlastRadiusAnalyzer::analyze(&diff, &sources, None);

        assert_eq!(report.modified_files.len(), 1);
        assert_eq!(report.direct_dependents.len(), 1);
        assert_eq!(report.direct_dependents[0], order_path);
        assert_eq!(report.indirect_dependents.len(), 1);
        assert_eq!(report.indirect_dependents[0], api_path);
        assert!(report.sensitive_paths_affected.contains(&payment_path));
        assert!(report.risk_score >= 40); // 18 (direct) + 8 (indirect) + 25 (sensitive payment) = 51
        assert_eq!(report.risk_level, RiskLevel::Medium);
        assert!(report.mermaid_diagram.contains("```mermaid"));
        assert!(
            report
                .mermaid_diagram
                .contains("src_payment_ts --> src_order_ts")
        );
    }
}
