use tb_rules::{Finding, Severity};

pub struct GitHubAnnotationReporter;

impl GitHubAnnotationReporter {
    pub fn format(findings: &[Finding]) -> String {
        let mut out = String::new();
        for f in findings {
            let cmd = match f.severity {
                Severity::Error => "error",
                Severity::Warn => "warning",
                Severity::Info => "notice",
            };

            let file_str = f.file.to_string_lossy().replace('\\', "/");
            let title = format!("{} ({})", f.rule_id, f.rule_name);
            let msg = if let Some(ref hint) = f.fix_hint {
                format!("{}. Fix: {}", f.message, hint)
            } else {
                f.message.clone()
            };

            // Escaping for GitHub workflow command
            let escaped_msg = msg
                .replace('%', "%25")
                .replace('\r', "%0D")
                .replace('\n', "%0A");

            out.push_str(&format!(
                "::{} file={},line={},col={},title={}::{}\n",
                cmd, file_str, f.start_line, f.start_col, title, escaped_msg
            ));
        }
        out
    }
}
