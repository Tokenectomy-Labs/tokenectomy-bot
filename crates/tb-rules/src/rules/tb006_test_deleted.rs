use crate::model::{Confidence, Finding, RuleContext, Severity};
use crate::traits::Rule;
use tb_diff::{DiffStatus, FileKind};

pub struct Tb006TestDeleted;

impl Rule for Tb006TestDeleted {
    fn id(&self) -> &'static str {
        "TB006"
    }

    fn name(&self) -> &'static str {
        "test-deleted-with-source-change"
    }

    fn default_severity(&self) -> Severity {
        Severity::Info
    }

    fn check(&self, ctx: &RuleContext) -> Vec<Finding> {
        let mut findings = Vec::new();

        if ctx.file_diff.kind != FileKind::Test {
            return findings;
        }

        if matches!(ctx.file_diff.status, DiffStatus::Deleted) {
            let path_str = ctx.file_diff.path.to_string_lossy();
            findings.push(Finding::new(
                self.id(),
                self.name(),
                self.default_severity(),
                Confidence::High,
                &ctx.file_diff.path,
                1,
                1,
                1,
                1,
                format!("Test file was completely deleted: `{}`", path_str),
                Some("Verify if deleting this test suite was intentional and that regressions are prevented.".to_string()),
                "file_deletion",
                &path_str,
            ));
        }

        findings
    }
}
