#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HoldPolicy {
    Forbidden,
    #[default]
    Allowed,
    Required,
}

impl HoldPolicy {
    pub fn allows_hold(self) -> bool {
        !matches!(self, Self::Forbidden)
    }
}
