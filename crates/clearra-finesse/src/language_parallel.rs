//! Request-wide scheduling around the existing exact vector/observation DPs.
//! No queue class or geometry language is copied to create a worker partition.

use std::sync::Mutex;

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    parallel_work::{try_for_each_chunk, ParallelWorkError},
};

use super::*;

impl QueueClassProductEvaluator<'_> {
    /// Evaluate the original 64-class vector products on the request's shared
    /// compute budget. The caller must admit N private product workspaces.
    pub fn oracle_with_control(
        &self,
        classes: &QueueClassSet,
        initial_hold: Option<PieceKind>,
        control: &ExecutionControl,
    ) -> Result<OracleQueueEvaluation, ParallelWorkError<GeometryLanguageError>> {
        if control.is_cancelled() {
            return Err(ParallelWorkError::Cancelled);
        }
        let costs = Mutex::new(
            QueueCostTable::unreachable(classes.classes.len())
                .map_err(ParallelWorkError::Worker)?,
        );
        try_for_each_chunk(
            control,
            classes.classes.len().div_ceil(DENSE_LANES),
            1,
            || Ok(()),
            |_, range| {
                for bucket in range {
                    let start = bucket * DENSE_LANES;
                    let end = (start + DENSE_LANES).min(classes.classes.len());
                    let value = self.oracle_product_chunk_with_cancel_and_observer(
                        &classes.classes[start..end],
                        initial_hold,
                        &mut || control.is_cancelled(),
                        &mut |_| {},
                    )?;
                    let mut ordered = CostVector64::unreachable(end - start)?;
                    for lane in 0..value.len() {
                        if let Some(cost) = value.get(lane).flatten() {
                            ordered.set_min(lane, cost)?;
                        }
                    }
                    costs
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .buckets[bucket] = ordered;
                }
                Ok(())
            },
        )?;
        Ok(OracleQueueEvaluation {
            costs: costs
                .into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            universe: classes.metadata,
        })
    }

    /// Independent initial observations may choose independent policies. Each
    /// complete observation class stays intact inside the existing exact DP;
    /// future hidden queues are never split into clairvoyant subproblems.
    pub fn visible_seven_with_control(
        &self,
        classes: &QueueClassSet,
        initial_hold: Option<PieceKind>,
        control: &ExecutionControl,
    ) -> Result<VisibleSevenEvaluation, ParallelWorkError<GeometryLanguageError>> {
        if classes.fully_revealed_by_visible_seven() {
            let oracle = self.oracle_with_control(classes, initial_hold, control)?;
            let score = score_costs(classes, &oracle.costs);
            return Ok(VisibleSevenEvaluation {
                costs: oracle.costs,
                successful_probability_mass: score.probability_mass,
                successful_unique_queue_count: score.unique_queue_count,
                total_inputs: score.total_inputs,
                universe: oracle.universe,
            });
        }
        if control.is_cancelled() {
            return Err(ParallelWorkError::Cancelled);
        }
        let all = (0..classes.classes.len()).collect::<Vec<_>>();
        let groups = split_observations(&all, classes, 0, initial_hold)
            .into_iter()
            .map(|members| Mutex::new((Some(members.into_boxed_slice()), None)))
            .collect::<Vec<_>>();
        drop(all);
        try_for_each_chunk(
            control,
            groups.len(),
            1,
            || Ok(BTreeMap::new()),
            |memo, range| {
                for index in range {
                    let members = groups[index]
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .0
                        .take()
                        .expect("each original observation has one ordinal");
                    let value = self.policy_value_with_cancel(
                        classes,
                        PolicyState {
                            node: self.language.root,
                            cursor: 0,
                            hold: initial_hold,
                            members,
                        },
                        memo,
                        &mut || control.is_cancelled(),
                    )?;
                    groups[index]
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .1 = Some(value.outcomes);
                }
                Ok(())
            },
        )?;
        let mut costs = QueueCostTable::unreachable(classes.classes.len())
            .map_err(ParallelWorkError::Worker)?;
        // The compact vector's bucket ordering also belongs to deterministic
        // output. Apply the original observation order, independently of when
        // its exact policy finished, then sum in original queue-class order.
        for group in groups {
            let outcomes = group
                .into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .1
                .expect("each initial observation completed");
            for outcome in outcomes.iter() {
                costs
                    .set_min(outcome.class_index, outcome.cost)
                    .map_err(ParallelWorkError::Worker)?;
            }
        }
        let score = score_costs(classes, &costs);
        Ok(VisibleSevenEvaluation {
            costs,
            successful_probability_mass: score.probability_mass,
            successful_unique_queue_count: score.unique_queue_count,
            total_inputs: score.total_inputs,
            universe: classes.metadata,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controlled_vector_and_observation_products_preserve_every_class() {
        let mut nodes = Vec::new();
        for depth in 0..=9 {
            let edges = if depth == 9 {
                Vec::new()
            } else {
                PieceKind::STANDARD_TETROMINOES
                    .iter()
                    .copied()
                    .enumerate()
                    .map(|(index, piece)| {
                        CostedGeometryEdge::new(
                            piece,
                            GeometryNodeId::new(depth + 1),
                            (index + depth as usize + 1) as u32,
                            index as u32,
                        )
                    })
                    .collect()
            };
            nodes.push(GeometryLanguageNode::new(depth as u16, depth == 9, edges));
        }
        let language = CostedGeometryLanguage::new(GeometryNodeId::new(0), nodes).unwrap();
        let patterns = (0..196)
            .map(|index| {
                let mut queue = vec![PieceKind::I; 9];
                queue[0] = PieceKind::STANDARD_TETROMINOES[index % 7];
                queue[1] = PieceKind::STANDARD_TETROMINOES[(index / 7) % 7];
                queue[8] = PieceKind::STANDARD_TETROMINOES[index / 49];
                QueuePattern::new(
                    PatternId::new(index),
                    queue,
                    ProbabilityValue::new(1.0 / 196.0).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let classes = QueueClassSet::group(&patterns, true).unwrap();
        assert!(!classes.fully_revealed_by_visible_seven());
        for hold in [false, true] {
            let evaluator = QueueClassProductEvaluator::new(&language).with_hold_enabled(hold);
            let oracle = evaluator.oracle(&classes, Some(PieceKind::T)).unwrap();
            let visible = evaluator
                .visible_seven(&classes, Some(PieceKind::T))
                .unwrap();
            for workers in [1, 2, 4, 8] {
                let control = ExecutionControl::default().with_parallelism(workers);
                assert_eq!(
                    evaluator
                        .oracle_with_control(&classes, Some(PieceKind::T), &control)
                        .unwrap(),
                    oracle
                );
                assert_eq!(
                    evaluator
                        .visible_seven_with_control(&classes, Some(PieceKind::T), &control)
                        .unwrap(),
                    visible
                );
            }
        }
        let control = ExecutionControl::default().with_parallelism(8);
        control.cancellation.handle().cancel();
        assert!(matches!(
            QueueClassProductEvaluator::new(&language)
                .oracle_with_control(&classes, None, &control),
            Err(ParallelWorkError::Cancelled)
        ));
    }
}
