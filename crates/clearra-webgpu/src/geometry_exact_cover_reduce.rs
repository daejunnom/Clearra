use std::mem::size_of;

const EMPTY_INDEX: u32 = u32::MAX;
const INITIAL_BUCKET_COUNT: usize = 256;
const PARALLEL_REDUCE_MIN_RECORDS: usize = 16_384;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TraceRecord {
    pub(crate) parent_index: u32,
    pub(crate) operation_index: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TraceLayerSegment {
    state_begin: usize,
    incoming_heads: Vec<u32>,
    edges: Vec<LinkedTraceEdge>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TraceLayer {
    segments: Vec<TraceLayerSegment>,
    state_count: usize,
}

pub(crate) struct TraceEdgeIter<'a> {
    edges: &'a [LinkedTraceEdge],
    cursor: u32,
}

impl Iterator for TraceEdgeIter<'_> {
    type Item = TraceRecord;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor == EMPTY_INDEX {
            return None;
        }
        let edge = *self.edges.get(self.cursor as usize)?;
        self.cursor = edge.next;
        Some(edge.trace)
    }
}

impl TraceLayer {
    pub(crate) const fn state_count(&self) -> usize {
        self.state_count
    }

    pub(crate) fn incoming_edges(&self, state_index: usize) -> Option<TraceEdgeIter<'_>> {
        if state_index >= self.state_count {
            return None;
        }
        let segment = self.segments.iter().find(|segment| {
            state_index >= segment.state_begin
                && state_index < segment.state_begin + segment.incoming_heads.len()
        })?;
        let local_index = state_index - segment.state_begin;
        Some(TraceEdgeIter {
            edges: &segment.edges,
            cursor: segment.incoming_heads[local_index],
        })
    }

    pub(crate) fn resident_bytes(&self) -> usize {
        self.segments.iter().fold(
            self.segments
                .capacity()
                .saturating_mul(size_of::<TraceLayerSegment>()),
            |total, segment| {
                total
                    .saturating_add(
                        segment
                            .incoming_heads
                            .capacity()
                            .saturating_mul(size_of::<u32>()),
                    )
                    .saturating_add(
                        segment
                            .edges
                            .capacity()
                            .saturating_mul(size_of::<LinkedTraceEdge>()),
                    )
            },
        )
    }
}

pub(crate) struct ReducedFrontier<const STATE_WORDS: usize> {
    pub(crate) state_segments: Vec<Vec<u32>>,
    pub(crate) trace_layer: TraceLayer,
}

pub(crate) struct ExactFrontierReducer<const STATE_WORDS: usize> {
    max_states: usize,
    states: Vec<u32>,
    incoming_heads: Vec<u32>,
    trace_edges: Vec<LinkedTraceEdge>,
    bucket_heads: Vec<u32>,
    next_indices: Vec<u32>,
}

