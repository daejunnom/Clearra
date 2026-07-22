use super::AppFilePolicy;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AppFileResolver {
    policy: AppFilePolicy,
}

impl AppFileResolver {
    pub fn new(policy: AppFilePolicy) -> Self {
        Self { policy }
    }
}
impl AppFileResolver {
    pub fn with_policy(&self, policy: AppFilePolicy) -> Self {
        Self { policy }
    }
}
impl AppFileResolver {
    pub fn policy(&self) -> &AppFilePolicy {
        &self.policy
    }
}
impl AppFileResolver {
    pub fn service_name(&self) -> &'static str {
        "app-file-resolver"
    }
}
