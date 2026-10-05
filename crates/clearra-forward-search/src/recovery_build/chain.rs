//! Stage identity and shared-frame boundaries. This module never multiplies
//! stage probabilities or resets a held token at a field boundary.
use super::{RecoveryBuildError as Error, RecoveryBuildFields, RecoveryBuildQuery};
use crate::board::{place_and_clear, ForwardBoard};
use clearra_core_domain::board::standard_pc_board::Board256Mask as Mask;
use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryChainStage {
    /// Disjoint target cells in the original editor frame, including final.
    pub target: Mask,
    pub supply: String,
}
pub(super) fn mirror(mask: Mask, height: u8) -> Mask {
    let mut out = ForwardBoard::EMPTY;
    for y in 0..height {
        for x in 0..10u16 {
            if mask.contains_index(u16::from(y) * 10 + x) {
                out.insert(u16::from(y) * 10 + 9 - x);
            }
        }
    }
    Mask::from_words(out.words())
}
pub(super) fn symmetric(mask: Mask, height: u8) -> bool {
    let (board, _, _) = place_and_clear(10, height, ForwardBoard::from_mask(mask));
    let mask = Mask::from_words(board.words());
    mirror(mask, height) == mask
}
pub(super) fn fields(
    height: u8,
    initial: Mask,
    targets: &[Mask],
) -> Result<RecoveryBuildFields, Error> {
    if targets.len() < 2 {
        return Err(Error::InvalidSupplyPattern);
    }
    let middle = targets[..targets.len() - 1]
        .iter()
        .fold(Mask::EMPTY, |m, t| m.union(*t));
    let before = ForwardBoard::from_mask(initial.union(middle));
    let final_target = ForwardBoard::from_mask(*targets.last().unwrap());
    let mut result = ForwardBoard::EMPTY;
    let mut output = 0u8;
    for y in 0..height {
        if before.row_bits(10, y) == 1023 {
            continue;
        }
        let row = final_target.row_bits(10, y);
        for x in 0..10u16 {
            if row & (1 << x) != 0 {
                result.insert(u16::from(output) * 10 + x);
            }
        }
        output += 1;
    }
    Ok(RecoveryBuildFields {
        height,
        initial,
        middle,
        result: Mask::from_words(result.words()),
    })
}
pub(super) fn validate(q: &RecoveryBuildQuery) -> Result<(), Error> {
    if q.chain_stages.len() < 3 || q.chain_stages.len() > 60 || !q.all_solutions {
        return Err(Error::InvalidSupplyPattern);
    }
    if q.first_supply != q.chain_stages[0].supply
        || q.second_supply != q.chain_stages.last().unwrap().supply
    {
        return Err(Error::InvalidSupplyPattern);
    }
    let mut used = q.fields.initial;
    let mut length = 0u16;
    for s in &q.chain_stages {
        if s.target.is_empty() || s.target.count_ones() % 4 != 0 {
            return Err(Error::TargetAreaNotTetrominoes);
        }
        if s.target.fits_cell_count(u16::from(q.fields.height) * 10) != Ok(true) {
            return Err(Error::BoardOutsideField);
        }
        if s.target.intersects(used) {
            return Err(Error::MiddleOverlapsStart);
        }
        used = used.union(s.target);
        let p =
            QueuePatternExpression::parse(&s.supply, 0).map_err(|_| Error::InvalidSupplyPattern)?;
        if p.sequence_len() == 0 {
            return Err(Error::EmptySupply);
        }
        length = length
            .checked_add(u16::try_from(p.sequence_len()).map_err(|_| Error::CounterOverflow)?)
            .ok_or(Error::CounterOverflow)?;
    }
    let expected = fields(
        q.fields.height,
        q.fields.initial,
        &q.chain_stages.iter().map(|s| s.target).collect::<Vec<_>>(),
    )?;
    if expected != q.fields {
        return Err(Error::InvalidSupplyPattern);
    }
    Ok(())
}
pub(super) fn lengths(q: &RecoveryBuildQuery) -> Result<Vec<u16>, Error> {
    q.chain_stages
        .iter()
        .map(|s| {
            let p = QueuePatternExpression::parse(&s.supply, 0)
                .map_err(|_| Error::InvalidSupplyPattern)?;
            u16::try_from(p.sequence_len()).map_err(|_| Error::CounterOverflow)
        })
        .collect()
}

/// Full target suffixes, not mirrored output counts. Actual physical locks and
/// original queue tokens are verified separately for EVERY retained orientation.
pub(super) fn orientations(
    q: &RecoveryBuildQuery,
    control: &clearra_core_domain::execution_cancellation::ExecutionControl,
) -> Result<Vec<(RecoveryBuildFields, Vec<Mask>)>, Error> {
    if q.chain_stages.is_empty() {
        let mut roots = vec![q.fields.clone()];
        if symmetric(q.fields.initial, q.fields.height) {
            let mut reflected = q.fields.clone();
            reflected.middle = mirror(reflected.middle, reflected.height);
            reflected.result = mirror(reflected.result, reflected.height);
            if !roots.contains(&reflected) {
                roots.push(reflected);
            }
        }
        let mut out = Vec::new();
        for root in roots {
            let mut variants = vec![root.clone()];
            if symmetric(root.initial.union(root.middle), root.height) {
                let mut reflected = root;
                reflected.result = mirror(reflected.result, reflected.height);
                if !variants.contains(&reflected) {
                    variants.push(reflected);
                }
            }
            for variant in variants {
                if !out.iter().any(|(f, _)| *f == variant) {
                    out.push((variant, Vec::new()));
                }
            }
        }
        return Ok(out);
    }
    let original = q.chain_stages.iter().map(|s| s.target).collect::<Vec<_>>();
    let mut variants = vec![original];
    for boundary in 0..q.chain_stages.len() {
        let existing = variants.len();
        for i in 0..existing {
            if control.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let prefix = variants[i][..boundary]
                .iter()
                .fold(q.fields.initial, |m, t| m.union(*t));
            if !symmetric(prefix, q.fields.height) {
                continue;
            }
            let mut next = variants[i].clone();
            for target in &mut next[boundary..] {
                *target = mirror(*target, q.fields.height);
            }
            if !variants.contains(&next) {
                variants
                    .try_reserve(1)
                    .map_err(|_| Error::MemoryUnavailable)?;
                variants.push(next);
            }
        }
    }
    variants
        .into_iter()
        .map(|targets| {
            Ok((
                fields(q.fields.height, q.fields.initial, &targets)?,
                targets,
            ))
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(super) struct Progress {
    pub used: Vec<u8>,
    pub balance: Vec<[i16; 7]>,
    pub early: Vec<u8>,
}
impl Progress {
    pub fn new(n: usize) -> Self {
        Self {
            used: vec![0; n],
            balance: vec![[0; 7]; n],
            early: vec![0; n.saturating_sub(1)],
        }
    }
    pub fn has_early(&self) -> bool {
        self.early.iter().any(|&n| n != 0)
    }
}

impl RecoveryBuildFields {
    pub fn from_chain(
        height: u8,
        initial: Mask,
        stages: &[RecoveryChainStage],
    ) -> Result<Self, Error> {
        if !(1..=24).contains(&height) {
            return Err(Error::InvalidHeight);
        }
        fields(
            height,
            initial,
            &stages.iter().map(|s| s.target).collect::<Vec<_>>(),
        )
    }
}
