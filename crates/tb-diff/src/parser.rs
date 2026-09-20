use crate::classifier::classify_path;
use crate::types::{DiffLine, DiffResult, DiffStatus, FileDiff, Hunk, LineKind};
use regex::Regex;
use std::path::PathBuf;

pub struct DiffParser;

impl DiffParser {
    pub fn parse(raw_diff: &str) -> DiffResult {
        let mut files = Vec::new();
        let lines: Vec<&str> = raw_diff.lines().collect();
        let mut i = 0;

        let hunk_header_re =
            Regex::new(r"^@@\s+-(\d+)(?:,(\d+))?\s+\+(\d+)(?:,(\d+))?\s+@@").expect("valid regex");

        while i < lines.len() {
            let line = lines[i];

            if line.starts_with("diff --git ") {
                let (file_diff, next_idx) = Self::parse_file_diff(&lines, i, &hunk_header_re);
                files.push(file_diff);
                i = next_idx;
            } else {
                i += 1;
            }
        }

        DiffResult {
            base_ref: None,
            head_ref: None,
            files,
        }
    }

    fn parse_file_diff(lines: &[&str], start_idx: usize, hunk_re: &Regex) -> (FileDiff, usize) {
        let header = lines[start_idx];
        let (raw_old_path, raw_new_path) = Self::extract_paths_from_diff_git(header);

        let mut old_path = raw_old_path;
        let mut new_path = raw_new_path;
        let mut status = DiffStatus::Modified;
        let mut is_binary = false;
        let mut hunks = Vec::new();

        let mut i = start_idx + 1;

        while i < lines.len() && !lines[i].starts_with("diff --git ") {
            let line = lines[i];

            if line.starts_with("new file mode ") {
                status = DiffStatus::Added;
                i += 1;
            } else if line.starts_with("deleted file mode ") {
                status = DiffStatus::Deleted;
                i += 1;
            } else if line.starts_with("rename from ") {
                let from = line.trim_start_matches("rename from ").trim();
                old_path = Some(PathBuf::from(from));
                i += 1;
            } else if line.starts_with("rename to ") {
                let to = line.trim_start_matches("rename to ").trim();
                let target = PathBuf::from(to);
                new_path = Some(target.clone());
                if let Some(ref from) = old_path {
                    status = DiffStatus::Renamed { from: from.clone() };
                }
                i += 1;
            } else if line.starts_with("Binary files ") && line.contains("differ") {
                is_binary = true;
                i += 1;
            } else if line.starts_with("--- ") {
                let path_str = line.trim_start_matches("--- ").trim();
                if path_str == "/dev/null" {
                    status = DiffStatus::Added;
                    old_path = None;
                } else if let Some(rest) = path_str.strip_prefix("a/") {
                    old_path = Some(PathBuf::from(rest));
                }
                i += 1;
            } else if line.starts_with("+++ ") {
                let path_str = line.trim_start_matches("+++ ").trim();
                if path_str == "/dev/null" {
                    status = DiffStatus::Deleted;
                    new_path = None;
                } else if let Some(rest) = path_str.strip_prefix("b/") {
                    new_path = Some(PathBuf::from(rest));
                }
                i += 1;
            } else if line.starts_with("@@ ") {
                // Parse Hunk
                let (hunk, next_i) = Self::parse_hunk(lines, i, hunk_re);
                hunks.push(hunk);
                i = next_i;
            } else {
                i += 1;
            }
        }

        let primary_path = match status {
            DiffStatus::Deleted => old_path.clone().unwrap_or_else(|| PathBuf::from("deleted")),
            _ => new_path
                .clone()
                .unwrap_or_else(|| old_path.clone().unwrap_or_else(|| PathBuf::from("unknown"))),
        };

        let kind = classify_path(&primary_path);

        (
            FileDiff {
                path: primary_path,
                old_path,
                status,
                kind,
                is_binary,
                hunks,
            },
            i,
        )
    }

