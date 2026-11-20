//! Test-only CEGIS experiment: keep the actual predecessor cuts instead of
//! forcing every occupied sample cell to remain occupied.

use super::*;
use crate::reachability_reference::reference_spawn_lock_anchors;

const MAX_CLAUSES: usize = 32;
const MAX_EDGE_VISITS: usize = 512;

#[derive(Clone, Copy, Eq, PartialEq)]
enum FrontierClause {
    Translation(u16),
    Rotation(ReverseRotationSource),
}

impl FrontierClause {
    fn source(self) -> u16 {
        match self {
            Self::Translation(source) => source,
            Self::Rotation(rotation) => rotation.source,
        }
    }

    fn excludes_edge(self, template: &ReachabilityTemplate, board: u64) -> bool {
        let mask = template.state_masks[usize::from(self.source())];
        mask == INVALID_STATE_MASK
            || board & mask != 0
            || matches!(self, Self::Rotation(rotation)
                if !reverse_rotation_is_first_success(template, board, rotation))
    }
}

fn incoming_clauses(
    template: &ReachabilityTemplate,
    target: u16,
) -> impl Iterator<Item = FrontierClause> + '_ {
    let target = usize::from(target);
    let rotations = &template.reverse_rotation_sources[template.reverse_rotation_offsets[target]
        as usize
        ..template.reverse_rotation_offsets[target + 1] as usize];
    template.reverse_translation_sources[target]
        .into_iter()
        .filter(|&source| source != INVALID_STATE_INDEX)
        .map(FrontierClause::Translation)
        .chain(rotations.iter().copied().map(FrontierClause::Rotation))
}

/// One conjunction over a bounded cut of an immutable exact template.
/// Each rotation clause permits either a colliding source or a geometrically
/// valid earlier kick which stays clear. Translation clauses only permit a
/// colliding source. Unlike a single occupancy interval, distinct clauses may
/// be satisfied by different cell choices on different boards.
///
/// This never leaves the primitive test boundary. It binds the exact concrete
/// target, original-row frame and borrowed profile/shape template. A future
/// candidate owner would also have to bind the row ID and session generation.
struct FrontierCertificate<'a> {
    template: &'a ReachabilityTemplate,
    frame: LocalRelationRowFrame,
    target: u16,
    target_mask: u64,
    clauses: [FrontierClause; MAX_CLAUSES],
    len: usize,
}

