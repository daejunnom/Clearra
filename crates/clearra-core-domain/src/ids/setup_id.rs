#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SetupFamilyId(u32);

impl SetupFamilyId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}
impl SetupFamilyId {
    pub fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TilingVariantId(u32);

impl TilingVariantId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}
impl TilingVariantId {
    pub fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BuildVariantId(u32);

impl BuildVariantId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}
impl BuildVariantId {
    pub fn get(self) -> u32 {
        self.0
    }
}
