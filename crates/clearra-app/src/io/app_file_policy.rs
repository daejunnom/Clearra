#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppFilePolicy {
    verbose_paths: bool,
}

impl AppFilePolicy {
    pub fn new(verbose_paths: bool) -> Self {
        Self { verbose_paths }
    }
}
impl AppFilePolicy {
    pub fn verbose_paths(&self) -> bool {
        self.verbose_paths
    }
}

impl Default for AppFilePolicy {
    fn default() -> Self {
        Self::new(false)
    }
}
