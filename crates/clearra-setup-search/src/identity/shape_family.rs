use clearra_core_domain::{
    ids::setup_id::SetupFamilyId,
    solution::{ShapeKey, VisualGroupKey},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShapeFamily {
    id: SetupFamilyId,
    occupied_shape_key: ShapeKey,
    visual_group_key: VisualGroupKey,
}

impl ShapeFamily {
    pub fn new(id: SetupFamilyId, occupied_shape: u64) -> Self {
        Self {
            id,
            occupied_shape_key: ShapeKey(occupied_shape),
            visual_group_key: VisualGroupKey(occupied_shape),
        }
    }
}
impl ShapeFamily {
    pub fn with_visual_group_key(mut self, visual_group_key: VisualGroupKey) -> Self {
        self.visual_group_key = visual_group_key;
        self
    }
}
impl ShapeFamily {
    pub fn id(self) -> SetupFamilyId {
        self.id
    }
}
impl ShapeFamily {
    pub fn occupied_shape(self) -> u64 {
        self.occupied_shape_key.0
    }
}
impl ShapeFamily {
    pub fn occupied_shape_key(self) -> ShapeKey {
        self.occupied_shape_key
    }
}
impl ShapeFamily {
    pub fn visual_group_key(self) -> VisualGroupKey {
        self.visual_group_key
    }
}
impl ShapeFamily {
    pub fn can_source_probability(self) -> bool {
        false
    }
}
