//! Test-only CEGIS experiment: keep the actual predecessor cuts instead of
//! forcing every occupied sample cell to remain occupied.

use super::*;
use crate::backend::wasm_cpu::geometry::GeometryCandidate;
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

#[test]
fn frontier_certificate_preserves_exact_same_candidate_4l_operation_graph() {
    // One actual 10x4 exact-cover candidate, with the minimum three distinct
    // operations needed to compare the target across two different prefixes.
    // I and L can independently lock first without clearing. Either prefix
    // obstructs T, but T can lock before either of them and complete the PC.
    const WIDTH: u8 = 10;
    const HEIGHT: u8 = 4;
    const START: u64 = 0xe3_f9fe_3f87;
    const OPERATIONS: [(PieceKind, u64); 3] = [
        (PieceKind::T, 0x4038),
        (PieceKind::I, 0x10_0401_0040),
        (PieceKind::L, 0x0c_0200_8000),
    ];
    let mut full = START;
    for (_, cells) in OPERATIONS {
        assert_eq!(full & cells, 0);
        full |= cells;
    }
    assert_eq!(full, (1 << (WIDTH * HEIGHT)) - 1);
    let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(
        WIDTH,
        HEIGHT,
        START,
        full ^ START,
    )
    .expect("four-row ILC catalog");
    let rows =
        OPERATIONS.map(|(piece, cells)| catalog.skeleton_id(piece, cells).expect("candidate row"));
    let candidate = GeometryCandidate::from_rows(&catalog, 0, &rows).expect("one exact candidate");
    assert_eq!(candidate.row_ids().len(), OPERATIONS.len());
    assert!(rows.iter().all(|row| candidate.row_ids().contains(row)));

    // Independently collapse the full logical rows, tracking their original
    // identities. Do not derive the sample boards from the movement proof.
    let states: [(u64, u16); 8] = core::array::from_fn(|subset| {
        let logical = OPERATIONS
            .iter()
            .enumerate()
            .filter(|(i, _)| subset & (1 << i) != 0)
            .fold(START, |board, (_, (_, cells))| board | cells);
        let row_mask = (1 << WIDTH) - 1;
        let mut board = 0;
        let mut deleted = 0;
        let mut compact_row = 0;
        for y in 0..HEIGHT {
            let row = (logical >> (WIDTH * y)) & row_mask;
            if row == row_mask {
                deleted |= 1 << y;
            } else {
                board |= row << (WIDTH * compact_row);
                compact_row += 1;
            }
        }
        (board, deleted)
    });
    const I_ONLY: usize = 0b010;
    const L_ONLY: usize = 0b100;
    assert_eq!(states[I_ONLY], (0xf3_fdff_3fc7, 0));
    assert_eq!(states[L_ONLY], (0xef_fbfe_bf87, 0));
    assert_eq!(states[I_ONLY].0 & states[L_ONLY].0, START);
    assert_eq!(states[0b111], (0, 0b1111));

    let profile = KickTableProfileId::Srs90;
    let templates =
        OPERATIONS.map(|(piece, _)| ReachabilityTemplate::compile(WIDTH, HEIGHT, piece, profile));
    let t_target = state_index(
        WIDTH,
        templates[0].ceiling,
        State {
            rotation: RotationState::Zero,
            x: 3,
            y: 0,
        },
    )
    .unwrap();
    assert_eq!(templates[0].state_masks[t_target], OPERATIONS[0].1);
    let no_clear = LocalRelationRowFrame::new(HEIGHT, 0).unwrap();
    let certificate = FrontierCertificate::derive(
        &templates[0],
        no_clear,
        t_target,
        states[I_ONLY].0,
        MAX_EDGE_VISITS,
    )
    .expect("closed on an independently reachable prefix");
    assert!(certificate.excludes(no_clear, t_target, states[L_ONLY].0));
    assert!(!certificate.excludes(no_clear, t_target, START));

    // All twelve outgoing subset/operation questions, not just the two
    // blocked edges. On a certificate miss use exact traversal; compare the
    // admitted transitions and PC routes with the independent scalar BFS.
    let mut oracle = [0_u8; 8];
    let mut accelerated = [0_u8; 8];
    let mut certified_prefixes = 0_u8;
    let mut scratch = ReachabilityScratch::default();
    for (subset, &(board, deleted)) in states[..7].iter().enumerate() {
        let frame = LocalRelationRowFrame::new(HEIGHT, deleted).unwrap();
        for (operation, (&row, &(piece, _))) in rows.iter().zip(&OPERATIONS).enumerate() {
            if subset & (1 << operation) != 0 {
                continue;
            }
            let template = &templates[operation];
            let reference =
                reference_spawn_lock_anchors(WIDTH, HEIGHT, board, piece, profile).unwrap();
            let mut exact = None;
            for lock in catalog.instantiations(row, deleted) {
                let target = state_index(
                    WIDTH,
                    template.ceiling,
                    State {
                        rotation: lock.rotation,
                        x: lock.x,
                        y: lock.lock_y,
                    },
                )
                .expect("catalog lock is a valid physical pose");
                assert_eq!(template.state_masks[target], lock.lock_mask);
                let anchor = lock.lock_y as usize * usize::from(WIDTH) + lock.x as usize;
                let scalar = reference[lock.rotation.quarter_turns() as usize] & (1 << anchor) != 0;
                if scalar {
                    oracle[subset] |= 1 << operation;
                }
                if piece == PieceKind::T && certificate.excludes(frame, target, board) {
                    assert!(!scalar, "false negative on subset {subset:b}");
                    certified_prefixes |= 1 << subset;
                    continue;
                }
                let exact = exact.get_or_insert_with(|| {
                    search_reachable_locks(template, board, &mut scratch, None)
                });
                assert!(exact.exhaustive);
                let reachable = exact
                    .locks
                    .contains(WIDTH, lock.rotation, lock.x, lock.lock_y);
                assert_eq!(
                    reachable, scalar,
                    "subset {subset:b}, operation {operation}"
                );
                if reachable {
                    accelerated[subset] |= 1 << operation;
                }
            }
        }
    }
    // I and L are reachable first, but only T→I→L and T→L→I can
    // reach the all-cleared subset. The guard never crosses the I+L frame.
    assert_eq!(oracle, [0b111, 0b110, 0b100, 0b100, 0b010, 0b010, 0, 0]);
    assert_eq!(accelerated, oracle);
    assert_eq!(certified_prefixes, (1 << I_ONLY) | (1 << L_ONLY));
}
