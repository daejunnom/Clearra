#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupArgs {
    remaining: String,
    allow_post_cycle_borrow: bool,
}

impl SetupArgs {
    pub fn new(remaining: impl Into<String>, allow_post_cycle_borrow: bool) -> Self {
        Self {
            remaining: remaining.into(),
            allow_post_cycle_borrow,
        }
    }
}
impl SetupArgs {
    pub fn remaining(&self) -> &str {
        &self.remaining
    }
}
impl SetupArgs {
    pub fn allow_post_cycle_borrow(&self) -> bool {
        self.allow_post_cycle_borrow
    }
}

impl Default for SetupArgs {
    fn default() -> Self {
        Self::new("IOTSZJL", false)
    }
}
