use super::two_line_supply_transition::TwoLineSupplyTransition;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TwoLineSupplyDp {
    transitions: Vec<TwoLineSupplyTransition>,
}

impl TwoLineSupplyDp {
    pub fn new(transitions: Vec<TwoLineSupplyTransition>) -> Self {
        Self { transitions }
    }
}
impl TwoLineSupplyDp {
    pub fn transitions(&self) -> &[TwoLineSupplyTransition] {
        &self.transitions
    }
}
impl TwoLineSupplyDp {
    pub fn is_empty(&self) -> bool {
        self.transitions.is_empty()
    }
}
