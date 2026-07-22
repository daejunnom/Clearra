use crate::profile::AttackModelId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttackModelDescriptor {
    id: AttackModelId,
    display_name: &'static str,
    exact_attack_table_pinned: bool,
}

impl AttackModelDescriptor {
    pub const fn new(
        id: AttackModelId,
        display_name: &'static str,
        exact_attack_table_pinned: bool,
    ) -> Self {
        Self {
            id,
            display_name,
            exact_attack_table_pinned,
        }
    }
}
impl AttackModelDescriptor {
    pub const fn id(self) -> AttackModelId {
        self.id
    }
}
impl AttackModelDescriptor {
    pub const fn display_name(self) -> &'static str {
        self.display_name
    }
}
impl AttackModelDescriptor {
    pub const fn exact_attack_table_pinned(self) -> bool {
        self.exact_attack_table_pinned
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AttackModelRegistry;

impl AttackModelRegistry {
    pub fn builtins() -> Vec<AttackModelDescriptor> {
        vec![
            AttackModelDescriptor::new(AttackModelId::Disabled, "Disabled", true),
            AttackModelDescriptor::new(AttackModelId::Guideline, "Guideline", false),
            AttackModelDescriptor::new(AttackModelId::Ppt, "Puyo Puyo Tetris", false),
            AttackModelDescriptor::new(AttackModelId::Tetrio, "TETR.IO", false),
        ]
    }
}
impl AttackModelRegistry {
    pub fn get(id: AttackModelId) -> Option<AttackModelDescriptor> {
        Self::builtins()
            .into_iter()
            .find(|descriptor| descriptor.id() == id)
    }
}
impl AttackModelRegistry {
    pub fn parse(value: &str) -> Option<AttackModelDescriptor> {
        AttackModelId::parse(value).and_then(Self::get)
    }
}
