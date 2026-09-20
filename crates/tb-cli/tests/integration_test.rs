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

#[test]
fn test_cli_review_with_custom_config() {
    let dir = tempdir().unwrap();
    let diff_file = dir.path().join("test.diff");
    let diff_content = r#"diff --git a/auth.test.ts b/auth.test.ts
--- a/auth.test.ts
+++ b/auth.test.ts
@@ -1,3 +1,3 @@
-describe("Authentication", () => {
+describe.skip("Authentication", () => {
   it("works", () => {});
 });
"#;
    fs::write(&diff_file, diff_content).unwrap();

    let config_file = dir.path().join("tokenectomy.json");
    let config_content = r#"{
      "rules": {
        "TB002": "off"
      }
    }"#;
    fs::write(&config_file, config_content).unwrap();

    let bin = env!("CARGO_BIN_EXE_tokenectomy-bot");
    let output = Command::new(bin)
        .current_dir(dir.path())
        .args([
            "review",
            "--diff-file",
            diff_file.to_str().unwrap(),
            "--config",
            config_file.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .expect("execute");

    // Because TB002 is off and no other findings exist, exit code is 0!
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"findings\": []"));
    assert!(stdout.contains("\"total\": 0"));
}

#[test]
fn test_cli_review_with_ignore_glob() {
    let dir = tempdir().unwrap();
    let diff_file = dir.path().join("test.diff");
    let diff_content = r#"diff --git a/legacy/auth.test.ts b/legacy/auth.test.ts
--- a/legacy/auth.test.ts
+++ b/legacy/auth.test.ts
@@ -1,3 +1,3 @@
-describe("Auth", () => {
+describe.skip("Auth", () => {
   it("works", () => {});
 });
"#;
    fs::write(&diff_file, diff_content).unwrap();

    let config_file = dir.path().join("tokenectomy.json");
    let config_content = r#"{
      "ignore": ["**/legacy/**"]
    }"#;
    fs::write(&config_file, config_content).unwrap();

    let bin = env!("CARGO_BIN_EXE_tokenectomy-bot");
    let output = Command::new(bin)
        .current_dir(dir.path())
        .args([
            "review",
            "--diff-file",
            diff_file.to_str().unwrap(),
            "--config",
            config_file.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .expect("execute");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"findings\": []"));
    assert!(stdout.contains("\"total\": 0"));
}

#[test]
fn test_cli_mcp_stdio_roundtrip() {
    use std::io::Write;

    let bin = env!("CARGO_BIN_EXE_tokenectomy-bot");
    let mut child = Command::new(bin)
        .arg("mcp")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn mcp process");

    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 100,
        "method": "tools/list"
    });

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(stdin, "{}", serde_json::to_string(&req).unwrap()).unwrap();
    }

    let output = child.wait_with_output().expect("wait");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"name\":\"pre_pr_review\""));
    assert!(stdout.contains("\"name\":\"audit_diff\""));
}

