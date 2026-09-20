use std::collections::HashMap;
use std::time::Instant;
use tb_diff::DiffParser;
use tb_rules::{RuleEngine, Severity};

struct TestCase {
    name: &'static str,
    diff: &'static str,
    expected_rule: Option<&'static str>,
}

#[test]
fn precision_benchmark() {
    let cases = vec![
        // 1. Positive: TB002 test-disabled
        TestCase {
            name: "TB002 describe.skip",
            diff: r#"diff --git a/auth.test.ts b/auth.test.ts
--- a/auth.test.ts
+++ b/auth.test.ts
@@ -1,3 +1,3 @@
-describe("Login", () => {
+describe.skip("Login", () => {
   it("works", () => {});
 });
"#,
            expected_rule: Some("TB002"),
        },
        // 2. Negative: Normal describe call (must NOT trigger TB002 or TB003)
        TestCase {
            name: "Clean test addition",
            diff: r#"diff --git a/login.test.ts b/login.test.ts
--- a/login.test.ts
+++ b/login.test.ts
@@ -1,1 +1,3 @@
+describe("Login", () => {
+  it("works", () => { expect(loginUser()).toBe(true); });
+});
"#,
            expected_rule: None,
        },
        // 3. Positive: TB003 tautological assertion
        TestCase {
            name: "TB003 expect(true).toBe(true)",
            diff: r#"diff --git a/calc.test.ts b/calc.test.ts
--- a/calc.test.ts
+++ b/calc.test.ts
@@ -5,1 +5,1 @@
-  expect(calc(2, 2)).toBe(4);
+  expect(true).toBe(true);
"#,
            expected_rule: Some("TB003"),
        },
        // 4. Positive: TB004 continue-on-error in CI
        TestCase {
            name: "TB004 continue-on-error",
            diff: r#"diff --git a/.github/workflows/ci.yml b/.github/workflows/ci.yml
--- a/.github/workflows/ci.yml
+++ b/.github/workflows/ci.yml
@@ -10,1 +10,2 @@
       - run: npm test
+        continue-on-error: true
"#,
            expected_rule: Some("TB004"),
        },
        // 5. Positive: TB005 early exit injected
        TestCase {
            name: "TB005 unreachable return",
            diff: r#"diff --git a/worker.ts b/worker.ts
--- a/worker.ts
+++ b/worker.ts
@@ -2,2 +2,3 @@
 function work() {
+  return;
   processNext();
 }
"#,
            expected_rule: Some("TB005"),
        },
        // 6. Positive: TB009 fixture-snooping in production
        TestCase {
            name: "TB009 NODE_ENV test check in source",
            diff: r#"diff --git a/src/service.ts b/src/service.ts
--- a/src/service.ts
+++ b/src/service.ts
@@ -3,1 +3,3 @@
+if (process.env.NODE_ENV === 'test') {
+  return mockService;
+}
"#,
            expected_rule: Some("TB009"),
        },
        // 7. Positive: TB101 silent-catch
        TestCase {
            name: "TB101 empty catch block",
            diff: r#"diff --git a/src/fetcher.ts b/src/fetcher.ts
--- a/src/fetcher.ts
+++ b/src/fetcher.ts
@@ -5,3 +5,5 @@
   try {
     fetch();
+  } catch (e) {
+  }
"#,
            expected_rule: Some("TB101"),
        },
        // 8. Negative: catch with logger (must NOT trigger TB101)
        TestCase {
            name: "Catch with logger",
            diff: r#"diff --git a/src/fetcher.ts b/src/fetcher.ts
--- a/src/fetcher.ts
+++ b/src/fetcher.ts
@@ -5,3 +5,5 @@
   try {
     fetch();
+  } catch (e) {
+    console.error("fetch failed", e);
+  }
"#,
            expected_rule: None,
        },
        // 9. Positive: TB104 async-foreach
        TestCase {
            name: "TB104 arr.forEach(async ...)",
            diff: r#"diff --git a/src/batch.ts b/src/batch.ts
--- a/src/batch.ts
+++ b/src/batch.ts
@@ -4,1 +4,3 @@
+items.forEach(async (item) => {
+  await process(item);
+});
"#,
            expected_rule: Some("TB104"),
        },
        // 10. Positive: TB201 dynamic-eval
        TestCase {
            name: "TB201 eval call",
            diff: r#"diff --git a/src/calc.ts b/src/calc.ts
--- a/src/calc.ts
+++ b/src/calc.ts
@@ -2,1 +2,1 @@
-return compute(expr);
+return eval(expr);
"#,
            expected_rule: Some("TB201"),
        },
        // 11. Positive: TB202 shell-injection
        TestCase {
            name: "TB202 exec with interpolation",
            diff: r#"diff --git a/src/exec.ts b/src/exec.ts
--- a/src/exec.ts
+++ b/src/exec.ts
@@ -3,1 +3,1 @@
-execSync("git status");
+execSync(`git checkout ${branch}`);
"#,
            expected_rule: Some("TB202"),
        },
        // 12. Positive: TB203 raw-sql-interpolation
        TestCase {
            name: "TB203 raw query interpolation",
            diff: r#"diff --git a/src/db.ts b/src/db.ts
--- a/src/db.ts
+++ b/src/db.ts
@@ -3,1 +3,1 @@
-await prisma.$queryRaw`SELECT * FROM users WHERE id = ${id}`;
+await prisma.$queryRawUnsafe(`SELECT * FROM users WHERE id = '${id}'`);
"#,
            expected_rule: Some("TB203"),
        },
    ];

    let engine = RuleEngine::new();
    let start_time = Instant::now();

    let mut true_positives = 0;
    let mut true_negatives = 0;
    let mut false_positives = 0;
    let mut false_negatives = 0;

    for case in &cases {
        let diff = DiffParser::parse(case.diff);
        let mut old_map = HashMap::new();
        let mut new_map = HashMap::new();

        for f in diff.scannable_files() {
            if let Some(syn_new) = f.reconstruct_synthetic_new_source() {
                new_map.insert(f.path.clone(), syn_new);
            }
            if let Some(syn_old) = f.reconstruct_synthetic_old_source() {
                old_map.insert(f.path.clone(), syn_old);
            }
        }

        let findings = engine.run(&diff, &old_map, &new_map);

        if let Some(expected) = case.expected_rule {
            let matched = findings.iter().any(|f| f.rule_id == expected);
            if matched {
                true_positives += 1;
            } else {
                false_negatives += 1;
                eprintln!(
                    "[FAIL] Expected rule {} for case '{}', but not found",
                    expected, case.name
                );
            }
        } else {
            // Negative case: should have 0 error/warn findings
            let unwanted: Vec<_> = findings
                .iter()
                .filter(|f| f.severity != Severity::Info)
                .collect();
            if unwanted.is_empty() {
                true_negatives += 1;
            } else {
                false_positives += 1;
                eprintln!("[FAIL] False positive on '{}': {:?}", case.name, unwanted);
            }
        }
    }

    let elapsed = start_time.elapsed();
    let total_cases = cases.len();
    let precision = (true_positives as f64) / ((true_positives + false_positives) as f64) * 100.0;

    println!("\n=== Precision Benchmark Results ===");
    println!("Total Test Cases:   {}", total_cases);
    println!("True Positives:     {}", true_positives);
    println!("True Negatives:     {}", true_negatives);
    println!("False Positives:    {}", false_positives);
    println!("False Negatives:    {}", false_negatives);
    println!("Precision Gate:     {:.2}% (Target >= 95.0%)", precision);
    println!(
        "Total Duration:     {:?} (~{:.2} ms/case)",
        elapsed,
        elapsed.as_secs_f64() * 1000.0 / total_cases as f64
    );
    println!("====================================\n");

    assert_eq!(
        false_positives, 0,
        "Zero false positives required by Precision Gate!"
    );
    assert_eq!(
        false_negatives, 0,
        "All target rules must correctly trigger!"
    );
    assert!(precision >= 95.0, "Precision Gate requires >= 95%");
}