    fn extract_paths_from_diff_git(header: &str) -> (Option<PathBuf>, Option<PathBuf>) {
        let remainder = header.trim_start_matches("diff --git ").trim();
        let parts: Vec<&str> = remainder.split(" b/").collect();
        if parts.len() == 2 {
            let a_part = parts[0].trim_start_matches("a/").trim_matches('"');
            let b_part = parts[1].trim_matches('"');
            (Some(PathBuf::from(a_part)), Some(PathBuf::from(b_part)))
        } else {
            (None, None)
        }
    }

    fn parse_hunk(lines: &[&str], start_idx: usize, hunk_re: &Regex) -> (Hunk, usize) {
        let header = lines[start_idx];
        let caps = hunk_re.captures(header);

        let (old_start, old_lines, new_start, new_lines) = if let Some(c) = caps {
            let os = c.get(1).map_or(1, |m| m.as_str().parse().unwrap_or(1));
            let ol = c.get(2).map_or(1, |m| m.as_str().parse().unwrap_or(1));
            let ns = c.get(3).map_or(1, |m| m.as_str().parse().unwrap_or(1));
            let nl = c.get(4).map_or(1, |m| m.as_str().parse().unwrap_or(1));
            (os, ol, ns, nl)
        } else {
            (1, 0, 1, 0)
        };

        let mut diff_lines = Vec::new();
        let mut curr_old = old_start;
        let mut curr_new = new_start;

        let mut i = start_idx + 1;
        while i < lines.len() {
            let line = lines[i];
            if line.starts_with("diff --git ") || line.starts_with("@@ ") {
                break;
            }

            if let Some(rest) = line.strip_prefix('+') {
                diff_lines.push(DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(curr_new),
                    content: rest.trim_end_matches('\r').to_string(),
                });
                curr_new += 1;
            } else if let Some(rest) = line.strip_prefix('-') {
                diff_lines.push(DiffLine {
                    kind: LineKind::Deleted,
                    old_lineno: Some(curr_old),
                    new_lineno: None,
                    content: rest.trim_end_matches('\r').to_string(),
                });
                curr_old += 1;
            } else if let Some(rest) = line.strip_prefix(' ') {
                diff_lines.push(DiffLine {
                    kind: LineKind::Context,
                    old_lineno: Some(curr_old),
                    new_lineno: Some(curr_new),
                    content: rest.trim_end_matches('\r').to_string(),
                });
                curr_old += 1;
                curr_new += 1;
            } else if line == "\\ No newline at end of file" {
                // Ignore git diff marker
            } else {
                // Stop if unexpected line
                break;
            }
            i += 1;
        }

        (
            Hunk {
                old_start,
                old_lines,
                new_start,
                new_lines,
                lines: diff_lines,
            },
            i,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_unified_diff() {
        let diff = r#"diff --git a/src/service.ts b/src/service.ts
index e69de29..d95f3ad 100644
--- a/src/service.ts
+++ b/src/service.ts
@@ -10,2 +10,3 @@
 const a = 1;
-const b = 2;
+const b = 3;
+const c = 4;
"#;
        let res = DiffParser::parse(diff);
        assert_eq!(res.files.len(), 1);
        let f = &res.files[0];
        assert_eq!(f.path, Path::new("src/service.ts"));
        assert_eq!(f.hunks.len(), 1);
        let h = &f.hunks[0];
        assert_eq!(h.old_start, 10);
        assert_eq!(h.old_lines, 2);
        assert_eq!(h.new_start, 10);
        assert_eq!(h.new_lines, 3);

        assert_eq!(f.added_lines(), vec![11, 12]);
        assert_eq!(f.deleted_lines(), vec![11]);
    }

    #[test]
    fn test_parse_rename() {
        let diff = r#"diff --git a/old_name.ts b/new_name.ts
similarity index 100%
rename from old_name.ts
rename to new_name.ts
"#;
        let res = DiffParser::parse(diff);
        assert_eq!(res.files.len(), 1);
        let f = &res.files[0];
        assert_eq!(f.path, Path::new("new_name.ts"));
        assert_eq!(f.old_path, Some(PathBuf::from("old_name.ts")));
        match &f.status {
            DiffStatus::Renamed { from } => assert_eq!(from, &PathBuf::from("old_name.ts")),
            _ => panic!("expected Renamed status"),
        }
    }
}
