#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupArgs {
    queue: String,
    fixed_queue: bool,
}

impl SetupArgs {
    pub fn new(queue: impl Into<String>, fixed_queue: bool) -> Self {
        Self {
            queue: queue.into(),
            fixed_queue,
        }
    }
}
impl SetupArgs {
    pub fn queue(&self) -> &str {
        &self.queue
    }
}
impl SetupArgs {
    pub fn fixed_queue(&self) -> bool {
        self.fixed_queue
    }
}

impl Default for SetupArgs {
    fn default() -> Self {
        Self::new("", false)
    }
}
