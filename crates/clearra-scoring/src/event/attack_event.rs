#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AttackEvent {
    amount: u32,
}

impl AttackEvent {
    pub fn new(amount: u32) -> Self {
        Self { amount }
    }
}
impl AttackEvent {
    pub fn amount(self) -> u32 {
        self.amount
    }
}
