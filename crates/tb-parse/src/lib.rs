use anyhow::{Result, bail};
use std::path::Path;
use tree_sitter::{Language, Node, Parser, Point, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SupportedLanguage {
    TypeScript,
    Tsx,
    JavaScript,
    Jsx,
    Rust,
    Python,
    Go,
}

impl SupportedLanguage {
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().trim() {
            "ts" | "typescript" => Some(SupportedLanguage::TypeScript),
            "tsx" => Some(SupportedLanguage::Tsx),
            "js" | "javascript" => Some(SupportedLanguage::JavaScript),
            "jsx" => Some(SupportedLanguage::Jsx),
            "rs" | "rust" => Some(SupportedLanguage::Rust),
            "py" | "python" => Some(SupportedLanguage::Python),
            "go" | "golang" => Some(SupportedLanguage::Go),
            _ => None,
        }
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        match ext {
            "ts" | "mts" | "cts" => Some(SupportedLanguage::TypeScript),
            "tsx" => Some(SupportedLanguage::Tsx),
            "js" | "mjs" | "cjs" => Some(SupportedLanguage::JavaScript),
            "jsx" => Some(SupportedLanguage::Jsx),
            "rs" => Some(SupportedLanguage::Rust),
            "py" | "pyi" => Some(SupportedLanguage::Python),
            "go" => Some(SupportedLanguage::Go),
            _ => None,
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            SupportedLanguage::TypeScript => "ts",
            SupportedLanguage::Tsx => "tsx",
            SupportedLanguage::JavaScript => "js",
            SupportedLanguage::Jsx => "jsx",
            SupportedLanguage::Rust => "rs",
            SupportedLanguage::Python => "py",
            SupportedLanguage::Go => "go",
        }
    }

    pub fn tree_sitter_language(&self) -> Language {
        match self {
            SupportedLanguage::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            SupportedLanguage::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            SupportedLanguage::JavaScript | SupportedLanguage::Jsx => {
                tree_sitter_javascript::LANGUAGE.into()
            }
            SupportedLanguage::Rust => tree_sitter_rust::LANGUAGE.into(),
            SupportedLanguage::Python => tree_sitter_python::LANGUAGE.into(),
            SupportedLanguage::Go => tree_sitter_go::LANGUAGE.into(),
        }
    }
}

pub struct ParsedSource {
    pub language: SupportedLanguage,
    pub source: String,
    pub tree: Tree,
}

impl ParsedSource {
    pub fn parse(path: &Path, source: String) -> Result<Self> {
        let language = match SupportedLanguage::from_path(path) {
            Some(l) => l,
            None => bail!("Unsupported language for path: {:?}", path),
        };

        let mut parser = Parser::new();
        let ts_lang = language.tree_sitter_language();
        parser
            .set_language(&ts_lang)
            .map_err(|e| anyhow::anyhow!("Failed to set language: {:?}", e))?;

        let tree = parser
            .parse(&source, None)
            .ok_or_else(|| anyhow::anyhow!("Tree-sitter parse failed for {:?}", path))?;

        Ok(Self {
            language,
            source,
            tree,
        })
    }

