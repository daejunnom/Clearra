//! Bounded reuse of proved graph suffix facts, not paths or reveal outcomes.
//!
//! A successful page may publish immutable adjacency and exhausted-empty
//! suffix facts. Cursor clones share those facts, never traversal progress.
//! Losing the cache (capacity, allocation, poison) only repeats ordinary work.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::{Pc4GraphPiece, QualifiedPc4TargetIdentity, TerminalDepthContract};

// Count both keys and targets, rather than bounding only the number of states.
// This is an optional cache ceiling, never a product search/output limit.
const MAX_RETAINED_ELEMENTS: usize = 65_536;
const ENTRY_ELEMENTS: usize = 16;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SuffixKey {
    field_id: u32,
    queue_index: usize,
    remaining_queue: Vec<Pc4GraphPiece>,
    permits_early_terminal: bool,
}

impl SuffixKey {
    pub(crate) fn try_new(
        field_id: u32,
        queue_index: usize,
        remaining_queue: &[Pc4GraphPiece],
        contract: TerminalDepthContract,
    ) -> Option<Self> {
        let mut queue = Vec::new();
        queue.try_reserve_exact(remaining_queue.len()).ok()?;
        queue.extend_from_slice(remaining_queue);
        Some(Self {
            field_id,
            queue_index,
            remaining_queue: queue,
            permits_early_terminal: contract == TerminalDepthContract::PredicateMayTerminateEarly,
        })
    }

    fn elements(&self) -> usize {
        ENTRY_ELEMENTS.saturating_add(self.remaining_queue.len())
    }
}

#[derive(Clone, Debug)]
pub(crate) enum SuffixFact {
    // Every descendant was visited successfully with zero terminal paths.
    Empty,
    // Every raw edge was validated before sorting and target deduplication.
    // Targets are IDs, not separately cloned generation/qualification owners.
    Adjacency(Arc<[u32]>),
}

impl SuffixFact {
    fn elements(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Adjacency(targets) => targets.len(),
        }
    }
}

#[derive(Debug, Default)]
struct Facts {
    entries: HashMap<SuffixKey, SuffixFact>,
    retained_elements: usize,
}

