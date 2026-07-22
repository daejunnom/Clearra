#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ShapeFamilyId(u32);

impl ShapeFamilyId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}
impl ShapeFamilyId {
    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ShapeKey(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VisualGroupKey(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShapeFamily {
    pub shape_family_id: ShapeFamilyId,
    pub occupied_shape_key: ShapeKey,
    pub visual_group_key: VisualGroupKey,
}

impl ShapeFamily {
    pub const fn new(
        shape_family_id: ShapeFamilyId,
        occupied_shape_key: ShapeKey,
        visual_group_key: VisualGroupKey,
    ) -> Self {
        Self {
            shape_family_id,
            occupied_shape_key,
            visual_group_key,
        }
    }
}
impl ShapeFamily {
    pub const fn groups_visual_shape_only(self) -> bool {
        true
    }
}
