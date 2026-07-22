use crate::layout::board64_layout::Board64Layout;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MirrorTransform;

impl MirrorTransform {
    pub fn mirror_mask(mask: u64, layout: Board64Layout) -> u64 {
        let width = layout.width();
        let height = layout.height();
        let mut mirrored = 0_u64;

        for y in 0..height {
            for x in 0..width {
                let source_index = u32::from(y) * u32::from(width) + u32::from(x);
                if (mask & (1_u64 << source_index)) == 0 {
                    continue;
                }
                let mirrored_x = width - 1 - x;
                let target_index = u32::from(y) * u32::from(width) + u32::from(mirrored_x);
                mirrored |= 1_u64 << target_index;
            }
        }

        mirrored
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_mask_horizontally() {
        let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
        let mask = 1_u64 << 0;

        assert_eq!(MirrorTransform::mirror_mask(mask, layout), 1_u64 << 9);
    }
}
