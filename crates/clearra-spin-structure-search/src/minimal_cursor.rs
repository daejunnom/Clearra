use super::*;
use crate::cooperative_sort::CooperativeSort;

struct OrderedOutcome {
    ordinal: usize,
    keyed: KeyedOutcome,
    keep: bool,
}

enum Phase {
    Load,
    Sort,
    Deduplicate,
    Minimum,
    Partition,
    SortRegular,
    SortMini,
    Publish,
    Complete,
}

pub(crate) struct MinimumCursor {
    report: SpinStructureReport,
    input: std::iter::Chain<
        std::vec::IntoIter<SpinStructureOutcome>,
        std::vec::IntoIter<SpinStructureOutcome>,
    >,
    ordered: Vec<OrderedOutcome>,
    sorted: std::vec::IntoIter<OrderedOutcome>,
    regular: Vec<KeyedOutcome>,
    mini: Vec<KeyedOutcome>,
    publish: std::vec::IntoIter<KeyedOutcome>,
    phase: Phase,
    sort: CooperativeSort,
    ordinal: usize,
    index: usize,
    group: usize,
    known: usize,
    minimum: Option<usize>,
    policy: MinimalityPolicy,
}

impl MinimumCursor {
    pub(crate) fn new(mut report: SpinStructureReport) -> Self {
        report.minimum_placements = None;
        for layer in &mut report.layers {
            layer.accepted_regular = 0;
            layer.accepted_mini = 0;
        }
        let input = std::mem::take(&mut report.regular)
            .into_iter()
            .chain(std::mem::take(&mut report.mini).into_iter());
        let policy = report
            .query
            .as_ref()
            .map_or(MinimalityPolicy::SubsetMinimal, |query| query.minimality);
        Self {
            report,
            input,
            ordered: Vec::new(),
            sorted: Vec::new().into_iter(),
            regular: Vec::new(),
            mini: Vec::new(),
            publish: Vec::new().into_iter(),
            phase: Phase::Load,
            sort: CooperativeSort::new(0),
            ordinal: 0,
            index: 0,
            group: 0,
            known: 0,
            minimum: None,
            policy,
        }
    }