impl Facts {
    fn insert(&mut self, key: SuffixKey, fact: SuffixFact, limit: usize) {
        let old = self.entries.get(&key);
        // An empty-suffix proof is stronger than its raw adjacency. Never
        // replace it with a weaker fact from another cursor's successful page.
        if matches!(old, Some(SuffixFact::Empty)) {
            return;
        }
        let old_elements = old.map_or(0, |old| key.elements() + old.elements());
        let next_elements = self
            .retained_elements
            .saturating_sub(old_elements)
            .saturating_add(key.elements())
            .saturating_add(fact.elements());
        if next_elements > limit || (old.is_none() && self.entries.try_reserve(1).is_err()) {
            return;
        }
        self.entries.insert(key, fact);
        self.retained_elements = next_elements;
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FixedQueueSuffixMemo {
    target: QualifiedPc4TargetIdentity,
    limit: usize,
    facts: Arc<Mutex<Facts>>,
}

impl FixedQueueSuffixMemo {
    pub(crate) fn new(target: &QualifiedPc4TargetIdentity, work_limit: usize) -> Self {
        Self {
            target: target.clone(),
            limit: work_limit.min(MAX_RETAINED_ELEMENTS),
            facts: Arc::new(Mutex::new(Facts::default())),
        }
    }

    pub(crate) fn matches_target(&self, target: &QualifiedPc4TargetIdentity) -> bool {
        &self.target == target
    }

    pub(crate) fn page(&self) -> SuffixMemoPage {
        SuffixMemoPage {
            shared: self.clone(),
            staged: Facts::default(),
        }
    }
}

pub(crate) struct SuffixMemoPage {
    shared: FixedQueueSuffixMemo,
    staged: Facts,
}

impl SuffixMemoPage {
    pub(crate) fn key(
        &self,
        field_id: u32,
        queue_index: usize,
        remaining_queue: &[Pc4GraphPiece],
        contract: TerminalDepthContract,
    ) -> Option<SuffixKey> {
        if ENTRY_ELEMENTS.saturating_add(remaining_queue.len()) > self.shared.limit {
            return None;
        }
        SuffixKey::try_new(field_id, queue_index, remaining_queue, contract)
    }

    pub(crate) fn get(&self, key: &SuffixKey) -> Option<SuffixFact> {
        self.staged
            .entries
            .get(key)
            .cloned()
            .or_else(|| self.shared.facts.lock().ok()?.entries.get(key).cloned())
    }

    pub(crate) fn stage_empty(&mut self, key: SuffixKey) {
        self.staged
            .insert(key, SuffixFact::Empty, self.shared.limit);
    }

    pub(crate) fn stage_adjacency(&mut self, key: SuffixKey, targets: Vec<u32>) {
        self.staged.insert(
            key,
            SuffixFact::Adjacency(targets.into()),
            self.shared.limit,
        );
    }

    // The caller must perform its final page guard before publishing facts.
    pub(crate) fn commit(self) {
        if let Ok(mut shared) = self.shared.facts.lock() {
            for (key, fact) in self.staged.entries {
                shared.insert(key, fact, self.shared.limit);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{manifest::tests::qualified_target_identity, Pc4RuleProfile, Pc4TerminalUseCase};

    fn memo(limit: usize) -> FixedQueueSuffixMemo {
        FixedQueueSuffixMemo::new(
            &qualified_target_identity(
                "suffix-cache-generation",
                "suffix-cache-manifest",
                Pc4RuleProfile::Srs,
                Pc4TerminalUseCase::PcSearch,
                4,
            ),
            limit,
        )
    }

    #[test]
    fn suffix_keys_bind_depth_queue_and_terminal_contract_without_binding_prefix() {
        let key = |index, queue: &[Pc4GraphPiece], contract| {
            SuffixKey::try_new(7, index, queue, contract).unwrap()
        };
        let exhausted = TerminalDepthContract::QueueExhaustedOnly;
        let early = TerminalDepthContract::PredicateMayTerminateEarly;
        let base = key(3, &[Pc4GraphPiece::I], exhausted);
        assert_ne!(base, key(4, &[Pc4GraphPiece::I], exhausted));
        assert_ne!(base, key(3, &[Pc4GraphPiece::O], exhausted));
        assert_ne!(base, key(3, &[Pc4GraphPiece::I], early));
        assert_eq!(base, key(3, &[Pc4GraphPiece::I], exhausted));
    }

    #[test]
    fn suffix_facts_are_bounded_and_only_committed_pages_are_reused() {
        let memo = memo(40);
        let mut abandoned = memo.page();
        let key = abandoned
            .key(
                7,
                0,
                &[Pc4GraphPiece::I],
                TerminalDepthContract::QueueExhaustedOnly,
            )
            .unwrap();
        abandoned.stage_empty(key.clone());
        drop(abandoned);
        assert!(memo.page().get(&key).is_none());

        let mut page = memo.page();
        for field in 0..100 {
            let key = page
                .key(
                    field,
                    0,
                    &[Pc4GraphPiece::I],
                    TerminalDepthContract::QueueExhaustedOnly,
                )
                .unwrap();
            page.stage_adjacency(key, vec![1, 2]);
        }
        assert!(page.staged.retained_elements <= 40);
        assert_eq!(page.staged.entries.len(), 2);
        page.commit();
        let shared = memo.facts.lock().unwrap();
        assert!(shared.retained_elements <= 40);
        assert_eq!(shared.entries.len(), 2);
    }

    #[test]
    fn a_later_adjacency_page_cannot_weaken_an_exhausted_empty_proof() {
        let memo = memo(100);
        let mut first = memo.page();
        let key = first
            .key(
                7,
                0,
                &[Pc4GraphPiece::I],
                TerminalDepthContract::QueueExhaustedOnly,
            )
            .unwrap();
        first.stage_empty(key.clone());
        first.commit();
        let mut second = memo.page();
        second.stage_adjacency(key.clone(), vec![8, 9]);
        second.commit();
        assert!(matches!(memo.page().get(&key), Some(SuffixFact::Empty)));
    }
}
