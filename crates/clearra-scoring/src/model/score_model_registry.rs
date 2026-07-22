use crate::profile::ScoreModelId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoreModelDescriptor {
    id: ScoreModelId,
    display_name: &'static str,
    exact_score_table_pinned: bool,
}

impl ScoreModelDescriptor {
    pub const fn new(
        id: ScoreModelId,
        display_name: &'static str,
        exact_score_table_pinned: bool,
    ) -> Self {
        Self {
            id,
            display_name,
            exact_score_table_pinned,
        }
    }
}
impl ScoreModelDescriptor {
    pub const fn id(self) -> ScoreModelId {
        self.id
    }
}
impl ScoreModelDescriptor {
    pub const fn display_name(self) -> &'static str {
        self.display_name
    }
}
impl ScoreModelDescriptor {
    pub const fn exact_score_table_pinned(self) -> bool {
        self.exact_score_table_pinned
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScoreModelRegistry;

impl ScoreModelRegistry {
    pub fn builtins() -> Vec<ScoreModelDescriptor> {
        vec![
            ScoreModelDescriptor::new(ScoreModelId::Disabled, "Disabled", true),
            ScoreModelDescriptor::new(ScoreModelId::Guideline, "Guideline", false),
            ScoreModelDescriptor::new(ScoreModelId::JstrisUltra, "Jstris Ultra", false),
            ScoreModelDescriptor::new(ScoreModelId::Tetrio, "TETR.IO", false),
        ]
    }
}
impl ScoreModelRegistry {
    pub fn get(id: ScoreModelId) -> Option<ScoreModelDescriptor> {
        Self::builtins()
            .into_iter()
            .find(|descriptor| descriptor.id() == id)
    }
}
impl ScoreModelRegistry {
    pub fn parse(value: &str) -> Option<ScoreModelDescriptor> {
        ScoreModelId::parse(value).and_then(Self::get)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_model_registry_lists_known_score_models() {
        assert!(ScoreModelRegistry::parse("tetrio").is_some());
        assert!(ScoreModelRegistry::parse("unknown").is_none());
        assert!(!ScoreModelRegistry::get(ScoreModelId::Tetrio)
            .expect("tetrio")
            .exact_score_table_pinned());
    }
}
