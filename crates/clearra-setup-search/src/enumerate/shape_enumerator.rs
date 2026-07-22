use clearra_core_domain::ids::setup_id::SetupFamilyId;

use crate::identity::shape_family::ShapeFamily;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ShapeEnumerator;

impl ShapeEnumerator {
    pub fn from_masks(masks: impl IntoIterator<Item = u64>) -> Vec<ShapeFamily> {
        masks
            .into_iter()
            .enumerate()
            .map(|(index, mask)| ShapeFamily::new(SetupFamilyId::new(index as u32), mask))
            .collect()
    }
}