#[test]
fn test_cli_custom_rule_validation_and_fixture() {
    let dir = tempdir().unwrap();
    let rule_file = dir.path().join("no_eval.scm");
    let rule_scm = r#"
;; @id: CORP001
;; @name: no-dynamic-eval
;; @severity: error
;; @message: Dynamic eval violates organization security baseline
;; @languages: typescript, javascript

(call_expression
  function: (identifier) @fn (#eq? @fn "eval")) @match
"#;
    fs::write(&rule_file, rule_scm).unwrap();

    let fixture_bad = dir.path().join("bad.ts");
    fs::write(&fixture_bad, "eval('2 + 2');\n").unwrap();

    let bin = env!("CARGO_BIN_EXE_tokenectomy-bot");

    // 1. Validation with matching fixture should succeed
    let output = Command::new(bin)
        .args([
            "rules",
            "validate",
            rule_file.to_str().unwrap(),
            "--fixture",
            fixture_bad.to_str().unwrap(),
        ])
        .output()
        .expect("execute");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Custom rule is valid"));
    assert!(stdout.contains("CORP001"));
    assert!(stdout.contains("PASSED"));

    // 2. Validation with clean fixture should fail with error
    let fixture_clean = dir.path().join("clean.ts");
    fs::write(&fixture_clean, "const x = 2 + 2;\n").unwrap();

    let output_clean = Command::new(bin)
        .args([
            "rules",
            "validate",
            rule_file.to_str().unwrap(),
            "--fixture",
            fixture_clean.to_str().unwrap(),
        ])
        .output()
        .expect("execute");

    assert_eq!(output_clean.status.code(), Some(1));
}

#[test]
fn test_cli_ledger_record_verify_and_metrics() {
    let dir = tempdir().unwrap();
    let ledger_path = dir.path().join("sealed-audit.jsonl");

    let diff_file = dir.path().join("test.diff");
    let diff_content = r#"diff --git a/calc.ts b/calc.ts
--- a/calc.ts
+++ b/calc.ts
@@ -1,1 +1,1 @@
-export const add = (a, b) => a + b;
+export const add = (a, b) => a + b; // clean
"#;
    fs::write(&diff_file, diff_content).unwrap();

    let bin = env!("CARGO_BIN_EXE_tokenectomy-bot");

    // 1. Run review and write to ledger
    let output_review = Command::new(bin)
        .current_dir(dir.path())
        .args([
            "review",
            "--diff-file",
            diff_file.to_str().unwrap(),
            "--ledger",
            ledger_path.to_str().unwrap(),
            "--repo-name",
            "Tokenectomy/TestRepo",
        ])
        .output()
        .expect("execute");

    assert_eq!(output_review.status.code(), Some(0));
    assert!(ledger_path.exists());

    // 2. Verify ledger integrity
    let output_verify = Command::new(bin)
        .args(["ledger", "verify", ledger_path.to_str().unwrap()])
        .output()
        .expect("execute");

    assert_eq!(output_verify.status.code(), Some(0));
    let stdout_verify = String::from_utf8_lossy(&output_verify.stdout);
    assert!(stdout_verify.contains("Integrity Verified"));

    // 3. Inspect metrics dashboard
    let output_metrics = Command::new(bin)
        .args(["ledger", "metrics", ledger_path.to_str().unwrap()])
        .output()
        .expect("execute");

    assert_eq!(output_metrics.status.code(), Some(0));
    let stdout_metrics = String::from_utf8_lossy(&output_metrics.stdout);
    assert!(stdout_metrics.contains("Total PRs Reviewed:       1"));
    assert!(stdout_metrics.contains("Pass Rate:                100.0%"));
    assert!(stdout_metrics.contains("Chain Status:      VERIFIED ✔"));

    // 4. Tamper with ledger and verify detection
    let content = fs::read_to_string(&ledger_path).unwrap();
    let tampered = content.replace("Tokenectomy/TestRepo", "Attacker/CompromisedRepo");
    fs::write(&ledger_path, tampered).unwrap();

    let output_tampered = Command::new(bin)
        .args(["ledger", "verify", ledger_path.to_str().unwrap()])
        .output()
        .expect("execute");

    assert_eq!(output_tampered.status.code(), Some(1));
    let stdout_tampered = String::from_utf8_lossy(&output_tampered.stdout);
    assert!(stdout_tampered.contains("TAMPERED / INVALID"));
}

#[test]
fn test_cli_org_policy_enforcement() {
    let dir = tempdir().unwrap();
    let diff_file = dir.path().join("test.diff");
    let diff_content = r#"diff --git a/test.ts b/test.ts
--- a/test.ts
+++ b/test.ts
@@ -1,1 +1,1 @@
+describe.skip("Auth", () => {});
"#;
    fs::write(&diff_file, diff_content).unwrap();

    let org_policy_file = dir.path().join("org-policy.json");
    let org_policy_content = r#"{
      "extends": "strict",
      "rules": {
        "TB002": "error"
      }
    }"#;
    fs::write(&org_policy_file, org_policy_content).unwrap();

    // Local repo attempts to disable TB002
    let repo_config_file = dir.path().join("tokenectomy.json");
    let repo_config_content = r#"{
      "rules": {
        "TB002": "off"
      }
    }"#;
    fs::write(&repo_config_file, repo_config_content).unwrap();

    let bin = env!("CARGO_BIN_EXE_tokenectomy-bot");

    // 1. Review without --allow-policy-downgrade must fail configuration policy
    let output = Command::new(bin)
        .current_dir(dir.path())
        .args([
            "review",
            "--diff-file",
            diff_file.to_str().unwrap(),
            "--config",
            repo_config_file.to_str().unwrap(),
            "--org-policy",
            org_policy_file.to_str().unwrap(),
            "--fail-open=false",
        ])
        .output()
        .expect("execute");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Organization policy forbids disabling rule 'TB002'"));

    // 2. Review with --allow-policy-downgrade succeeds
    let output_allowed = Command::new(bin)
        .current_dir(dir.path())
        .args([
            "review",
            "--diff-file",
            diff_file.to_str().unwrap(),
            "--config",
            repo_config_file.to_str().unwrap(),
            "--org-policy",
            org_policy_file.to_str().unwrap(),
            "--allow-policy-downgrade",
            "--format",
            "json",
        ])
        .output()
        .expect("execute");

    assert_eq!(output_allowed.status.code(), Some(0));
}
