//! Reflect the complete remaining target suffix at a symmetric boundary.
//! Queues, hold tokens and kick rules remain unchanged and are verified again.
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
fn append_final(
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
    // Retain the two pre-existing direction IDs before adding initial mirrors.
    append_final(&mut result, fields.clone())?;
    if applicable(fields.height, fields.initial, fields.middle)? {
        let mut other = fields.clone();
        other.middle = reflected(fields.middle, fields.height)?;
        other.result = reflected(fields.result, fields.height)?;
        append_final(&mut result, other)?;
    }
    Ok(result)
}