pub(crate) enum ExactFrontierReduceStage<const STATE_WORDS: usize> {
    Serial(ExactFrontierReducer<STATE_WORDS>),
    Sharded {
        max_states: usize,
        reducers: Vec<ExactFrontierReducer<STATE_WORDS>>,
        partition_indices: Vec<u32>,
        partition_offsets: Vec<usize>,
        partition_cursors: Vec<usize>,
        partition_results: Vec<Result<(), FrontierReduceError>>,
        peak_partition_bytes: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LinkedTraceEdge {
    trace: TraceRecord,
    next: u32,
}

#[derive(Clone, Copy)]
enum TraceInsertPolicy {
    #[cfg(test)]
    ExactDeduplicate,
    AppendGenerated,
}

#[derive(Clone, Copy)]
enum TraceRecords<'a> {
    #[cfg(test)]
    Typed(&'a [TraceRecord]),
    Words(&'a [u32]),
}

impl TraceRecords<'_> {
    fn is_well_formed(self) -> bool {
        match self {
            #[cfg(test)]
            Self::Typed(_) => true,
            Self::Words(words) => words.len().is_multiple_of(2),
        }
    }

    fn len(self) -> usize {
        match self {
            #[cfg(test)]
            Self::Typed(records) => records.len(),
            Self::Words(words) => words.len() / 2,
        }
    }

    fn get(self, index: usize) -> Option<TraceRecord> {
        match self {
            #[cfg(test)]
            Self::Typed(records) => records.get(index).copied(),
            Self::Words(words) => {
                let begin = index.checked_mul(2)?;
                let pair = words.get(begin..begin + 2)?;
                Some(TraceRecord {
                    parent_index: pair[0],
                    operation_index: pair[1],
                })
            }
        }
    }
}

impl<const STATE_WORDS: usize> ExactFrontierReduceStage<STATE_WORDS> {
    pub(crate) fn new(
        max_states: usize,
        host_workers: usize,
        expected_record_count: usize,
    ) -> Result<Self, FrontierReduceError> {
        let available_workers = std::thread::available_parallelism()
            .map_or(1, usize::from)
            .max(1);
        let worker_count = host_workers
            .max(1)
            .min(available_workers)
            .min(max_states.max(1));
        if cfg!(target_arch = "wasm32")
            || worker_count == 1
            || expected_record_count < PARALLEL_REDUCE_MIN_RECORDS
        {
            return ExactFrontierReducer::new(max_states).map(Self::Serial);
        }

        let mut reducers = Vec::new();
        reducers
            .try_reserve_exact(worker_count)
            .map_err(|_| FrontierReduceError::AllocationFailed)?;
        for _ in 0..worker_count {
            reducers.push(ExactFrontierReducer::new(max_states)?);
        }
        Ok(Self::Sharded {
            max_states,
            reducers,
            partition_indices: Vec::new(),
            partition_offsets: Vec::new(),
            partition_cursors: Vec::new(),
            partition_results: Vec::new(),
            peak_partition_bytes: 0,
        })
    }

    #[cfg(test)]
    pub(crate) fn extend(
        &mut self,
        state_words: &[u32],
        traces: &[TraceRecord],
    ) -> Result<(), FrontierReduceError> {
        self.extend_with_policy(
            state_words,
            TraceRecords::Typed(traces),
            TraceInsertPolicy::ExactDeduplicate,
        )
    }

    pub(crate) fn extend_generated_words(
        &mut self,
        state_words: &[u32],
        trace_words: &[u32],
    ) -> Result<(), FrontierReduceError> {
        // A dispatch emits each global parent/operation pair once. Appending
        // those edges avoids a quadratic per-state scan; final candidates are
        // still canonicalized and exactly deduplicated.
        self.extend_with_policy(
            state_words,
            TraceRecords::Words(trace_words),
            TraceInsertPolicy::AppendGenerated,
        )
    }

    fn extend_with_policy(
        &mut self,
        state_words: &[u32],
        traces: TraceRecords<'_>,
        trace_policy: TraceInsertPolicy,
    ) -> Result<(), FrontierReduceError> {
        match self {
            Self::Serial(reducer) => reducer.extend_with_policy(state_words, traces, trace_policy),
            Self::Sharded {
                max_states,
                reducers,
                partition_indices,
                partition_offsets,
                partition_cursors,
                partition_results,
                peak_partition_bytes,
            } => {
                let partition_bytes = extend_sharded(
                    reducers,
                    state_words,
                    traces,
                    *max_states,
                    partition_indices,
                    partition_offsets,
                    partition_cursors,
                    partition_results,
                    trace_policy,
                )?;
                *peak_partition_bytes = (*peak_partition_bytes).max(partition_bytes);
                Ok(())
            }
        }
    }

    pub(crate) fn state_count(&self) -> usize {
        match self {
            Self::Serial(reducer) => reducer.state_count(),
            Self::Sharded { reducers, .. } => reducers.iter().fold(0_usize, |total, reducer| {
                total.saturating_add(reducer.state_count())
            }),
        }
    }

    pub(crate) fn peak_host_bytes(&self) -> usize {
        match self {
            Self::Serial(reducer) => reducer.resident_bytes(),
            Self::Sharded {
                reducers,
                peak_partition_bytes,
                ..
            } => reducers
                .iter()
                .fold(*peak_partition_bytes, |total, reducer| {
                    total.saturating_add(reducer.resident_bytes())
                }),
        }
    }

    pub(crate) fn finish(self) -> Result<ReducedFrontier<STATE_WORDS>, FrontierReduceError> {
        match self {
            Self::Serial(reducer) => reducer.finish(),
            Self::Sharded {
                max_states,
                reducers,
                ..
            } => finish_sharded(reducers, max_states),
        }
    }
}

impl<const STATE_WORDS: usize> ExactFrontierReducer<STATE_WORDS> {
    pub(crate) fn new(max_states: usize) -> Result<Self, FrontierReduceError> {
        if STATE_WORDS == 0 || max_states == 0 || max_states > u32::MAX as usize {
            return Err(FrontierReduceError::InvalidInput);
        }
        let bucket_count = INITIAL_BUCKET_COUNT.min(max_states).next_power_of_two();
        let mut bucket_heads = Vec::new();
        bucket_heads
            .try_reserve_exact(bucket_count)
            .map_err(|_| FrontierReduceError::AllocationFailed)?;
        bucket_heads.resize(bucket_count, EMPTY_INDEX);
        Ok(Self {
            max_states,
            states: Vec::new(),
            incoming_heads: Vec::new(),
            trace_edges: Vec::new(),
            bucket_heads,
            next_indices: Vec::new(),
        })
    }

    #[cfg(test)]
    pub(crate) fn extend(
        &mut self,
        state_words: &[u32],
        traces: &[TraceRecord],
    ) -> Result<(), FrontierReduceError> {
        self.extend_with_policy(
            state_words,
            TraceRecords::Typed(traces),
            TraceInsertPolicy::ExactDeduplicate,
        )
    }

    fn extend_with_policy(
        &mut self,
        state_words: &[u32],
        traces: TraceRecords<'_>,
        trace_policy: TraceInsertPolicy,
    ) -> Result<(), FrontierReduceError> {
        if !traces.is_well_formed()
            || !state_words.len().is_multiple_of(STATE_WORDS)
            || state_words.len() / STATE_WORDS != traces.len()
        {
            return Err(FrontierReduceError::InvalidInput);
        }
        self.reserve_for_records(traces.len())?;
        for (index, words) in state_words.chunks_exact(STATE_WORDS).enumerate() {
            let state: [u32; STATE_WORDS] = words
                .try_into()
                .map_err(|_| FrontierReduceError::InvalidInput)?;
            let trace = traces.get(index).ok_or(FrontierReduceError::InvalidInput)?;
            self.insert(state, trace, trace_policy)?;
        }
        Ok(())
    }

    fn extend_indices(
        &mut self,
        state_words: &[u32],
        traces: TraceRecords<'_>,
        indices: &[u32],
        trace_policy: TraceInsertPolicy,
    ) -> Result<(), FrontierReduceError> {
        self.reserve_for_records(indices.len())?;
        for compact_index in indices.iter().copied() {
            let index = compact_index as usize;
            let begin = index
                .checked_mul(STATE_WORDS)
                .ok_or(FrontierReduceError::InvalidInput)?;
            let end = begin
                .checked_add(STATE_WORDS)
                .ok_or(FrontierReduceError::InvalidInput)?;
            let state: [u32; STATE_WORDS] = state_words
                .get(begin..end)
                .ok_or(FrontierReduceError::InvalidInput)?
                .try_into()
                .map_err(|_| FrontierReduceError::InvalidInput)?;
            let trace = traces.get(index).ok_or(FrontierReduceError::InvalidInput)?;
            self.insert(state, trace, trace_policy)?;
        }
        Ok(())
    }

    pub(crate) fn state_count(&self) -> usize {
        self.states.len() / STATE_WORDS
    }

    pub(crate) fn resident_bytes(&self) -> usize {
        self.states
            .capacity()
            .saturating_mul(size_of::<u32>())
            .saturating_add(
                self.incoming_heads
                    .capacity()
                    .saturating_mul(size_of::<u32>()),
            )
            .saturating_add(
                self.trace_edges
                    .capacity()
                    .saturating_mul(size_of::<LinkedTraceEdge>()),
            )
            .saturating_add(
                self.bucket_heads
                    .capacity()
                    .saturating_mul(size_of::<u32>()),
            )
            .saturating_add(
                self.next_indices
                    .capacity()
                    .saturating_mul(size_of::<u32>()),
            )
    }

    pub(crate) fn finish(self) -> Result<ReducedFrontier<STATE_WORDS>, FrontierReduceError> {
        let state_count = self.state_count();
        Ok(ReducedFrontier {
            state_segments: vec![self.states],
            trace_layer: TraceLayer {
                segments: vec![TraceLayerSegment {
                    state_begin: 0,
                    incoming_heads: self.incoming_heads,
                    edges: self.trace_edges,
                }],
                state_count,
            },
        })
    }

    fn insert(
        &mut self,
        state: [u32; STATE_WORDS],
        trace: TraceRecord,
        trace_policy: TraceInsertPolicy,
    ) -> Result<(), FrontierReduceError> {
        let bucket = state_bucket(&state, self.bucket_heads.len());
        let mut existing = self.bucket_heads[bucket];
        while existing != EMPTY_INDEX {
            let index = existing as usize;
            let begin = index * STATE_WORDS;
            if self.states[begin..begin + STATE_WORDS] == state {
                self.insert_trace(index, trace, trace_policy)?;
                return Ok(());
            }
            existing = self.next_indices[index];
        }

        let state_count = self.state_count();
        if state_count >= self.max_states {
            return Err(FrontierReduceError::CapacityExceeded {
                generated_state_count: state_count.saturating_add(1) as u32,
            });
        }
        let bucket = state_bucket(&state, self.bucket_heads.len());
        let index = state_count;
        self.states.extend_from_slice(&state);
        self.incoming_heads.push(EMPTY_INDEX);
        self.next_indices.push(self.bucket_heads[bucket]);
        self.bucket_heads[bucket] = index as u32;
        self.insert_trace(index, trace, trace_policy)?;
        Ok(())
    }

    fn insert_trace(
        &mut self,
        state_index: usize,
        trace: TraceRecord,
        _trace_policy: TraceInsertPolicy,
    ) -> Result<(), FrontierReduceError> {
        #[cfg(test)]
        if matches!(_trace_policy, TraceInsertPolicy::ExactDeduplicate) {
            let mut cursor = self.incoming_heads[state_index];
            while cursor != EMPTY_INDEX {
                let edge = self.trace_edges[cursor as usize];
                if edge.trace == trace {
                    return Ok(());
                }
                cursor = edge.next;
            }
        }
        let edge_index = u32::try_from(self.trace_edges.len()).map_err(|_| {
            FrontierReduceError::CapacityExceeded {
                generated_state_count: self.state_count() as u32,
            }
        })?;
        self.trace_edges.push(LinkedTraceEdge {
            trace,
            next: self.incoming_heads[state_index],
        });
        self.incoming_heads[state_index] = edge_index;
        Ok(())
    }

    fn reserve_for_records(&mut self, record_count: usize) -> Result<(), FrontierReduceError> {
        let state_count = self.state_count();
        let possible_new_states = record_count.min(self.max_states.saturating_sub(state_count));
        let target_state_count = state_count.saturating_add(possible_new_states);
        self.ensure_bucket_capacity(target_state_count)?;
        self.states
            .try_reserve(possible_new_states.saturating_mul(STATE_WORDS))
            .map_err(|_| FrontierReduceError::AllocationFailed)?;
        self.incoming_heads
            .try_reserve(possible_new_states)
            .map_err(|_| FrontierReduceError::AllocationFailed)?;
        self.next_indices
            .try_reserve(possible_new_states)
            .map_err(|_| FrontierReduceError::AllocationFailed)?;
        self.trace_edges
            .try_reserve(record_count)
            .map_err(|_| FrontierReduceError::AllocationFailed)?;
        Ok(())
    }

    fn ensure_bucket_capacity(
        &mut self,
        target_state_count: usize,
    ) -> Result<(), FrontierReduceError> {
        let desired = target_state_count
            .saturating_mul(4)
            .div_ceil(3)
            .max(1)
            .checked_next_power_of_two()
            .ok_or(FrontierReduceError::AllocationFailed)?;
        let new_count = desired.max(
            INITIAL_BUCKET_COUNT
                .min(self.max_states)
                .next_power_of_two(),
        );
        if new_count <= self.bucket_heads.len() {
            return Ok(());
        }
        let mut grown = Vec::new();
        grown
            .try_reserve_exact(new_count)
            .map_err(|_| FrontierReduceError::AllocationFailed)?;
        grown.resize(new_count, EMPTY_INDEX);
        for (index, state_words) in self.states.chunks_exact(STATE_WORDS).enumerate() {
            let state: &[u32; STATE_WORDS] = state_words
                .try_into()
                .map_err(|_| FrontierReduceError::InvalidInput)?;
            let bucket = state_bucket(state, new_count);
            self.next_indices[index] = grown[bucket];
            grown[bucket] = index as u32;
        }
        self.bucket_heads = grown;
        Ok(())
    }
}

fn extend_sharded<const STATE_WORDS: usize>(
    reducers: &mut [ExactFrontierReducer<STATE_WORDS>],
    state_words: &[u32],
    traces: TraceRecords<'_>,
    max_states: usize,
    partition_indices: &mut Vec<u32>,
    partition_offsets: &mut Vec<usize>,
    partition_cursors: &mut Vec<usize>,
    partition_results: &mut Vec<Result<(), FrontierReduceError>>,
    trace_policy: TraceInsertPolicy,
) -> Result<usize, FrontierReduceError> {
    if reducers.is_empty()
        || !traces.is_well_formed()
        || !state_words.len().is_multiple_of(STATE_WORDS)
        || state_words.len() / STATE_WORDS != traces.len()
    {
        return Err(FrontierReduceError::InvalidInput);
    }

    partition_offsets.clear();
    partition_offsets
        .try_reserve(reducers.len().saturating_add(1))
        .map_err(|_| FrontierReduceError::AllocationFailed)?;
    partition_offsets.resize(reducers.len().saturating_add(1), 0_usize);
    for words in state_words.chunks_exact(STATE_WORDS) {
        let state: &[u32; STATE_WORDS] = words
            .try_into()
            .map_err(|_| FrontierReduceError::InvalidInput)?;
        let shard = state_shard(state, reducers.len());
        partition_offsets[shard + 1] = partition_offsets[shard + 1]
            .checked_add(1)
            .ok_or(FrontierReduceError::AllocationFailed)?;
    }
    for shard in 0..reducers.len() {
        partition_offsets[shard + 1] = partition_offsets[shard + 1]
            .checked_add(partition_offsets[shard])
            .ok_or(FrontierReduceError::AllocationFailed)?;
    }

    let record_count = traces.len();
    partition_indices.clear();
    partition_indices
        .try_reserve(record_count)
        .map_err(|_| FrontierReduceError::AllocationFailed)?;
    partition_indices.resize(record_count, 0_u32);
    partition_cursors.clear();
    partition_cursors
        .try_reserve(reducers.len())
        .map_err(|_| FrontierReduceError::AllocationFailed)?;
    partition_cursors.extend_from_slice(&partition_offsets[..reducers.len()]);
    for (index, words) in state_words.chunks_exact(STATE_WORDS).enumerate() {
        let state: &[u32; STATE_WORDS] = words
            .try_into()
            .map_err(|_| FrontierReduceError::InvalidInput)?;
        let shard = state_shard(state, reducers.len());
        let destination = partition_cursors[shard];
        partition_indices[destination] =
            u32::try_from(index).map_err(|_| FrontierReduceError::AllocationFailed)?;
        partition_cursors[shard] = destination + 1;
    }
    partition_results.clear();
    partition_results
        .try_reserve(reducers.len())
        .map_err(|_| FrontierReduceError::AllocationFailed)?;
    partition_results.resize(reducers.len(), Ok(()));
    let partition_bytes = partition_indices
        .capacity()
        .saturating_mul(size_of::<u32>())
        .saturating_add(
            partition_offsets
                .capacity()
                .saturating_mul(size_of::<usize>()),
        )
        .saturating_add(
            partition_cursors
                .capacity()
                .saturating_mul(size_of::<usize>()),
        )
        .saturating_add(
            partition_results
                .capacity()
                .saturating_mul(size_of::<Result<(), FrontierReduceError>>()),
        );
    std::thread::scope(|scope| {
        for (shard, (reducer, result)) in reducers
            .iter_mut()
            .zip(partition_results.iter_mut())
            .enumerate()
        {
            let begin = partition_offsets[shard];
            let end = partition_offsets[shard + 1];
            let partition = &partition_indices[begin..end];
            scope.spawn(move || {
                *result = reducer.extend_indices(state_words, traces, partition, trace_policy);
            });
        }
    });
    for result in partition_results.iter().copied() {
        result?;
    }

    let state_count = reducers.iter().fold(0_usize, |total, reducer| {
        total.saturating_add(reducer.state_count())
    });
    if state_count > max_states {
        return Err(FrontierReduceError::CapacityExceeded {
            generated_state_count: u32::try_from(state_count).unwrap_or(u32::MAX),
        });
    }
    Ok(partition_bytes)
}

fn finish_sharded<const STATE_WORDS: usize>(
    reducers: Vec<ExactFrontierReducer<STATE_WORDS>>,
    max_states: usize,
) -> Result<ReducedFrontier<STATE_WORDS>, FrontierReduceError> {
    let state_count = reducers.iter().fold(0_usize, |total, reducer| {
        total.saturating_add(reducer.state_count())
    });
    if state_count > max_states {
        return Err(FrontierReduceError::CapacityExceeded {
            generated_state_count: u32::try_from(state_count).unwrap_or(u32::MAX),
        });
    }
    let mut state_segments = Vec::new();
    state_segments
        .try_reserve_exact(reducers.len())
        .map_err(|_| FrontierReduceError::AllocationFailed)?;
    let mut trace_segments = Vec::new();
    trace_segments
        .try_reserve_exact(reducers.len())
        .map_err(|_| FrontierReduceError::AllocationFailed)?;
    let mut state_begin = 0usize;
    for reducer in reducers {
        let local_state_count = reducer.state_count();
        if local_state_count == 0 {
            continue;
        }
        let ExactFrontierReducer {
            states,
            incoming_heads,
            trace_edges,
            ..
        } = reducer;
        state_segments.push(states);
        trace_segments.push(TraceLayerSegment {
            state_begin,
            incoming_heads,
            edges: trace_edges,
        });
        state_begin = state_begin.saturating_add(local_state_count);
    }
    Ok(ReducedFrontier {
        state_segments,
        trace_layer: TraceLayer {
            segments: trace_segments,
            state_count,
        },
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FrontierReduceError {
    InvalidInput,
    AllocationFailed,
    CapacityExceeded { generated_state_count: u32 },
}

fn state_bucket<const STATE_WORDS: usize>(
    state: &[u32; STATE_WORDS],
    bucket_count: usize,
) -> usize {
    (state_hash(state) as usize) & (bucket_count - 1)
}

fn state_shard<const STATE_WORDS: usize>(state: &[u32; STATE_WORDS], shard_count: usize) -> usize {
    (state_hash(state) as usize) % shard_count
}

fn state_hash<const STATE_WORDS: usize>(state: &[u32; STATE_WORDS]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for word in state {
        hash ^= u64::from(*word);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical_frontier<const STATE_WORDS: usize>(
        frontier: ReducedFrontier<STATE_WORDS>,
    ) -> Vec<(Vec<u32>, Vec<(u32, u32)>)> {
        let mut canonical = Vec::new();
        let mut global_index = 0usize;
        for segment in &frontier.state_segments {
            for state in segment.chunks_exact(STATE_WORDS) {
                let mut edges = frontier
                    .trace_layer
                    .incoming_edges(global_index)
                    .expect("every reduced state has trace offsets")
                    .map(|edge| (edge.parent_index, edge.operation_index))
                    .collect::<Vec<_>>();
                edges.sort_unstable();
                canonical.push((state.to_vec(), edges));
                global_index += 1;
            }
        }
        canonical.sort_unstable();
        canonical
    }

    #[test]
    fn exact_reducer_preserves_distinct_states_in_one_hash_bucket() {
        let mut collision = None;
        'outer: for left in 0_u32..10_000 {
            for right in (left + 1)..10_000 {
                if state_bucket(&[left], 256) == state_bucket(&[right], 256) {
                    collision = Some((left, right));
                    break 'outer;
                }
            }
        }
        let (left, right) = collision.expect("test must find a bucket collision");
        let mut reducer = ExactFrontierReducer::<1>::new(512).expect("reducer");
        reducer
            .extend(
                &[left, right],
                &[
                    TraceRecord {
                        parent_index: 1,
                        operation_index: 10,
                    },
                    TraceRecord {
                        parent_index: 2,
                        operation_index: 20,
                    },
                ],
            )
            .expect("colliding states remain valid");

        assert_eq!(reducer.state_count(), 2);
        let reduced = reducer.finish().expect("reduced frontier");
        assert_eq!(reduced.state_segments, vec![vec![left, right]]);
    }

    #[test]
    fn exact_reducer_unions_all_distinct_incoming_trace_edges() {
        let state = [7_u32, 11_u32];
        let mut reducer = ExactFrontierReducer::<2>::new(8).expect("reducer");
        reducer
            .extend(
                &[state[0], state[1], state[0], state[1], state[0], state[1]],
                &[
                    TraceRecord {
                        parent_index: 1,
                        operation_index: 3,
                    },
                    TraceRecord {
                        parent_index: 2,
                        operation_index: 4,
                    },
                    TraceRecord {
                        parent_index: 1,
                        operation_index: 3,
                    },
                ],
            )
            .expect("trace union");

        assert_eq!(reducer.state_count(), 1);
        let reduced = reducer.finish().expect("reduced frontier");
        let mut edges = reduced
            .trace_layer
            .incoming_edges(0)
            .expect("state has incoming edges")
            .collect::<Vec<_>>();
        edges.sort_unstable_by_key(|edge| (edge.parent_index, edge.operation_index));
        assert_eq!(
            edges,
            vec![
                TraceRecord {
                    parent_index: 1,
                    operation_index: 3,
                },
                TraceRecord {
                    parent_index: 2,
                    operation_index: 4,
                },
            ]
        );
    }

    #[test]
    fn reducer_finish_transfers_frontier_and_trace_allocations_without_copying() {
        let mut reducer = ExactFrontierReducer::<2>::new(8).expect("reducer");
        reducer
            .extend(
                &[3, 5, 7, 11],
                &[
                    TraceRecord {
                        parent_index: 0,
                        operation_index: 2,
                    },
                    TraceRecord {
                        parent_index: 1,
                        operation_index: 4,
                    },
                ],
            )
            .expect("records");
        let state_allocation = reducer.states.as_ptr();
        let head_allocation = reducer.incoming_heads.as_ptr();
        let edge_allocation = reducer.trace_edges.as_ptr();

        let reduced = reducer.finish().expect("finish");
        assert_eq!(reduced.state_segments[0].as_ptr(), state_allocation);
        assert_eq!(
            reduced.trace_layer.segments[0].incoming_heads.as_ptr(),
            head_allocation
        );
        assert_eq!(
            reduced.trace_layer.segments[0].edges.as_ptr(),
            edge_allocation
        );
    }

    #[test]
    fn sharded_exact_reduce_matches_serial_state_and_trace_union() {
        let mut state_words = Vec::new();
        let mut traces = Vec::new();
        for index in 0_u32..4096 {
            let state = [index % 257, (index.wrapping_mul(17)) % 509];
            state_words.extend_from_slice(&state);
            traces.push(TraceRecord {
                parent_index: index % 31,
                operation_index: index % 19,
            });
            if index % 5 == 0 {
                state_words.extend_from_slice(&state);
                traces.push(TraceRecord {
                    parent_index: (index + 1) % 31,
                    operation_index: (index + 2) % 19,
                });
            }
        }

        let mut serial = ExactFrontierReduceStage::<2>::new(8192, 1, state_words.len() / 2)
            .expect("serial reducer");
        serial.extend(&state_words, &traces).expect("serial reduce");
        let mut sharded = ExactFrontierReduceStage::<2>::new(8192, 4, PARALLEL_REDUCE_MIN_RECORDS)
            .expect("sharded reducer");
        sharded
            .extend(&state_words, &traces)
            .expect("sharded reduce");

        assert_eq!(
            canonical_frontier(serial.finish().expect("serial frontier")),
            canonical_frontier(sharded.finish().expect("sharded frontier"))
        );
    }

    #[test]
    fn exact_reducer_capacity_never_discards_a_state_silently() {
        let mut reducer = ExactFrontierReducer::<1>::new(1).expect("reducer");
        let error = reducer
            .extend(
                &[1, 2],
                &[
                    TraceRecord {
                        parent_index: 0,
                        operation_index: 0,
                    },
                    TraceRecord {
                        parent_index: 0,
                        operation_index: 1,
                    },
                ],
            )
            .expect_err("capacity must be explicit");
        assert_eq!(
            error,
            FrontierReduceError::CapacityExceeded {
                generated_state_count: 2
            }
        );
    }
}
