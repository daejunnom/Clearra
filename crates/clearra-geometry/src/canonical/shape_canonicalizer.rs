use crate::{canonical::mirror_transform::MirrorTransform, layout::board64_layout::Board64Layout};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CanonicalShape {
    mask: u64,
    mirrored: bool,
}

impl CanonicalShape {
    pub fn mask(self) -> u64 {
        self.mask
    }
}
impl CanonicalShape {
    pub fn used_mirror(self) -> bool {
        self.mirrored
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ShapeCanonicalizer;

impl ShapeCanonicalizer {
    pub fn canonicalize(mask: u64, layout: Board64Layout) -> CanonicalShape {
        let mirrored = MirrorTransform::mirror_mask(mask, layout);
        if mirrored < mask {
            CanonicalShape {
                mask: mirrored,
                mirrored: true,
            }
        } else {
            CanonicalShape {
                mask,
                mirrored: false,
            }
        }
    }
}