    pub(crate) fn step(&mut self) -> bool {
        match self.phase {
            Phase::Load => {
                if let Some(outcome) = self.input.next() {
                    self.ordered.push(OrderedOutcome {
                        ordinal: self.ordinal,
                        keyed: KeyedOutcome::new(outcome),
                        keep: true,
                    });
                    self.ordinal += 1;
                } else {
                    self.sort = CooperativeSort::new(self.ordered.len());
                    self.phase = Phase::Sort;
                }
            }
            Phase::Sort => {
                if self.sort.step(&mut self.ordered, |a, b| {
                    let (left, right) = (&a.keyed, &b.keyed);
                    left.key
                        .target
                        .cmp(&right.key.target)
                        .then_with(|| {
                            left.key
                                .target_cleared_rows
                                .cmp(&right.key.target_cleared_rows)
                        })
                        .then_with(|| left.outcome.build.len().cmp(&right.outcome.build.len()))
                        .then_with(|| left.key.cmp(&right.key))
                        .then_with(|| left.outcome.mini.cmp(&right.outcome.mini))
                        .then_with(|| a.ordinal.cmp(&b.ordinal))
                }) {
                    self.sorted = std::mem::take(&mut self.ordered).into_iter();
                    self.phase = Phase::Deduplicate;
                }
            }
            Phase::Deduplicate => {
                if let Some(outcome) = self.sorted.next() {
                    if self
                        .ordered
                        .last()
                        .is_some_and(|last| last.keyed.key == outcome.keyed.key)
                    {
                        self.report.stages.exact_outcome_deduplications += 1;
                    } else {
                        let count = outcome.keyed.outcome.build.len();
                        self.minimum =
                            Some(self.minimum.map_or(count, |minimum| minimum.min(count)));
                        self.ordered.push(outcome);
                    }
                } else {
                    self.phase = Phase::Minimum;
                }
            }
            Phase::Minimum => {
                if self.index == self.ordered.len() {
                    self.sorted = std::mem::take(&mut self.ordered).into_iter();
                    self.phase = Phase::Partition;
                } else if self.policy == MinimalityPolicy::MinimumPieceCount {
                    self.ordered[self.index].keep =
                        Some(self.ordered[self.index].keyed.outcome.build.len()) == self.minimum;
                    self.index += 1;
                } else {
                    if self.index != 0
                        && (self.ordered[self.index - 1].keyed.key.target
                            != self.ordered[self.index].keyed.key.target
                            || self.ordered[self.index - 1].keyed.key.target_cleared_rows
                                != self.ordered[self.index].keyed.key.target_cleared_rows)
                    {
                        self.group = self.index;
                        self.known = self.index;
                    }
                    if self.known == self.index {
                        self.index += 1;
                        self.known = self.group;
                    } else if multiset_subset(
                        &self.ordered[self.known].keyed.key.operations,
                        &self.ordered[self.index].keyed.key.operations,
                    ) {
                        self.ordered[self.index].keep = false;
                        self.report.stages.exact_outcome_deduplications += 1;
                        self.index += 1;
                        self.known = self.group;
                    } else {
                        self.known += 1;
                    }
                }
            }
            Phase::Partition => {
                if let Some(outcome) = self.sorted.next() {
                    if outcome.keep {
                        let count = outcome.keyed.outcome.build.len() as u8;
                        self.report.minimum_placements = Some(
                            self.report
                                .minimum_placements
                                .map_or(count, |minimum| minimum.min(count)),
                        );
                        if outcome.keyed.outcome.mini {
                            self.mini.push(outcome.keyed);
                        } else {
                            self.regular.push(outcome.keyed);
                        }
                    }
                } else {
                    self.sort = CooperativeSort::new(self.regular.len());
                    self.phase = Phase::SortRegular;
                }
            }
            Phase::SortRegular => {
                if self.sort.step(&mut self.regular, |a, b| a.key.cmp(&b.key)) {
                    self.sort = CooperativeSort::new(self.mini.len());
                    self.phase = Phase::SortMini;
                }
            }
            Phase::SortMini => {
                if self.sort.step(&mut self.mini, |a, b| a.key.cmp(&b.key)) {
                    self.publish = std::mem::take(&mut self.regular).into_iter();
                    self.phase = Phase::Publish;
                }
            }
            Phase::Publish => {
                if let Some(outcome) = self.publish.next() {
                    if let Some(layer) = self
                        .report
                        .layers
                        .iter_mut()
                        .find(|layer| usize::from(layer.depth) == outcome.outcome.build.len())
                    {
                        if outcome.outcome.mini {
                            layer.accepted_mini += 1;
                        } else {
                            layer.accepted_regular += 1;
                        }
                    }
                    if outcome.outcome.mini {
                        self.report.mini.push(outcome.outcome);
                    } else {
                        self.report.regular.push(outcome.outcome);
                    }
                } else if !self.mini.is_empty() {
                    self.publish = std::mem::take(&mut self.mini).into_iter();
                } else {
                    self.phase = Phase::Complete;
                    return true;
                }
            }
            Phase::Complete => return true,
        }
        false
    }

    pub(crate) fn finish(self) -> Option<SpinStructureReport> {
        matches!(self.phase, Phase::Complete).then_some(self.report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        PieceInventory, SpinStructureMode, SpinStructureQuery, SpinStructureSearcher,
        StructureBoard,
    };
    use clearra_core_domain::piece::piece_kind::PieceKind;
    #[test]
    fn cooperative_minimum_preserves_stable_representatives_and_subsets() {
        let mut query = SpinStructureQuery::new(
            PieceInventory::from_pieces([PieceKind::T, PieceKind::I, PieceKind::O]).unwrap(),
            SpinStructureMode::TSpins,
        );
        query.height = 4;
        query.fill_top = 4;
        query.max_placements = Some(3);
        query.line_requirement = crate::SpinLineRequirement::Any;
        query.initial_board = [(4, 2), (6, 2), (4, 0)]
            .into_iter()
            .fold(StructureBoard::EMPTY, |board, (x, y)| {
                board.with_cell(x, y).unwrap()
            });
        let report = SpinStructureSearcher::run(query).unwrap();
        for policy in [
            MinimalityPolicy::SubsetMinimal,
            MinimalityPolicy::MinimumPieceCount,
        ] {
            let mut source = report.clone();
            source.query.as_mut().unwrap().minimality = policy;
            source.regular.extend(source.regular.clone());
            source.mini.extend(source.mini.clone());
            let mut expected = finalize(source.clone());
            crate::structural_search::refresh_layer_acceptance(&mut expected);
            let mut cursor = MinimumCursor::new(source);
            while !cursor.step() {}
            assert_eq!(cursor.finish().unwrap(), expected);
        }
    }
}
