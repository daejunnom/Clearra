#[derive(Clone, Debug, PartialEq)]
pub struct RenderSetup {
    family_id: u32,
    probability: f64,
}

impl RenderSetup {
    pub fn new(family_id: u32, probability: f64) -> Self {
        Self {
            family_id,
            probability,
        }
    }
}
impl RenderSetup {
    pub fn family_id(&self) -> u32 {
        self.family_id
    }
}
impl RenderSetup {
    pub fn probability(&self) -> f64 {
        self.probability
    }
}
