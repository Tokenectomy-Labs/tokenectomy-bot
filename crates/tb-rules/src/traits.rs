use crate::model::{Finding, RuleContext, Severity};

pub trait Rule: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn default_severity(&self) -> Severity;
    fn needs_old_side(&self) -> bool {
        false
    }
    fn check(&self, ctx: &RuleContext) -> Vec<Finding>;
}
