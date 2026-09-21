use crate::model::AutoFix;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixResult {
    pub original: String,
    pub fixed: String,
    pub applied_count: usize,
}

pub struct CodeFixer;

impl CodeFixer {
    /// Applies a list of AutoFix objects to source text.
    /// Fixes are sorted in descending order of (start_line, start_col)
    /// to prevent offset drifting.
    pub fn apply(source: &str, fixes: &[AutoFix]) -> FixResult {
        if fixes.is_empty() {
            return FixResult {
                original: source.to_string(),
                fixed: source.to_string(),
                applied_count: 0,
            };
        }

        let mut sorted_fixes = fixes.to_vec();
        // Sort descending by line and column
        sorted_fixes.sort_by(|a, b| {
            b.start_line
                .cmp(&a.start_line)
                .then_with(|| b.start_col.cmp(&a.start_col))
        });

        let mut lines: Vec<String> = source.lines().map(|s| s.to_string()).collect();
        let has_trailing_newline = source.ends_with('\n');

        let mut applied_count = 0;
        let mut last_affected_line = usize::MAX;

        for fix in sorted_fixes {
            if fix.start_line == 0 || fix.end_line == 0 {
                continue;
            }
            if fix.end_line >= last_affected_line {
                continue; // Skip overlapping fix to maintain safe AST integrity
            }

            let start_idx = fix.start_line.saturating_sub(1);
            let end_idx = (fix.end_line.saturating_sub(1)).min(lines.len().saturating_sub(1));

            if start_idx >= lines.len() {
                continue;
            }

            // Check if column-level single line replacement
            if fix.start_line == fix.end_line && fix.start_col > 0 && fix.end_col >= fix.start_col {
                let line = &lines[start_idx];
                let col_start = (fix.start_col.saturating_sub(1)).min(line.len());
                let col_end = fix.end_col.min(line.len());

                if col_start <= col_end {
                    let mut new_line = String::new();
                    new_line.push_str(&line[..col_start]);
                    new_line.push_str(&fix.replacement);
                    new_line.push_str(&line[col_end..]);
                    lines[start_idx] = new_line;
                    last_affected_line = fix.start_line;
                    applied_count += 1;
                    continue;
                }
            }

            // Span-based / Block-level replacement
            let prefix = if fix.start_col > 1 && fix.start_col <= lines[start_idx].len() + 1 {
                lines[start_idx][..fix.start_col - 1].to_string()
            } else {
                String::new()
            };

            let suffix = if fix.end_col > 0 && fix.end_col < lines[end_idx].len() {
                lines[end_idx][fix.end_col..].to_string()
            } else {
                String::new()
            };

            let indent = lines[start_idx]
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect::<String>();

            let raw_repl_lines: Vec<&str> = fix.replacement.lines().collect();
            let mut replacement_lines: Vec<String> = Vec::new();

            if raw_repl_lines.is_empty() {
                replacement_lines.push(format!("{}{}{}", prefix, fix.replacement, suffix));
            } else if raw_repl_lines.len() == 1 {
                replacement_lines.push(format!("{}{}{}", prefix, raw_repl_lines[0], suffix));
            } else {
                for (i, &l) in raw_repl_lines.iter().enumerate() {
                    if i == 0 {
                        replacement_lines.push(format!("{}{}", prefix, l));
                    } else if i == raw_repl_lines.len() - 1 {
                        let indented = if !l.trim().is_empty() && !l.starts_with(&indent) {
                            format!("{}{}", indent, l)
                        } else {
                            l.to_string()
                        };
                        replacement_lines.push(format!("{}{}", indented, suffix));
                    } else {
                        let indented = if !l.trim().is_empty() && !l.starts_with(&indent) {
                            format!("{}{}", indent, l)
                        } else {
                            l.to_string()
                        };
                        replacement_lines.push(indented);
                    }
                }
            }

            lines.splice(start_idx..=end_idx, replacement_lines);
            last_affected_line = fix.start_line;
            applied_count += 1;
        }

        let mut result = lines.join("\n");
        if has_trailing_newline && !result.is_empty() {
            result.push('\n');
        }

        FixResult {
            original: source.to_string(),
            fixed: result,
            applied_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_fixer_block_replacement() {
        let code = r#"function run() {
    try {
        doSomething();
    } catch (e) {
    }
}
"#;

        let fix = AutoFix::new(
            "    } catch (e) {\n        console.error(e);\n    }",
            5,
            6,
            0,
            0,
            "Log error instead of empty catch",
        );

        let res = CodeFixer::apply(code, &[fix]);
        assert_eq!(res.applied_count, 1);
        assert!(res.fixed.contains("console.error(e);"));
    }

    #[test]
    fn test_code_fixer_column_replacement() {
        let code = "describe.skip('feature', () => {});\n";
        let fix = AutoFix::new("describe", 1, 1, 1, 13, "Re-enable skipped test block");

        let res = CodeFixer::apply(code, &[fix]);
        assert_eq!(res.applied_count, 1);
        assert_eq!(res.fixed, "describe('feature', () => {});\n");
    }

    #[test]
    fn test_code_fixer_multiple_non_interfering_fixes() {
        let code = r#"line 1
line 2
line 3
line 4
"#;

        let fix1 = AutoFix::new("FIX 2", 2, 2, 0, 0, "fix line 2");
        let fix2 = AutoFix::new("FIX 4", 4, 4, 0, 0, "fix line 4");

        let res = CodeFixer::apply(code, &[fix1, fix2]);
        assert_eq!(res.applied_count, 2);
        assert_eq!(res.fixed, "line 1\nFIX 2\nline 3\nFIX 4\n");
    }
}
