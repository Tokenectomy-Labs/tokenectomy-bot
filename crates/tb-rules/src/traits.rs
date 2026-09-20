use crate::model::{Finding, RuleContext, Severity};

pub trait Rule: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn default_severity(&self) -> Severity;
    fn needs_old_side(&self) -> bool {
        false
    }
    fn check(&self, ctx: &RuleContext) -> Vec<Finding>;
}