    pub fn root_node(&self) -> Node<'_> {
        self.tree.root_node()
    }

    pub fn node_text<'a>(&'a self, node: &Node) -> &'a str {
        node.utf8_text(self.source.as_bytes()).unwrap_or("")
    }

    /// Convert tree-sitter 0-indexed point to 1-indexed (line, col)
    pub fn point_to_1indexed(point: Point) -> (usize, usize) {
        (point.row + 1, point.column + 1)
    }

    /// 1-indexed (start_line, end_line)
    pub fn node_line_range(node: &Node) -> (usize, usize) {
        let start = node.start_position().row + 1;
        let end = node.end_position().row + 1;
        (start, end)
    }

    /// 1-indexed (start_col, end_col)
    pub fn node_col_range(node: &Node) -> (usize, usize) {
        let start = node.start_position().column + 1;
        let end = node.end_position().column + 1;
        (start, end)
    }

    /// Checks whether node's line range overlaps with any changed range in the diff.
    /// Range overlap condition: max(start_a, start_b) <= min(end_a, end_b)
    pub fn node_overlaps_ranges(node: &Node, ranges: &[(usize, usize)]) -> bool {
        let (n_start, n_end) = Self::node_line_range(node);
        for &(r_start, r_end) in ranges {
            let overlap_start = n_start.max(r_start);
            let overlap_end = n_end.min(r_end);
            if overlap_start <= overlap_end {
                return true;
            }
        }
        false
    }

    /// Traverses all descendants of a root node recursively
    pub fn find_all_descendants<'a, F>(&'a self, root: Node<'a>, predicate: &F) -> Vec<Node<'a>>
    where
        F: Fn(&Node<'a>) -> bool,
    {
        let mut results = Vec::new();
        let mut cursor = root.walk();

        Self::visit_node(&mut cursor, &mut results, predicate);
        results
    }

    fn visit_node<'a, F>(
        cursor: &mut tree_sitter::TreeCursor<'a>,
        results: &mut Vec<Node<'a>>,
        predicate: &F,
    ) where
        F: Fn(&Node<'a>) -> bool,
    {
        let node = cursor.node();
        if predicate(&node) {
            results.push(node);
        }

        if cursor.goto_first_child() {
            loop {
                Self::visit_node(cursor, results, predicate);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_typescript() {
        let code = r#"
            function add(a: number, b: number): number {
                return a + b;
            }
        "#
        .to_string();

        let parsed = ParsedSource::parse(Path::new("test.ts"), code).expect("must parse");
        let (start, end) = ParsedSource::node_line_range(&parsed.root_node());
        assert!(start >= 1);
        assert!(end >= 1);

        let calls = parsed
            .find_all_descendants(parsed.root_node(), &|n| n.kind() == "function_declaration");
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn test_range_overlap() {
        let code = "const x = 10;\nconst y = 20;\nconst z = 30;\n".to_string();
        let parsed = ParsedSource::parse(Path::new("test.js"), code).expect("must parse");

        let vars =
            parsed.find_all_descendants(parsed.root_node(), &|n| n.kind() == "lexical_declaration");
        assert_eq!(vars.len(), 3);

        // Line 2 was changed
        let changed = vec![(2, 2)];
        assert!(!ParsedSource::node_overlaps_ranges(&vars[0], &changed));
        assert!(ParsedSource::node_overlaps_ranges(&vars[1], &changed));
        assert!(!ParsedSource::node_overlaps_ranges(&vars[2], &changed));
    }

    #[test]
    fn test_parse_python() {
        let code = "@pytest.mark.skip\ndef hello():\n    eval('1+1')\n    assert True\n    try:\n        pass\n    except Exception:\n        pass\n".to_string();
        let parsed = ParsedSource::parse(Path::new("test.py"), code).expect("must parse python");
        let decs = parsed.find_all_descendants(parsed.root_node(), &|n| n.kind() == "decorator");
        let calls = parsed.find_all_descendants(parsed.root_node(), &|n| n.kind() == "call");
        let asserts =
            parsed.find_all_descendants(parsed.root_node(), &|n| n.kind() == "assert_statement");
        let excepts =
            parsed.find_all_descendants(parsed.root_node(), &|n| n.kind() == "except_clause");
        assert_eq!(decs.len(), 1);
        assert_eq!(calls.len(), 1);
        assert_eq!(asserts.len(), 1);
        assert_eq!(excepts.len(), 1);
    }

    #[test]
    fn test_parse_go() {
        let code = "package main\n\nfunc TestFoo(t *testing.T) {\n\tt.Skip(\"reason\")\n\tif err != nil {\n\t}\n}\n".to_string();
        let parsed = ParsedSource::parse(Path::new("main_test.go"), code).expect("must parse go");
        let calls =
            parsed.find_all_descendants(parsed.root_node(), &|n| n.kind() == "call_expression");
        let ifs = parsed.find_all_descendants(parsed.root_node(), &|n| n.kind() == "if_statement");
        assert!(!calls.is_empty());
        assert!(!ifs.is_empty());
    }

    #[test]
    fn test_tree_sitter_query_execution() {
        use tree_sitter::{Query, QueryCursor, StreamingIterator};
        let code = "const secret = '12345';\nconst normal = 42;\n".to_string();
        let parsed = ParsedSource::parse(Path::new("test.js"), code).unwrap();
        let ts_lang = parsed.language.tree_sitter_language();
        let query_str = "(variable_declarator name: (identifier) @name value: (string) @val)";
        let query = Query::new(&ts_lang, query_str).expect("query compile");
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, parsed.root_node(), parsed.source.as_bytes());
        let mut count = 0;
        while let Some(m) = matches.next() {
            count += 1;
            assert!(!m.captures().is_empty());
        }
        assert_eq!(count, 1);
    }
}
