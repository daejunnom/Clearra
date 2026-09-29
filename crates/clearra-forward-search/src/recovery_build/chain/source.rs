//! One reduced language per source at disjoint levels. Probability is measured
//! after execution-language union, never by multiplying stage success rates.
use super::{RecoveryChainError as Error, RecoveryChainQuery};
use crate::recovery_build::{
    search::piece_index,
    staged::{
        diagram::{Diagram, Id, ALL, NONE},
        source::{cancelled, compile_language, PIECES},
    },
    RecoveryBuildError as Core,
};
use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_supply::{
    pattern_universe::{
        pattern_universe_materializer::PatternUniverseMaterializer, MaterializedPatternUniverse,
    },
    queue::queue_pattern_expression::QueuePatternExpression,
};
use std::collections::HashMap;

// Bound recursive diagram depth before construction. Overflow and unsupported
// input are errors, not zero coverage or an implicitly truncated supply.
pub(super) const MAX_SOURCE_WINDOW: usize = 256;
pub(super) fn validate(q: &RecoveryChainQuery) -> Result<(), Error> {
    let mut total = 0usize;
    for input in &q.supplies {
        let expression =
            QueuePatternExpression::parse(input, 0).map_err(|_| Core::InvalidSupplyPattern)?;
        if expression.sequence_len() == 0 {
            return Err(Core::EmptySupply.into());
        }
        total = total
            .checked_add(expression.sequence_len())
            .ok_or(Core::CounterOverflow)?;
    }
    if total > MAX_SOURCE_WINDOW {
        return Err(Error::SupplyWindowTooLong {
            maximum: MAX_SOURCE_WINDOW,
        });
    }
    Ok(())
}
#[derive(Clone)]
pub(super) struct Sources {
    pub universes: Vec<MaterializedPatternUniverse>,
    pub boundaries: Vec<u16>,
    pub fixed_counts: Vec<Option<[u8; 7]>>,
    pub universe: Id,
    pub possible: u128,
    uniform: bool,
}
impl Sources {
    pub fn compile(
        q: &RecoveryChainQuery,
        diagram: &mut Diagram,
        control: &ExecutionControl,
    ) -> Result<Self, Error> {
        validate(q)?;
        let mut universes = Vec::new();
        let mut boundaries = vec![0_u16];
        let mut fixed_counts = Vec::new();
        let mut universe = ALL;
        let mut possible = 1_u128;
        let mut uniform = true;
        for input in &q.supplies {
            cancelled(control)?;
            let expression =
                QueuePatternExpression::parse(input, 0).map_err(|_| Core::InvalidSupplyPattern)?;
            let u = PatternUniverseMaterializer::queue_pattern_expression(&expression, 0)
                .map_err(|_| Core::PatternDomainUnavailable)?;
            let offset = *boundaries.last().ok_or(Core::PatternDomainUnavailable)?;
            let (root, counts, compact) = compile_language(diagram, &u, offset, control)?;
            let len = u16::try_from(u.sequence_len_at(0)).map_err(|_| Core::CounterOverflow)?;
            boundaries.push(offset.checked_add(len).ok_or(Core::CounterOverflow)?);
            possible = possible
                .checked_mul(u.pattern_count() as u128)
                .ok_or(Core::CounterOverflow)?;
            universe = diagram.intersect(universe, root)?;
            uniform &= compact;
            fixed_counts.push(counts);
            universes.push(u);
        }
        let out = Self {
            universes,
            boundaries,
            fixed_counts,
            universe,
            possible,
            uniform,
        };
        if diagram.count(universe, 0, out.end())? != possible {
            return Err(Core::PatternDomainUnavailable.into());
        }
        Ok(out)
    }
    pub fn end(&self) -> u16 {
        *self.boundaries.last().expect("validated sources")
    }
    pub fn stage_of(&self, index: u16) -> Option<usize> {
        self.boundaries
            .windows(2)
            .position(|w| w[0] <= index && index < w[1])
    }
    pub fn len(&self, stage: usize) -> usize {
        usize::from(self.boundaries[stage + 1] - self.boundaries[stage])
    }
    pub fn measure(
        &self,
        diagram: &mut Diagram,
        language: Id,
        control: &ExecutionControl,
    ) -> Result<(u128, f64), Error> {
        cancelled(control)?;
        if diagram.difference(language, self.universe)? != NONE {
            return Err(Core::PatternDomainUnavailable.into());
        }
        let count = diagram.count(language, 0, self.end())?;
        if self.uniform {
            return Ok((count, count as f64 / self.possible as f64));
        }
        let weight = self.weight(diagram, language, 0, &mut HashMap::new(), control)?;
        if !weight.is_finite() || !(-1e-12..=1.0 + 1e-12).contains(&weight) {
            return Err(Core::PatternDomainUnavailable.into());
        }
        Ok((count, weight.clamp(0.0, 1.0)))
    }
    fn weight(
        &self,
        diagram: &Diagram,
        language: Id,
        stage: usize,
        memo: &mut HashMap<(usize, Id), f64>,
        control: &ExecutionControl,
    ) -> Result<f64, Error> {
        cancelled(control)?;
        if language == NONE {
            return Ok(0.0);
        }
        if stage == self.universes.len() {
            return if language == ALL {
                Ok(1.0)
            } else {
                Err(Core::PatternDomainUnavailable.into())
            };
        }
        if let Some(&v) = memo.get(&(stage, language)) {
            return Ok(v);
        }
        let mut sum = 0.0;
        let mut correction = 0.0;
        let u = &self.universes[stage];
        for i in 0..u.pattern_count() {
            cancelled(control)?;
            let queue = u.sequence_at(i);
            let mut child = language;
            for (offset, &p) in queue.iter().enumerate() {
                child = diagram.follow(
                    child,
                    self.boundaries[stage] + offset as u16,
                    piece_index(p),
                );
            }
            let value =
                u.weight_at(i).get() * self.weight(diagram, child, stage + 1, memo, control)?;
            let adjusted = value - correction;
            let next = sum + adjusted;
            correction = (next - sum) - adjusted;
            sum = next;
        }
        memo.try_reserve(1).map_err(|_| Core::MemoryUnavailable)?;
        memo.insert((stage, language), sum);
        Ok(sum)
    }
    pub fn first(&self, diagram: &Diagram, mut language: Id) -> Result<Vec<Vec<PieceKind>>, Error> {
        if language == NONE {
            return Err(Core::PatternDomainUnavailable.into());
        }
        let mut queues = vec![Vec::new(); self.universes.len()];
        for level in 0..self.end() {
            let mut chosen = None;
            for (piece, &kind) in PIECES.iter().enumerate() {
                let child = diagram.follow(language, level, piece);
                if child != NONE {
                    chosen = Some((kind, child));
                    break;
                }
            }
            let (piece, child) = chosen.ok_or(Core::PatternDomainUnavailable)?;
            let stage = self.stage_of(level).ok_or(Core::PatternDomainUnavailable)?;
            queues[stage].push(piece);
            language = child;
        }
        if language != ALL {
            return Err(Core::PatternDomainUnavailable.into());
        }
        Ok(queues)
    }
}
