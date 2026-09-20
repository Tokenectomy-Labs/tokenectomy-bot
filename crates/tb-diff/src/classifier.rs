use crate::types::FileKind;
use std::path::Path;

pub fn classify_path(path: &Path) -> FileKind {
    let path_str = path.to_string_lossy();
    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");

    // 1. Ignored files & directories
    if is_ignored_path(&path_str, file_name) {
        return FileKind::Ignored;
    }

    // 2. Generated files
    if is_generated_path(&path_str, file_name) {
        return FileKind::Generated;
    }

    // 3. CI files
    if is_ci_path(&path_str, file_name) {
        return FileKind::Ci;
    }

    // 4. Test files
    if is_test_path(&path_str, file_name) {
        return FileKind::Test;
    }

    // 5. Config files
    if is_config_path(&path_str, file_name) {
        return FileKind::Config;
    }

    // Default to Source for recognizable code files
    FileKind::Source
}

fn is_ignored_path(path_str: &str, file_name: &str) -> bool {
    let p = path_str.replace('\\', "/");

    // Directories
    if p.contains("/node_modules/")
        || p.starts_with("node_modules/")
        || p.contains("/dist/")
        || p.starts_with("dist/")
        || p.contains("/build/")
        || p.starts_with("build/")
        || p.contains("/vendor/")
        || p.starts_with("vendor/")
        || p.contains("/target/")
        || p.starts_with("target/")
        || p.contains("/.git/")
        || p.starts_with(".git/")
    {
        return true;
    }

    // Lockfiles & minified assets
    match file_name {
        "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml" | "Cargo.lock" | "composer.lock"
        | "Gemfile.lock" | "poetry.lock" | "flake.lock" => return true,
        _ => {}
    }

    if file_name.ends_with(".min.js")
        || file_name.ends_with(".min.css")
        || file_name.ends_with(".map")
        || file_name.ends_with(".bundle.js")
    {
        return true;
    }

    false
}

fn is_generated_path(path_str: &str, file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    if lower.contains(".generated.")
        || lower.starts_with("generated.")
        || lower.contains(".g.dart")
        || lower.contains(".pb.go")
        || lower.contains("_pb2.py")
    {
        return true;
    }

    let p = path_str.replace('\\', "/");
    if p.contains("/generated/") || p.contains("/__generated__/") {
        return true;
    }

    false
}

fn is_ci_path(path_str: &str, file_name: &str) -> bool {
    let p = path_str.replace('\\', "/");
    if p.starts_with(".github/workflows/") || p.contains("/.github/workflows/") {
        return true;
    }

    match file_name {
        ".gitlab-ci.yml" | "azure-pipelines.yml" | "Jenkinsfile" | ".travis.yml" => true,
        _ => p.starts_with(".circleci/") || p.contains("/.circleci/"),
    }
}

fn is_test_path(path_str: &str, file_name: &str) -> bool {
    let p = path_str.replace('\\', "/");

    // Directory hints
    if p.contains("/__tests__/")
        || p.starts_with("__tests__/")
        || p.contains("/tests/")
        || p.starts_with("tests/")
        || p.contains("/test/")
        || p.starts_with("test/")
        || p.contains("/spec/")
        || p.starts_with("spec/")
        || p.contains("/fixtures/")
        || p.starts_with("fixtures/")
    {
        return true;
    }

    // Filename suffixes
    let lower = file_name.to_lowercase();
    lower.ends_with(".test.ts")
        || lower.ends_with(".test.tsx")
        || lower.ends_with(".test.js")
        || lower.ends_with(".test.jsx")
        || lower.ends_with(".spec.ts")
        || lower.ends_with(".spec.tsx")
        || lower.ends_with(".spec.js")
        || lower.ends_with(".spec.jsx")
        || lower.ends_with("_test.go")
        || lower.ends_with("_test.rs")
        || lower.ends_with("_test.py")
        || lower.ends_with("test.py")
        || (lower.starts_with("test_") && lower.ends_with(".py"))
}

fn is_config_path(_path_str: &str, file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    if lower.starts_with("tsconfig") && lower.ends_with(".json") {
        return true;
    }
    if lower.contains("jest.config")
        || lower.contains("vitest.config")
        || lower.contains("webpack.config")
        || lower.contains("vite.config")
        || lower.contains("babel.config")
        || lower.contains("rollup.config")
        || lower.contains(".eslintrc")
        || lower.contains("eslint.config")
    {
        return true;
    }

    matches!(
        file_name,
        "Cargo.toml" | "package.json" | "pyproject.toml" | "go.mod" | "deno.json"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classifications() {
        assert_eq!(classify_path(Path::new("src/auth.ts")), FileKind::Source);
        assert_eq!(
            classify_path(Path::new("src/__tests__/auth.test.ts")),
            FileKind::Test
        );
        assert_eq!(
            classify_path(Path::new("tests/integration_test.rs")),
            FileKind::Test
        );
        assert_eq!(
            classify_path(Path::new(".github/workflows/ci.yml")),
            FileKind::Ci
        );
        assert_eq!(classify_path(Path::new("tsconfig.json")), FileKind::Config);
        assert_eq!(
            classify_path(Path::new("package-lock.json")),
            FileKind::Ignored
        );
        assert_eq!(
            classify_path(Path::new("node_modules/lodash/index.js")),
            FileKind::Ignored
        );
        assert_eq!(
            classify_path(Path::new("src/generated/types.ts")),
            FileKind::Generated
        );
    }
}
