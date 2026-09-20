use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Source,
    Test,
    Config,
    Ci,
    Generated,
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    Added,
    Modified,
    Deleted,
    Renamed { from: PathBuf },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    Added,
    Deleted,
    Context,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_lineno: Option<usize>,
    pub new_lineno: Option<usize>,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: PathBuf,
    pub old_path: Option<PathBuf>,
    pub status: DiffStatus,
    pub kind: FileKind,
    pub is_binary: bool,
    pub hunks: Vec<Hunk>,
}

impl FileDiff {
    pub fn added_lines(&self) -> Vec<usize> {
        let mut lines = Vec::new();
        for hunk in &self.hunks {
            for line in &hunk.lines {
                if line.kind == LineKind::Added {
                    lines.extend(line.new_lineno);
                }
            }
        }
        lines
    }

    pub fn deleted_lines(&self) -> Vec<usize> {
        let mut lines = Vec::new();
        for hunk in &self.hunks {
            for line in &hunk.lines {
                if line.kind == LineKind::Deleted {
                    lines.extend(line.old_lineno);
                }
            }
        }
        lines
    }

    pub fn changed_line_ranges_new(&self) -> Vec<(usize, usize)> {
        let mut ranges = Vec::new();
        for hunk in &self.hunks {
            if hunk.new_lines > 0 {
                let start = hunk.new_start;
                let end = hunk.new_start + hunk.new_lines - 1;
                ranges.push((start, end));
            } else if hunk.old_lines > 0 {
                // Pure deletion in old side maps to point in new side
                ranges.push((hunk.new_start, hunk.new_start));
            }
        }
        ranges
    }

    pub fn changed_line_ranges_old(&self) -> Vec<(usize, usize)> {
        let mut ranges = Vec::new();
        for hunk in &self.hunks {
            if hunk.old_lines > 0 {
                let start = hunk.old_start;
                let end = hunk.old_start + hunk.old_lines - 1;
                ranges.push((start, end));
            } else if hunk.new_lines > 0 {
                // Pure addition in new side maps to point in old side
                ranges.push((hunk.old_start, hunk.old_start));
            }
        }
        ranges
    }

    pub fn is_scannable(&self) -> bool {
        !self.is_binary && self.kind != FileKind::Ignored
    }

    /// Reconstructs lines for new side from hunks when the full file is unavailable on disk
    pub fn reconstruct_synthetic_new_source(&self) -> Option<String> {
        if self.hunks.is_empty() {
            return None;
        }
        let max_line = self
            .hunks
            .iter()
            .flat_map(|h| h.lines.iter())
            .filter_map(|l| l.new_lineno)
            .max()?;

        let mut lines = vec![String::new(); max_line];
        for hunk in &self.hunks {
            for line in &hunk.lines {
                if let Some(n) = line.new_lineno.filter(|&n| (1..=max_line).contains(&n)) {
                    lines[n - 1] = line.content.clone();
                }
            }
        }
        Some(lines.join("\n"))
    }

    /// Reconstructs lines for old side from hunks when the full file is unavailable on disk
    pub fn reconstruct_synthetic_old_source(&self) -> Option<String> {
        if self.hunks.is_empty() {
            return None;
        }
        let max_line = self
            .hunks
            .iter()
            .flat_map(|h| h.lines.iter())
            .filter_map(|l| l.old_lineno)
            .max()?;

        let mut lines = vec![String::new(); max_line];
        for hunk in &self.hunks {
            for line in &hunk.lines {
                if let Some(n) = line.old_lineno.filter(|&n| (1..=max_line).contains(&n)) {
                    lines[n - 1] = line.content.clone();
                }
            }
        }
        Some(lines.join("\n"))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffResult {
    pub base_ref: Option<String>,
    pub head_ref: Option<String>,
    pub files: Vec<FileDiff>,
}

impl DiffResult {
    pub fn scannable_files(&self) -> impl Iterator<Item = &FileDiff> {
        self.files.iter().filter(|f| f.is_scannable())
    }

    pub fn filter_by_kind(&self, kind: FileKind) -> impl Iterator<Item = &FileDiff> {
        self.files.iter().filter(move |f| f.kind == kind)
    }

    pub fn find_by_path(&self, path: &Path) -> Option<&FileDiff> {
        self.files
            .iter()
            .find(|f| f.path == path || f.old_path.as_deref() == Some(path))
    }
}