const _: () = assert!(core::mem::size_of::<FrontierCertificate<'_>>() <= 256);

impl<'a> FrontierCertificate<'a> {
    fn derive(
        template: &'a ReachabilityTemplate,
        frame: LocalRelationRowFrame,
        target: usize,
        sample: u64,
        edge_limit: usize,
    ) -> Option<Self> {
        let target_mask = *template.state_masks.get(target)?;
        let target = u16::try_from(target).ok()?;
        if target == INVALID_STATE_INDEX
            || target_mask.count_ones() != 4
            || template.height != frame.target_height()
            || !frame.accepts_physical_board(template.width, sample | target_mask)
            || sample & target_mask != 0
        {
            return None;
        }

        let mut edges_left = edge_limit.min(MAX_EDGE_VISITS);
        let mut members = [target; MAX_NO_ENTRY_POSES];
        let mut count = 1;
        let mut cursor = 0;
        while cursor < count {
            let pose = members[cursor];
            cursor += 1;
            if template.is_sky_seed(usize::from(pose)) {
                return None;
            }
            for clause in incoming_clauses(template, pose) {
                edges_left = edges_left.checked_sub(1)?;
                let source = clause.source();
                if !members[..count].contains(&source) && !clause.excludes_edge(template, sample) {
                    *members.get_mut(count)? = source;
                    count += 1;
                }
            }
        }

        // Every route from a sky entry to the target would need to cross one
        // of these incoming edges. Use the final closure so no condition is
        // imposed for edges whose source was reached through another member.
        let mut proof = Self {
            template,
            frame,
            target,
            target_mask,
            clauses: [FrontierClause::Translation(INVALID_STATE_INDEX); MAX_CLAUSES],
            len: 0,
        };
        let members = &members[..count];
        for &pose in members {
            for clause in incoming_clauses(template, pose) {
                edges_left = edges_left.checked_sub(1)?;
                if members.contains(&clause.source())
                    || proof.clauses[..proof.len].contains(&clause)
                {
                    continue;
                }
                if !clause.excludes_edge(template, sample) {
                    return None;
                }
                *proof.clauses.get_mut(proof.len)? = clause;
                proof.len += 1;
            }
        }
        Some(proof)
    }

    fn excludes(&self, frame: LocalRelationRowFrame, target: usize, board: u64) -> bool {
        frame == self.frame
            && target == usize::from(self.target)
            && self
                .frame
                .accepts_physical_board(self.template.width, board)
            && board & self.target_mask == 0
            && self.clauses[..self.len]
                .iter()
                .all(|&clause| clause.excludes_edge(self.template, board))
    }
}

#[test]
fn frontier_certificate_reuses_incomparable_boards_but_rejects_rectangular_hull() {
    let profile = KickTableProfileId::Srs90;
    let piece = PieceKind::T;
    let rotation = RotationState::Zero;
    let template = ReachabilityTemplate::compile(10, 3, piece, profile);
    let frame = LocalRelationRowFrame::new(3, 0).unwrap();
    let target = state_index(
        10,
        template.ceiling,
        State {
            rotation,
            x: 2,
            y: 0,
        },
    )
    .unwrap();
    assert_eq!(template.state_masks[target], 0x201c);

    const A: u64 = 0x4002;
    const B: u64 = 0x5000;
    const UNBLOCKED_KICK: u64 = A | (1 << 11);
    assert_ne!(A & B, A);
    assert_ne!(A & B, B);
    for board in [A, B] {
        assert_eq!(
            bounded_no_entry_on_ordered_interval(&template, board, board, target),
            (true, 1)
        );
    }
    let proof = FrontierCertificate::derive(&template, frame, target, A, MAX_EDGE_VISITS).unwrap();
    assert!(proof.len > 0);
    assert!(FrontierCertificate::derive(&template, frame, target, A, 1).is_none());

    let mut scratch = ReachabilityScratch::default();
    let reference = |board| {
        reference_spawn_lock_anchors(10, 3, board, piece, profile).unwrap()[0] & (1 << 2) != 0
    };
    let mut fallback = |board| {
        let result = search_reachable_locks(&template, board, &mut scratch, None);
        assert!(result.exhaustive);
        result.locks.contains(10, rotation, 2, 0)
    };
    for board in [A, B] {
        assert!(proof.excludes(frame, target, board));
        assert!(!reference(board));
    }
    for board in [A & B, UNBLOCKED_KICK] {
        assert!(!proof.excludes(frame, target, board));
        assert!(reference(board));
        assert!(fallback(board));
        assert!(
            FrontierCertificate::derive(&template, frame, target, board, MAX_EDGE_VISITS).is_none()
        );
    }
    assert!(!proof.excludes(LocalRelationRowFrame::new(3, 1 << 2).unwrap(), target, A));
    assert!(!proof.excludes(frame, target + 1, A));
    assert!(!proof.excludes(frame, target, A | (1 << 30)));

    // Enumerate both collision alternatives and the nearby competing-kick
    // footprints. A failed guard is Unknown, so evaluate the ordinary exact
    // fallback there, not the inverse of the guard bit.
    let cells = [1, 5, 11, 12, 14, 15, 22, 24];
    let mut excluded = 0;
    let mut reachable = 0;
    for choices in 0..1 << cells.len() {
        let board = cells.iter().enumerate().fold(0_u64, |mask, (i, &cell)| {
            mask | (u64::from(choices & (1 << i) != 0) << cell)
        });
        let exact = reference(board);
        let result = if proof.excludes(frame, target, board) {
            excluded += 1;
            false
        } else {
            fallback(board)
        };
        reachable += usize::from(exact);
        assert_eq!(result, exact, "SRS90 board {board:#x}");
    }
    assert!(excluded > 2);
    assert!(reachable > 0);
}
