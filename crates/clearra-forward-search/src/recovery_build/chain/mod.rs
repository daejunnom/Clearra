//! Multi-boundary Build composition in one persistent logical frame. Stages do
//! not reset the real board, hold slot, source cursor or B2B state. Independent
//! input languages are concatenated symbolically, never expanded as tuples.
mod geometry;
mod parallel;
mod plan;
mod solver;
mod source;
#[cfg(test)]
mod tests;
mod wire;
use super::staged::source::cancelled;
use super::{RecoveryBuildError as Error, RecoveryBuildQuery};
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask as Mask, execution_cancellation::ExecutionControl,
    piece::piece_kind::PieceKind,
};
pub(super) use parallel::{Coordinator, Worker};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryBuildStage {
    /// Cells added at this stage, in the common logical frame of the editor.
    pub target: Mask,
    pub supply: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryChainWitness {
    pub targets: Vec<[u64; 4]>,
    pub queues: Vec<Vec<PieceKind>>,
    pub pattern_indices: Vec<usize>,
    pub placement_stages: Vec<u8>,
    /// One independently enforced quota per nonfinal boundary.
    pub early_by_boundary: Vec<u8>,
}

pub(super) fn validate(q: &RecoveryBuildQuery) -> Result<(), Error> {
    use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;
    let n = q.stages.len();
    if !(2..=60).contains(&n) || !(1..=24).contains(&q.fields.height) {
        return Err(Error::InvalidHeight);
    }
    if q.fields.middle != q.stages[0].target
        || q.first_supply != q.stages[0].supply
        || q.fields.result != q.stages[n - 1].target
        || q.second_supply != q.stages[n - 1].supply
    {
        return Err(Error::InvalidSupplyPattern);
    }
    let cells = u16::from(q.fields.height) * 10;
    let mut occupied = q.fields.initial;
    if occupied.fits_cell_count(cells) != Ok(true) {
        return Err(Error::BoardOutsideField);
    }
    for stage in &q.stages {
        if stage.target.fits_cell_count(cells) != Ok(true) {
            return Err(Error::BoardOutsideField);
        }
        if stage.target.intersects(occupied) {
            return Err(Error::MiddleOverlapsStart);
        }
        let area = stage.target.count_ones();
        if area == 0 || area % 4 != 0 {
            return Err(Error::TargetAreaNotTetrominoes);
        }
        occupied = occupied.union(stage.target);
        let expression = QueuePatternExpression::parse(&stage.supply, 0)
            .map_err(|_| Error::InvalidSupplyPattern)?;
        if expression.sequence_len() == 0 {
            return Err(Error::EmptySupply);
        }
    }
    Ok(())
}

pub(super) fn reflected(mask: Mask, height: u8) -> Result<Mask, Error> {
    mask.mirrored_horizontally(10, u16::from(height))
        .map_err(|_| Error::BoardOutsideField)
}
pub(super) fn project(mask: Mask, occupied: Mask, height: u8) -> Mask {
    let mut out = Mask::EMPTY;
    let mut physical = 0_u16;
    for y in 0..u16::from(height) {
        let full = (0..10).all(|x| occupied.contains_index(y * 10 + x));
        if !full {
            for x in 0..10 {
                if mask.contains_index(y * 10 + x) {
                    out = out.union(Mask::singleton(physical * 10 + x).expect("bounded cell"));
                }
            }
            physical += 1;
        }
    }
    out
}
/// Uses the existing Build mirror applicability rule on the nominal boundary.
/// This admits a target orientation, not a claim of mirrored physical motion.
fn can_reflect(base: Mask, target: Mask, height: u8) -> Result<bool, Error> {
    Ok(
        clearra_problem::BuildProbabilityField::from_words_preserving_height(
            height,
            base.words(),
            target.words(),
        )
        .map_err(|_| Error::BoardOutsideField)?
        .with_horizontal_mirror_included(true)
        .includes_applicable_horizontal_mirror(),
    )
}
