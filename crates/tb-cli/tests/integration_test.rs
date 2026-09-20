use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_cli_init_command() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("tokenectomy.json");

    let bin = env!("CARGO_BIN_EXE_tokenectomy-bot");
    let status = Command::new(bin)
        .arg("init")
        .arg("--output")
        .arg(&config_path)
        .status()
        .expect("failed to execute binary");

    assert!(status.success());
    assert!(config_path.exists());
    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("\"extends\": \"recommended\""));
    assert!(content.contains("\"TB001\""));
    assert!(content.contains("\"TB002\""));
}

#[test]
fn test_cli_review_diff_file_with_findings() {
    let dir = tempdir().unwrap();
    let test_file = dir.path().join("auth.test.ts");
    let test_content = r#"
describe.skip("Authentication Service", () => {
  it("verifies credentials correctly", () => {
    expect(true).toBe(true);
  });
});
"#;
    fs::write(&test_file, test_content).unwrap();

    let diff_file = dir.path().join("test.diff");
    let diff_content = r#"diff --git a/auth.test.ts b/auth.test.ts
index 1111111..2222222 100644
--- a/auth.test.ts
+++ b/auth.test.ts
@@ -1,5 +1,5 @@
-describe("Authentication Service", () => {
+describe.skip("Authentication Service", () => {
   it("verifies credentials correctly", () => {
-    expect(verify("admin", "secret")).toBe(true);
+    expect(true).toBe(true);
   });
 });
"#;
    fs::write(&diff_file, diff_content).unwrap();

    let bin = env!("CARGO_BIN_EXE_tokenectomy-bot");

    // 1. Text format
    let output_text = Command::new(bin)
        .current_dir(dir.path())
        .args([
            "review",
            "--diff-file",
            diff_file.to_str().unwrap(),
            "--format",
            "text",
            "--fail-on",
            "error",
        ])
        .output()
        .expect("execute");

    assert_eq!(output_text.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output_text.stdout);
    assert!(stdout.contains("TB002"));
    assert!(stdout.contains("TB003"));

    // 2. JSON format
    let output_json = Command::new(bin)
        .current_dir(dir.path())
        .args([
            "review",
            "--diff-file",
            diff_file.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .expect("execute");

    let json_str = String::from_utf8_lossy(&output_json.stdout);
    assert!(json_str.contains("\"rule_id\": \"TB002\""));
    assert!(json_str.contains("\"rule_id\": \"TB003\""));

    // 3. GitHub Annotations format
    let output_gh = Command::new(bin)
        .current_dir(dir.path())
        .args([
            "review",
            "--diff-file",
            diff_file.to_str().unwrap(),
            "--format",
            "github",
        ])
        .output()
        .expect("execute");

    let gh_str = String::from_utf8_lossy(&output_gh.stdout);
    assert!(gh_str.contains("::error file=auth.test.ts,line="));

    // 4. SARIF format
    let output_sarif = Command::new(bin)
        .current_dir(dir.path())
        .args([
            "review",
            "--diff-file",
            diff_file.to_str().unwrap(),
            "--format",
            "sarif",
        ])
        .output()
        .expect("execute");

    let sarif_str = String::from_utf8_lossy(&output_sarif.stdout);
    assert!(sarif_str.contains("\"version\": \"2.1.0\""));
    assert!(sarif_str.contains("oasis-tcs"));
}
