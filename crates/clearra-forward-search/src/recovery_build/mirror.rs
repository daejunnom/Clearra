//! Automatic target orientations at symmetric nominal boundaries.
//! A reflection at the initial boundary transforms the entire remaining suffix;
//! a reflection after the middle transforms only the final target. Every branch
//! is still verified against the original supply, hold and rotation rules.
use super::{RecoveryBuildError as Error, RecoveryBuildFields};
use crate::board::{place_and_clear, ForwardBoard};
use clearra_core_domain::board::standard_pc_board::Board256Mask as Mask;
use clearra_problem::BuildProbabilityField;

fn reflected(mask: Mask, height: u8) -> Result<Mask, Error> {
    mask.mirrored_horizontally(10, u16::from(height))
        .map_err(|_| Error::BoardOutsideField)
}
fn applicable(height: u8, base: Mask, target: Mask) -> Result<bool, Error> {
    Ok(
        BuildProbabilityField::from_words_preserving_height(height, base.words(), target.words())
            .map_err(|_| Error::BoardOutsideField)?
            .with_horizontal_mirror_included(true)
            .includes_applicable_horizontal_mirror(),
    )
}
fn append_final_orientations(
    output: &mut Vec<RecoveryBuildFields>,
    fields: RecoveryBuildFields,
) -> Result<(), Error> {
    if !output.contains(&fields) {
        output.push(fields.clone());
    }
    let (base, _, _) = place_and_clear(
        10,
        fields.height,
        ForwardBoard::from_mask(fields.initial.union(fields.middle)),
    );
    if applicable(fields.height, Mask::from_words(base.words()), fields.result)? {
        let mut other = fields;
        other.result = reflected(other.result, other.height)?;
        if !output.contains(&other) {
            output.push(other);
        }
    }
    Ok(())
}

pub(super) fn orientations(
    fields: &RecoveryBuildFields,
) -> Result<Vec<RecoveryBuildFields>, Error> {
    fields.prepare()?;
    let mut result = Vec::with_capacity(4);
    // Keep pre-existing orientation IDs in their original order.
    append_final_orientations(&mut result, fields.clone())?;
    if applicable(fields.height, fields.initial, fields.middle)? {
        let mut other = fields.clone();
        other.middle = reflected(fields.middle, fields.height)?;
        other.result = reflected(fields.result, fields.height)?;
        append_final_orientations(&mut result, other)?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mask(v: u64) -> Mask {
        Mask::from_words([v, 0, 0, 0])
    }
    #[test]
    fn initial_symmetry_reflects_both_targets_not_the_queue_or_board() {
        let fields = RecoveryBuildFields {
            height: 8,
            initial: mask(0),
            middle: mask(0x1007),
            result: mask(0x300c00),
        };
        let variants = orientations(&fields).unwrap();
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0], fields);
        assert_eq!(variants[1].initial, fields.initial);
        assert_eq!(variants[1].middle, reflected(fields.middle, 8).unwrap());
        assert_eq!(variants[1].result, reflected(fields.result, 8).unwrap());
    }
    #[test]
    fn asymmetric_start_does_not_enable_initial_reflection() {
        let fields = RecoveryBuildFields {
            height: 8,
            initial: mask(512),
            middle: mask(0x1007),
            result: mask(0x300c00),
        };
        assert_eq!(orientations(&fields).unwrap(), vec![fields]);
    }
    #[test]
    fn existing_middle_boundary_mirror_keeps_its_index_and_is_not_duplicated() {
        let fields = RecoveryBuildFields {
            height: 8,
            initial: mask(0x3f0),
            middle: mask(0xf),
            result: mask(0xc03),
        };
        let variants = orientations(&fields).unwrap();
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0], fields);
        assert_eq!(variants[1].middle, fields.middle);
        assert_eq!(variants[1].result, reflected(fields.result, 8).unwrap());
    }
}
