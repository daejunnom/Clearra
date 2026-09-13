use super::*;
use core::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
struct Reader {
    calls: AtomicUsize,
    malformed: bool,
}
impl Pc4FiniteQueueReader for Reader {
    fn read_queue(&self, ordinal: usize) -> Result<Vec<Pc4GraphPiece>, Pc4FiniteQueueReadError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        if self.malformed {
            return Ok(vec![]);
        }
        Ok(vec![if ordinal < 2 {
            Pc4GraphPiece::I
        } else {
            Pc4GraphPiece::O
        }])
    }
}
fn nz(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}
fn setup(malformed: bool) -> (Arc<Reader>, Pc4FiniteQueueFamily) {
    let reader = Arc::new(Reader {
        calls: AtomicUsize::new(0),
        malformed,
    });
    let family = Pc4FiniteQueueFamily::uniform(reader.clone(), nz(4), nz(1));
    (reader, family)
}

#[test]
fn finite_queues_read_only_requested_ordinals_and_preserve_duplicate_probability() {
    let (reader, family) = setup(false);
    let mut cursor = family.cursor();
    assert_eq!(reader.calls.load(Ordering::Relaxed), 0);
    let page = family
        .next_page(&mut cursor, nz(2), 2, 2, &|| false)
        .unwrap();
    assert_eq!(reader.calls.load(Ordering::Relaxed), 2);
    assert_eq!(page[0].pieces, page[1].pieces);
    assert_ne!(page[0].rank, page[1].rank);
    assert_eq!(page[0].probability.denominator(), 4);
    assert_eq!(cursor.next_rank(), 2);
    let page2 = family
        .clone()
        .next_page(&mut cursor, nz(2), 2, 2, &|| false)
        .unwrap();
    assert!(cursor.is_exhausted());
    assert_eq!(reader.calls.load(Ordering::Relaxed), 4);
    let total = page
        .iter()
        .chain(page2.iter())
        .fold(Pc4ExactProbability::zero(), |sum, item| {
            sum.checked_add(item.probability).unwrap()
        });
    assert_eq!(total, Pc4ExactProbability::one());
}

#[test]
fn finite_queue_same_size_reader_cannot_consume_another_familys_cursor() {
    let (_, family) = setup(false);
    let (_, other) = setup(false);
    assert_ne!(family, other);
    let mut cursor = family.cursor();
    assert_eq!(
        other.next_page(&mut cursor, nz(1), 1, 1, &|| false),
        Err(Pc4BagRevealPageError::CursorMismatch)
    );
    assert_eq!(cursor.next_rank(), 0);
}

#[test]
fn finite_queue_budget_and_allocation_overflow_fail_before_reading() {
    let (reader, family) = setup(false);
    let mut cursor = family.cursor();
    assert!(matches!(
        family.next_page(&mut cursor, nz(2), 1, 2, &|| false),
        Err(Pc4BagRevealPageError::BudgetExceeded {
            kind: Pc4BagRevealPageBudgetKind::PageSequences,
            ..
        })
    ));
    assert!(matches!(
        family.next_page(&mut cursor, nz(2), 2, 1, &|| false),
        Err(Pc4BagRevealPageError::BudgetExceeded {
            kind: Pc4BagRevealPageBudgetKind::AllocatedPieces,
            ..
        })
    ));
    let huge = Pc4FiniteQueueFamily::uniform(reader.clone(), nz(2), NonZeroUsize::MAX);
    assert!(matches!(
        huge.next_page(&mut huge.cursor(), nz(2), 2, usize::MAX, &|| false),
        Err(Pc4BagRevealPageError::BudgetExceeded { .. })
    ));
    assert_eq!(reader.calls.load(Ordering::Relaxed), 0);
    assert_eq!(cursor.next_rank(), 0);
}

#[test]
fn finite_queue_malformed_row_and_late_cancellation_commit_no_cursor() {
    let (_, malformed) = setup(true);
    let mut cursor = malformed.cursor();
    assert_eq!(
        malformed.next_page(&mut cursor, nz(1), 1, 1, &|| false),
        Err(Pc4BagRevealPageError::FiniteQueue(
            Pc4FiniteQueueReadError::QueueLengthMismatch
        ))
    );
    assert_eq!(cursor.next_rank(), 0);
    let (_, family) = setup(false);
    let mut cursor = family.cursor();
    let calls = Cell::new(0);
    assert_eq!(
        family.next_page(&mut cursor, nz(2), 2, 2, &|| {
            calls.set(calls.get() + 1);
            calls.get() == 4
        }),
        Err(Pc4BagRevealPageError::Cancelled)
    );
    assert_eq!(cursor.next_rank(), 0);
    assert!(!cursor.is_exhausted());
}
