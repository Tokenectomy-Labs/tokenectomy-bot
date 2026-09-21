use crate::model::{Confidence, Finding, RuleContext, Severity};
use crate::traits::Rule;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tb_parse::ParsedSource;

pub struct Tb301PhantomSymbol;

impl Rule for Tb301PhantomSymbol {
    fn id(&self) -> &'static str {
        "TB301"
    }

    fn name(&self) -> &'static str {
        "phantom-symbol"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, ctx: &RuleContext) -> Vec<Finding> {
        let mut findings = Vec::new();

        let new_parsed = match ctx.new_parsed {
            Some(p) => p,
            None => return findings,
        };

        let all_sources = match ctx.all_sources {
            Some(s) if !s.is_empty() => s,
            _ => return findings,
        };

        let changed_ranges = ctx.file_diff.changed_line_ranges_new();
        if changed_ranges.is_empty() {
            return findings;
        }

        let mut disk_cache = std::collections::HashMap::new();
        let current_dir = ctx.file_diff.path.parent().unwrap_or(Path::new(""));

        // 1. Check JavaScript / TypeScript imports
        let import_statements = new_parsed.find_all_descendants(new_parsed.root_node(), &|node| {
            node.kind() == "import_statement"
        });

        for stmt in import_statements {
            if !ParsedSource::node_overlaps_ranges(&stmt, &changed_ranges) {
                continue;
            }

            let source_node = (0..stmt.child_count())
                .filter_map(|i| stmt.child(i))
                .find(|c| c.kind() == "string");

            let source_str = match source_node {
                Some(n) => new_parsed.node_text(&n).trim_matches(['\'', '"', '`']),
                None => continue,
            };

            // Only check relative project imports (starting with ./ or ../)
            if !source_str.starts_with("./") && !source_str.starts_with("../") {
                continue;
            }

            let candidate_target = resolve_relative_path(current_dir, source_str);

            // Find matching file in all_sources or on disk
            let matched_entry = find_matching_source(
                &candidate_target,
                all_sources,
                ctx.repo_dir,
                &mut disk_cache,
            );

            let (start_line, end_line) = ParsedSource::node_line_range(&stmt);
            let (start_col, _) = ParsedSource::point_to_1indexed(stmt.start_position());
            let (_, end_col) = ParsedSource::point_to_1indexed(stmt.end_position());

            match matched_entry {
                None => {
                    // Entire module is a phantom hallucination
                    findings.push(Finding::new(
                        self.id(),
                        self.name(),
                        self.default_severity(),
                        Confidence::High,
                        &ctx.file_diff.path,
                        start_line,
                        end_line,
                        start_col,
                        end_col,
                        format!(
                            "Phantom module detected (`{}`). Module does not exist in repository — AI agent hallucinated an uncreated file.",
                            source_str
                        ),
                        Some("Verify file path or create the referenced module before importing it.".to_string()),
                        stmt.kind(),
                        new_parsed.node_text(&stmt),
                    ));
                }
                Some((target_path, target_content)) => {
                    // Target file exists. Now verify imported named symbols!
                    let imported_symbols = extract_imported_symbols(new_parsed, &stmt);
                    if imported_symbols.is_empty() {
                        continue;
                    }

                    let exported_symbols = extract_exported_symbols(target_path, target_content);

                    for sym in imported_symbols {
                        // Skip default, type, or wildcard imports
                        if sym == "default" || sym == "*" {
                            continue;
                        }

                        if !exported_symbols.contains(&sym) {
                            findings.push(Finding::new(
                                self.id(),
                                self.name(),
                                self.default_severity(),
                                Confidence::High,
                                &ctx.file_diff.path,
                                start_line,
                                end_line,
                                start_col,
                                end_col,
                                format!(
                                    "Phantom symbol detected: `{}` is not exported by `{}`. AI agent hallucinated a non-existent function or variable.",
                                    sym, source_str
                                ),
                                Some(format!(
                                    "Export `{}` from `{}` or use one of the available exports: [{}].",
                                    sym,
                                    target_path.display(),
                                    exported_symbols.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
                                )),
                                stmt.kind(),
                                new_parsed.node_text(&stmt),
                            ));
                        }
                    }
                }
            }
        }

        // 2. Check Python relative imports (`from .module import symbol`)
        let python_from_imports = new_parsed
            .find_all_descendants(new_parsed.root_node(), &|node| {
                node.kind() == "import_from_statement"
            });

        for stmt in python_from_imports {
            if !ParsedSource::node_overlaps_ranges(&stmt, &changed_ranges) {
                continue;
            }

            let text = new_parsed.node_text(&stmt).trim();
            // e.g. "from .utils import helper, calculate"
            if let Some(rest) = text.strip_prefix("from ") {
                let parts: Vec<&str> = rest.split(" import ").collect();
                if parts.len() == 2 {
                    let mod_path = parts[0].trim();
                    if mod_path.starts_with('.') {
                        let rel_name = mod_path.trim_start_matches('.');
                        let candidate = current_dir.join(rel_name);
                        let matched = find_matching_source(
                            &candidate,
                            all_sources,
                            ctx.repo_dir,
                            &mut disk_cache,
                        );

                        let (start_line, end_line) = ParsedSource::node_line_range(&stmt);
                        let (start_col, _) = ParsedSource::point_to_1indexed(stmt.start_position());
                        let (_, end_col) = ParsedSource::point_to_1indexed(stmt.end_position());

                        match matched {
                            None => {
                                findings.push(Finding::new(
                                    self.id(),
                                    self.name(),
                                    self.default_severity(),
                                    Confidence::High,
                                    &ctx.file_diff.path,
                                    start_line,
                                    end_line,
                                    start_col,
                                    end_col,
                                    format!(
                                        "Phantom Python module detected (`{}`). Module does not exist in repository.",
                                        mod_path
                                    ),
                                    Some("Verify Python module name or create the referenced file.".to_string()),
                                    stmt.kind(),
                                    text,
                                ));
                            }
                            Some((target_path, target_content)) => {
                                let imported_names: Vec<&str> =
                                    parts[1].split(',').map(|s| s.trim()).collect();
                                let exported = extract_python_exports(target_content);
                                for name in imported_names {
                                    if !name.is_empty() && name != "*" && !exported.contains(name) {
                                        findings.push(Finding::new(
                                            self.id(),
                                            self.name(),
                                            self.default_severity(),
                                            Confidence::High,
                                            &ctx.file_diff.path,
                                            start_line,
                                            end_line,
                                            start_col,
                                            end_col,
                                            format!(
                                                "Phantom Python symbol detected: `{}` is not defined in `{}`.",
                                                name, mod_path
                                            ),
                                            Some(format!("Define `{}` in `{}` before importing it.", name, target_path.display())),
                                            stmt.kind(),
                                            text,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        findings
    }
}

fn resolve_relative_path(base_dir: &Path, relative: &str) -> PathBuf {
    let joined = base_dir.join(relative);
    let mut normalized = PathBuf::new();
    for comp in joined.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            c => normalized.push(c),
        }
    }
    normalized
}

fn find_matching_source<'a>(
    candidate_base: &Path,
    all_sources: &'a std::collections::HashMap<PathBuf, String>,
    repo_dir: Option<&Path>,
    disk_cache: &'a mut std::collections::HashMap<PathBuf, String>,
) -> Option<(&'a PathBuf, &'a str)> {
    let variations = [
        candidate_base.to_path_buf(),
        candidate_base.with_extension("ts"),
        candidate_base.with_extension("tsx"),
        candidate_base.with_extension("js"),
        candidate_base.with_extension("jsx"),
        candidate_base.with_extension("py"),
        candidate_base.join("index.ts"),
        candidate_base.join("index.tsx"),
        candidate_base.join("index.js"),
        candidate_base.join("index.jsx"),
        candidate_base.join("__init__.py"),
    ];

    for var in &variations {
        let norm_var = var.to_string_lossy().replace('\\', "/");
        for (path, content) in all_sources {
            let norm_path = path.to_string_lossy().replace('\\', "/");
            if norm_path == norm_var || norm_path.ends_with(&norm_var) {
                return Some((path, content.as_str()));
            }
        }
    }

    if let Some(r) = repo_dir {
        for var in &variations {
            let full = r.join(var);
            if !full.is_file() {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&full) {
                disk_cache.insert(var.clone(), content);
                let (k, v) = disk_cache.get_key_value(var).unwrap();
                return Some((k, v.as_str()));
            }
        }
    }

    None
}

fn extract_imported_symbols(parsed: &ParsedSource, import_stmt: &tree_sitter::Node) -> Vec<String> {
    let mut symbols = Vec::new();
    let named_nodes =
        parsed.find_all_descendants(*import_stmt, &|node| node.kind() == "import_specifier");

    for spec in named_nodes {
        let text = parsed.node_text(&spec).trim();
        // Handle "foo as bar" -> imported symbol is foo
        let actual_symbol = if let Some(first) = text.split(" as ").next() {
            first.trim()
        } else {
            text
        };
        if !actual_symbol.is_empty() {
            symbols.push(actual_symbol.to_string());
        }
    }

    symbols
}

fn extract_exported_symbols(path: &Path, content: &str) -> HashSet<String> {
    let mut exports = HashSet::new();

    if let Ok(parsed) = ParsedSource::parse(path, content.to_string()) {
        let export_nodes = parsed.find_all_descendants(parsed.root_node(), &|node| {
            node.kind() == "export_statement"
        });

        for exp in export_nodes {
            let text = parsed.node_text(&exp);

            // Pattern: export function foo
            // Pattern: export const foo, export let foo, export var foo
            // Pattern: export class foo, export interface foo, export type foo, export enum foo
            // Pattern: export { foo, bar as baz }
            for line in text.lines() {
                let trimmed = line.trim();
                let clean = trimmed.strip_prefix("export ").unwrap_or(trimmed);

                if clean.starts_with("function ") || clean.starts_with("async function ") {
                    let fn_part = clean
                        .strip_prefix("async function ")
                        .unwrap_or_else(|| clean.strip_prefix("function ").unwrap_or(clean));
                    if let Some(name) = fn_part.split('(').next() {
                        let sym = name.trim();
                        if !sym.is_empty() {
                            exports.insert(sym.to_string());
                        }
                    }
                } else if clean.starts_with("const ")
                    || clean.starts_with("let ")
                    || clean.starts_with("var ")
                {
                    let var_part = clean.split_whitespace().nth(1).unwrap_or("");
                    let sym = var_part
                        .split(':')
                        .next()
                        .unwrap_or(var_part)
                        .split('=')
                        .next()
                        .unwrap_or(var_part)
                        .trim();
                    if !sym.is_empty() {
                        exports.insert(sym.to_string());
                    }
                } else if clean.starts_with("class ")
                    || clean.starts_with("interface ")
                    || clean.starts_with("type ")
                    || clean.starts_with("enum ")
                {
                    if let Some(name) = clean.split_whitespace().nth(1) {
                        let sym = name
                            .split('{')
                            .next()
                            .unwrap_or(name)
                            .split('<')
                            .next()
                            .unwrap_or(name)
                            .trim();
                        if !sym.is_empty() {
                            exports.insert(sym.to_string());
                        }
                    }
                } else if clean.starts_with('{') {
                    // export { foo, bar as baz }
                    let inner = clean.trim_matches(['{', '}', ';']).trim();
                    for part in inner.split(',') {
                        let sym = part.split(" as ").next().unwrap_or(part).trim();
                        if !sym.is_empty() {
                            exports.insert(sym.to_string());
                        }
                    }
                }
            }
        }
    } else {
        // Fallback simple line scanning
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(clean) = trimmed.strip_prefix("export ") {
                let parts: Vec<&str> = clean.split_whitespace().collect();
                if parts.len() >= 2 {
                    let sym = parts[1].trim_matches(['(', '{', ':', ';', '=']);
                    if !sym.is_empty() {
                        exports.insert(sym.to_string());
                    }
                }
            }
        }
    }

    exports
}

fn extract_python_exports(content: &str) -> HashSet<String> {
    let mut symbols = HashSet::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("def ") {
            if let Some(name) = rest.split('(').next() {
                symbols.insert(name.trim().to_string());
            }
        } else if let Some(rest) = trimmed.strip_prefix("class ") {
            if let Some(name) = rest.split('(').next().or_else(|| rest.split(':').next()) {
                symbols.insert(name.trim().to_string());
            }
        } else if !trimmed.starts_with('#') && trimmed.contains(" = ") {
            let var = trimmed.split(" = ").next().unwrap_or("").trim();
            if !var.is_empty() {
                symbols.insert(var.to_string());
            }
        }
    }
    symbols
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tb_diff::{DiffLine, DiffStatus, FileDiff, FileKind, Hunk, LineKind};

    #[test]
    fn test_tb301_detects_non_existent_module() {
        let code = r#"
            import { calculateTax } from './finance';
        "#
        .to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("src/checkout.ts"), code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("src/checkout.ts"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 1,
                old_lines: 0,
                new_start: 2,
                new_lines: 1,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(2),
                    content: "            import { calculateTax } from './finance';".to_string(),
                }],
            }],
        };

        let mut sources = HashMap::new();
        sources.insert(PathBuf::from("src/checkout.ts"), "code".to_string());
        // Notice: src/finance.ts is NOT in sources!

        let mut ctx = RuleContext::new(&file_diff, None, Some(&parsed));
        ctx.all_sources = Some(&sources);

        let rule = Tb301PhantomSymbol;
        let findings = rule.check(&ctx);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB301");
        assert!(findings[0].message.contains("Phantom module detected"));
    }

    #[test]
    fn test_tb301_detects_hallucinated_symbol_in_existing_module() {
        let consumer_code = r#"
            import { getValidDiscount, hallucinatedSecretMethod } from './discounts';
        "#
        .to_string();

        let discounts_code = r#"
            export function getValidDiscount(code: string) {
                return 10;
            }
            export const MAX_DISCOUNT = 50;
        "#
        .to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("src/cart.ts"), consumer_code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("src/cart.ts"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 1,
                old_lines: 0,
                new_start: 2,
                new_lines: 1,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(2),
                    content: "            import { getValidDiscount, hallucinatedSecretMethod } from './discounts';".to_string(),
                }],
            }],
        };

        let mut sources = HashMap::new();
        sources.insert(PathBuf::from("src/cart.ts"), "consumer".to_string());
        sources.insert(PathBuf::from("src/discounts.ts"), discounts_code);

        let mut ctx = RuleContext::new(&file_diff, None, Some(&parsed));
        ctx.all_sources = Some(&sources);

        let rule = Tb301PhantomSymbol;
        let findings = rule.check(&ctx);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TB301");
        assert!(findings[0].message.contains("hallucinatedSecretMethod"));
        assert!(findings[0].message.contains("Phantom symbol detected"));
    }

    #[test]
    fn test_tb301_passes_valid_exports() {
        let consumer_code = r#"
            import { getValidDiscount } from './discounts';
        "#
        .to_string();

        let discounts_code = r#"
            export function getValidDiscount(code: string) {
                return 10;
            }
        "#
        .to_string();

        let parsed = ParsedSource::parse(&PathBuf::from("src/cart.ts"), consumer_code).unwrap();
        let file_diff = FileDiff {
            path: PathBuf::from("src/cart.ts"),
            old_path: None,
            status: DiffStatus::Modified,
            kind: FileKind::Source,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 1,
                old_lines: 0,
                new_start: 2,
                new_lines: 1,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(2),
                    content: "            import { getValidDiscount } from './discounts';"
                        .to_string(),
                }],
            }],
        };

        let mut sources = HashMap::new();
        sources.insert(PathBuf::from("src/cart.ts"), "consumer".to_string());
        sources.insert(PathBuf::from("src/discounts.ts"), discounts_code);

        let mut ctx = RuleContext::new(&file_diff, None, Some(&parsed));
        ctx.all_sources = Some(&sources);

        let rule = Tb301PhantomSymbol;
        let findings = rule.check(&ctx);

        assert!(findings.is_empty());
    }
}
