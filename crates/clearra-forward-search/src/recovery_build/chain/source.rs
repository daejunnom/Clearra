use super::super::staged::{
    diagram::{Diagram, Id, ALL, NONE},
    source::compile_stage,
};
use super::*;
use clearra_supply::{
    pattern_universe::{
        pattern_universe_materializer::PatternUniverseMaterializer, MaterializedPatternUniverse,
    },
    queue::queue_pattern_expression::QueuePatternExpression,
};
use std::collections::{BTreeMap, HashMap};

pub(super) struct Source {
    pub inputs: Vec<MaterializedPatternUniverse>,
    pub offsets: Vec<u16>,
    pub counts: Vec<Option<[u8; 7]>>,
    pub universe: Id,
    pub possible: u128,
}
impl Source {
    pub fn new(
        q: &RecoveryBuildQuery,
        d: &mut Diagram,
        control: &ExecutionControl,
    ) -> Result<Self, Error> {
        validate(q)?;
        let mut inputs = Vec::new();
        let mut offsets = vec![0_u16];
        let mut counts = Vec::new();
        let mut universe = ALL;
        let mut possible = 1_u128;
        for s in &q.stages {
            cancelled(control)?;
            let expression = QueuePatternExpression::parse(&s.supply, 0)
                .map_err(|_| Error::InvalidSupplyPattern)?;
            let input = PatternUniverseMaterializer::queue_pattern_expression(&expression, 0)
                .map_err(|_| Error::PatternDomainUnavailable)?;
            let offset = *offsets.last().ok_or(Error::PatternDomainUnavailable)?;
            let (root, len, inventory) = compile_stage(d, &input, offset, control)?;
            offsets.push(offset.checked_add(len).ok_or(Error::CounterOverflow)?);
            possible = possible
                .checked_mul(input.pattern_count() as u128)
                .ok_or(Error::CounterOverflow)?;
            universe = d.intersect(universe, root)?;
            counts.push(inventory);
            inputs.push(input);
        }
        if d.count(
            universe,
            0,
            *offsets.last().ok_or(Error::PatternDomainUnavailable)?,
        )? != possible
        {
            return Err(Error::PatternDomainUnavailable);
        }
        Ok(Self {
            inputs,
            offsets,
            counts,
            universe,
            possible,
        })
    }
    pub fn end(&self) -> u16 {
        *self.offsets.last().expect("nonempty stage offsets")
    }
    pub fn origin(&self, index: u16) -> usize {
        self.offsets.partition_point(|&offset| offset <= index) - 1
    }
    pub fn whole_inventory(&self, pieces: usize) -> Option<[u8; 7]> {
        if usize::from(self.end()) != pieces {
            return None;
        }
        let mut total = [0_u8; 7];
        for counts in &self.counts {
            for (out, n) in total.iter_mut().zip((*counts)?) {
                *out = out.checked_add(n)?;
            }
        }
        Some(total)
    }
    pub fn queues(&self, d: &Diagram, mut language: Id) -> Result<Vec<Vec<PieceKind>>, Error> {
        use super::super::staged::source::PIECES;
        let mut word = Vec::new();
        for level in 0..self.end() {
            let (p, next) = (0..7)
                .find_map(|p| {
                    let next = d.follow(language, level, p);
                    (next != NONE).then_some((p, next))
                })
                .ok_or(Error::PatternDomainUnavailable)?;
            word.push(PIECES[p]);
            language = next;
        }
        if language != ALL {
            return Err(Error::PatternDomainUnavailable);
        }
        Ok(self
            .offsets
            .windows(2)
            .map(|w| word[usize::from(w[0])..usize::from(w[1])].to_vec())
            .collect())
    }
    pub fn indices(
        &self,
        queues: &[Vec<PieceKind>],
        control: &ExecutionControl,
    ) -> Result<Vec<usize>, Error> {
        if queues.len() != self.inputs.len() {
            return Err(Error::PatternDomainUnavailable);
        }
        self.inputs
            .iter()
            .zip(queues)
            .map(|(input, queue)| {
                for i in 0..input.pattern_count() {
                    cancelled(control)?;
                    if input.sequence_at(i).as_ref() == queue.as_slice() {
                        return Ok(i);
                    }
                }
                Err(Error::PatternDomainUnavailable)
            })
            .collect()
    }
    pub fn accepts(&self, d: &Diagram, mut language: Id, queues: &[Vec<PieceKind>]) -> bool {
        if queues.len() != self.inputs.len() {
            return false;
        }
        for (s, queue) in queues.iter().enumerate() {
            if queue.len() != usize::from(self.offsets[s + 1] - self.offsets[s]) {
                return false;
            }
            for (i, &p) in queue.iter().enumerate() {
                language = d.follow(
                    language,
                    self.offsets[s] + i as u16,
                    super::super::search::piece_index(p),
                );
            }
        }
        language == ALL
    }
    /// Exact weighted measure over independent stages. Identical residual
    /// languages are memoized between stages, so no Cartesian tuple loop exists.
    /// The compact uniform final stage can be counted directly in the MDD.
    pub fn measure(
        &self,
        d: &mut Diagram,
        root: Id,
        control: &ExecutionControl,
    ) -> Result<(u128, f64), Error> {
        fn visit(
            s: &Source,
            d: &mut Diagram,
            stage: usize,
            id: Id,
            memo: &mut HashMap<(usize, Id), (u128, f64)>,
            control: &ExecutionControl,
        ) -> Result<(u128, f64), Error> {
            cancelled(control)?;
            if id == NONE {
                return Ok((0, 0.0));
            }
            if stage == s.inputs.len() {
                return if id == ALL {
                    Ok((1, 1.0))
                } else {
                    Err(Error::PatternDomainUnavailable)
                };
            }
            if let Some(&v) = memo.get(&(stage, id)) {
                return Ok(v);
            }
            if id == ALL {
                let n = s.inputs[stage..]
                    .iter()
                    .try_fold(1_u128, |n, u| n.checked_mul(u.pattern_count() as u128))
                    .ok_or(Error::CounterOverflow)?;
                return Ok((n, 1.0));
            }
            let input = &s.inputs[stage];
            if stage + 1 == s.inputs.len() && input.uniform_compact_source().is_some() {
                let count = d.count(id, s.offsets[stage], s.end())?;
                let value = (count, count as f64 / input.pattern_count() as f64);
                memo.try_reserve(1).map_err(|_| Error::MemoryUnavailable)?;
                memo.insert((stage, id), value);
                return Ok(value);
            }
            // Group equal residuals before entering the next stage. These are
            // single-source rows, never tuples of all previous source rows.
            let mut tails = BTreeMap::<Id, (u128, f64)>::new();
            for i in 0..input.pattern_count() {
                cancelled(control)?;
                let mut tail = id;
                for (j, p) in input.sequence_at(i).iter().enumerate() {
                    tail = d.follow(
                        tail,
                        s.offsets[stage] + j as u16,
                        super::super::search::piece_index(*p),
                    );
                }
                if tail != NONE {
                    let v = tails.entry(tail).or_default();
                    v.0 = v.0.checked_add(1).ok_or(Error::CounterOverflow)?;
                    v.1 += input.weight_at(i).get();
                }
            }
            let mut count = 0_u128;
            let mut sum = 0_f64;
            let mut correction = 0_f64;
            for (tail, (prefixes, mass)) in tails {
                let (n, p) = visit(s, d, stage + 1, tail, memo, control)?;
                count = count
                    .checked_add(prefixes.checked_mul(n).ok_or(Error::CounterOverflow)?)
                    .ok_or(Error::CounterOverflow)?;
                let y = mass * p - correction;
                let t = sum + y;
                correction = (t - sum) - y;
                sum = t;
            }
            let value = (count, sum.clamp(0.0, 1.0));
            memo.try_reserve(1).map_err(|_| Error::MemoryUnavailable)?;
            memo.insert((stage, id), value);
            Ok(value)
        }
        visit(self, d, 0, root, &mut HashMap::new(), control)
    }
}
