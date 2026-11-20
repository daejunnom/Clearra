//! Monotone movement proofs without a pose graph or visited-state storage.
//!
//! For every standard piece, a fixed-orientation path using only horizontal
//! translations and downward moves is a sufficient entry witness, independent
//! of the kick table. Its absence is Unknown: a rotation may provide another
//! path. On an occupancy interval, minimum-board grounding and a maximum-board
//! translation path certify every intermediate board without enumerating it.
//!
//! Admission proves from the actual registry and kick entries that all O
//! rotations occupy the same normalized 2x2 cells and cannot translate the
//! piece. The connected solver's sky seeds include every collision-free x and
//! rotation above the board. A path is therefore a sequence of horizontal
//! components at monotonically decreasing anchor heights. Reversing that
//! sequence gives the row-bitset recurrence below.
//!
//! This is a spawn-to-lock Boolean primitive under the existing solver's sky
//! contract. It is not a finite-lock-delay, finesse, spin or arbitrary-entry
//! relation. A caller must also preserve the original line-clear frame: cells
//! below the target cannot affect this movement, but clearing their row can
//! change the coordinate system.

use std::sync::OnceLock;

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_rules::kicks::{KickTableProfile, KickTableProfileId};

use crate::conditioned_local_relation::LocalRelationRowFrame;

use super::{
    builtin_kick_profile, grounded_index, normalized_kick_delta, piece_index,
    reverse_rotation_is_first_success, state_from_index, state_index, KickWitnessGuard,
    KickWitnessOrder, ReachabilityTemplate, State, INVALID_STATE_INDEX, INVALID_STATE_MASK,
};

/// A positive-only proof attempt using no rotations. Failure means Unknown for
/// the full movement system, including NoKick, which still permits rotations.
/// This is deliberately not an exact Boolean lock result like `MonotoneOResult`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TranslationIntervalAttempt {
    pub proven: bool,
    /// Physical anchor rows evaluated, including unsuccessful attempts.
    pub row_steps: u8,
}

/// Validated normalized cells for one immutable standard shape/orientation.
/// Only these small shape-derived operands are shared; no candidate, board,
/// line-clear frame, pose graph, or movement result is retained here.
#[derive(Clone, Copy, Debug)]
struct TranslationShape {
    cells: [(u8, u8); 4],
    width: u8,
    height: u8,
}

#[derive(Clone, Copy, Debug)]
struct TranslationPathAttempt {
    exists: bool,
    row_steps: u8,
}

impl TranslationShape {
    fn for_piece(piece: PieceKind, rotation: RotationState) -> Option<&'static Self> {
        static SHAPES: [OnceLock<Option<TranslationShape>>; 28] = [const { OnceLock::new() }; 28];
        let index = piece_index(piece) * 4 + usize::from(rotation.quarter_turns());
        SHAPES[index]
            .get_or_init(|| Self::compile(piece, rotation))
            .as_ref()
    }

    fn compile(piece: PieceKind, rotation: RotationState) -> Option<Self> {
        let definition = standard_tetromino_registry().get(piece)?;
        let mut cells = [(0, 0); 4];
        let mut occupied = 0_u16;
        let mut min_x = 4;
        let mut min_y = 4;
        let mut width = 0;
        let mut height = 0;
        for (index, cell) in definition.shape(rotation).cells().into_iter().enumerate() {
            if !(0..=3).contains(&cell.x()) || !(0..=3).contains(&cell.y()) {
                return None;
            }
            let x = cell.x() as u8;
            let y = cell.y() as u8;
            cells[index] = (x, y);
            occupied |= 1_u16 << (y * 4 + x);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            width = width.max(x + 1);
            height = height.max(y + 1);
        }
        if occupied.count_ones() != 4 || min_x != 0 || min_y != 0 {
            return None;
        }
        Some(Self {
            cells,
            width,
            height,
        })
    }

    fn lock_mask(&self, width: u8, height: u8, x: i8, y: i8) -> Option<u64> {
        if x < 0
            || y < 0
            || x as u16 + u16::from(self.width) > u16::from(width)
            || y as u16 + u16::from(self.height) > u16::from(height)
        {
            return None;
        }
        let mut mask = 0_u64;
        for &(dx, dy) in &self.cells {
            let index = (y as u32 + u32::from(dy)) * u32::from(width) + x as u32 + u32::from(dx);
            mask |= 1_u64.checked_shl(index)?;
        }
        Some(mask)
    }

    /// Exact for fixed-orientation translations only. It is private so its
    /// negative result cannot be mistaken for full rotation-aware reachability.
    /// The connected solver seeds every admissible x/orientation above the
    /// board; a caller with a narrower spawn/entry contract cannot use this.
    fn path_from_sky(
        &self,
        width: u8,
        height: u8,
        board: u64,
        x: i8,
        y: i8,
    ) -> Option<TranslationPathAttempt> {
        if !(1..=10).contains(&width)
            || !(1..=6).contains(&height)
            || self.width > width
            || x < 0
            || x as u16 + u16::from(self.width) > u16::from(width)
            || y < 0
            || y >= height as i8
            || board >> (u32::from(width) * u32::from(height)) != 0
        {
            return None;
        }
        let row_cells = (1_u16 << width) - 1;
        let anchor_mask = (1_u16 << (width - self.width + 1)) - 1;
        let mut frontier = 1_u16 << x as u32;
        let mut row_steps = 0;
        for row in y as u8..height {
            let mut free_anchors = anchor_mask;
            for &(dx, dy) in &self.cells {
                let occupied = board_row(board, width, height, row + dy, row_cells);
                free_anchors &= !(occupied >> dx);
            }
            frontier = horizontal_closure(frontier, free_anchors);
            row_steps += 1;
            if frontier == 0 {
                return Some(TranslationPathAttempt {
                    exists: false,
                    row_steps,
                });
            }
        }
        Some(TranslationPathAttempt {
            exists: true,
            row_steps,
        })
    }
}

/// Prove a lock for every occupancy `minimum <= board <= maximum` in one fixed
/// row-clear frame, for any connected kick profile with the existing solver's
/// horizontal/down translations and all-x/all-orientation sky seeds.
///
/// For a shape S and anchor row y, the free-anchor set is
/// `F_y = anchor_mask & AND_(dx,dy in S) !(row(maximum,y+dy) >> dx)`.
/// Starting at the target, horizontal component closure followed by the same
/// anchors one row above is exact for the reverse of translation-only paths:
/// every downward path visits rows in order, and each within-row segment lies
/// in one free horizontal component. Thus a nonempty frontier at the sky is a
/// real path on maximum. Removing occupied cells cannot invalidate any chosen
/// translation on that path; grounding proved on minimum persists when cells
/// are added. The two facts prove every intermediate lock.
///
/// No rotation is executed, so neither later kicks nor earlier first-success
/// competitors participate in this witness. This does not prove that their
/// absence is irrelevant to *all* possible paths. `proven == false` and `None`
/// must both remain Unknown, never a candidate-rejection result.
///
/// The caller must establish the candidate's row-contributor conditions and
/// use ordinary Boolean lock feasibility, not finesse, spin, finite-lock-delay
/// or arbitrary-entry semantics. Bounds and target must use the same physical
/// frame. This preserves LocalRelationRowFrame's current width-10 admission.
#[allow(clippy::too_many_arguments)]
pub(super) fn proves_translation_lock_interval(
    piece: PieceKind,
    width: u8,
    height: u8,
    minimum: u64,
    maximum: u64,
    rotation: RotationState,
    x: i8,
    y: i8,
    expected_lock_mask: u64,
    frame: LocalRelationRowFrame,
) -> Option<TranslationIntervalAttempt> {
    if width != 10
        || !(1..=6).contains(&height)
        || minimum & !maximum != 0
        || frame.target_height() != height
        || !frame.accepts_physical_board(width, minimum)
        || !frame.accepts_physical_board(width, maximum)
    {
        return None;
    }
    let shape = TranslationShape::for_piece(piece, rotation)?;
    let lock_mask = shape.lock_mask(width, height, x, y)?;
    if expected_lock_mask != lock_mask || !frame.accepts_physical_board(width, lock_mask) {
        return None;
    }
    let floor_cells = (1_u64 << width) - 1;
    let support_cells = (lock_mask >> width) & !lock_mask;
    if lock_mask & floor_cells == 0 && minimum & support_cells == 0 {
        return Some(TranslationIntervalAttempt {
            proven: false,
            row_steps: 0,
        });
    }
    let path = shape.path_from_sky(width, height, maximum, x, y)?;
    Some(TranslationIntervalAttempt {
        proven: path.exists,
        row_steps: path.row_steps,
    })
}

/// The raw-first and later-only modes retain separate experimental scopes.
/// The unrestricted mode can accept either, but it still requires an actual
/// first-success path. Geometrically invalid candidates have already been
/// removed by the exact template's occupancy-independent compilation.
///
/// The exact template provides the sky seeds and every movement edge. Follow
/// only straight downward translations to the source, checking occupancy on
/// maximum. The chosen destination is also clear on maximum, so that kick
/// remains first-success on every intermediate board. The final lock is
/// grounded on minimum, and the precursor need not itself be grounded. No
/// board, pose graph, or visited set is allocated or retained by this check.
///
/// For reuse, retain one actual occupied bit per earlier valid destination
/// (shared bits can serve more than one) and, when needed, one grounding bit.
/// Forbid exactly the selected descent and final-pose footprints. These
/// sufficient conditions give an interval wider than the observed boards
/// without assuming the ordered-kick system itself is monotone.
pub(super) fn last_kick_lock_interval<const RETAIN_GUARD: bool>(
    template: &ReachabilityTemplate,
    minimum: u64,
    maximum: u64,
    target: usize,
    frame_mask: u64,
    order: KickWitnessOrder,
) -> Option<KickWitnessGuard> {
    let target_mask = *template.state_masks.get(target)?;
    if target_mask == INVALID_STATE_MASK || maximum & target_mask != 0 {
        return None;
    }
    let grounding = if RETAIN_GUARD {
        let down = template.translation_targets[target][0];
        if down == INVALID_STATE_INDEX {
            0
        } else {
            let supported = minimum & template.state_masks[usize::from(down)];
            if supported == 0 {
                return None;
            }
            one_set_bit(supported)
        }
    } else {
        if !grounded_index(template, minimum, target) {
            return None;
        }
        0
    };
    let begin = template.reverse_rotation_offsets[target] as usize;
    let end = template.reverse_rotation_offsets[target + 1] as usize;
    'edge: for &edge in &template.reverse_rotation_sources[begin..end] {
        let source = usize::from(edge.source);
        let transition = source * 3 + usize::from(edge.slot);
        let first = template.rotation_target_offsets[transition] as usize;
        let chosen = first + usize::from(edge.prior_target_count);
        if usize::from(template.rotation_targets[chosen]) != target {
            continue;
        }
        let raw_index = template.rotation_kick_indices[chosen];
        match order {
            KickWitnessOrder::RawFirst if raw_index != 0 || chosen != first => continue,
            KickWitnessOrder::LaterOnly if raw_index == 0 => continue,
            KickWitnessOrder::RawFirst | KickWitnessOrder::LaterOnly | KickWitnessOrder::Any => {}
        }
        let mut required = grounding;
        if RETAIN_GUARD {
            for &earlier in &template.rotation_targets[first..chosen] {
                let blockers = minimum & template.state_masks[usize::from(earlier)];
                if blockers == 0 {
                    continue 'edge;
                }
                if required & blockers == 0 {
                    required |= one_set_bit(blockers);
                }
            }
        } else if chosen != first && !reverse_rotation_is_first_success(template, minimum, edge) {
            continue;
        }
        let Some(path) = straight_descent_clear_mask::<RETAIN_GUARD>(template, maximum, source)
        else {
            continue;
        };
        if !RETAIN_GUARD {
            return Some(KickWitnessGuard { minimum, maximum });
        }
        let allowed = frame_mask & !(path | target_mask);
        if required & !allowed != 0 || maximum & !allowed != 0 {
            continue;
        }
        debug_assert_eq!(required & !minimum, 0);
        return Some(KickWitnessGuard {
            minimum: required,
            maximum: allowed,
        });
    }
    None
}

fn one_set_bit(nonzero: u64) -> u64 {
    nonzero & nonzero.wrapping_neg()
}

#[cfg(test)]
fn has_straight_descent_from_sky(
    template: &ReachabilityTemplate,
    maximum: u64,
    source: usize,
) -> bool {
    straight_descent_clear_mask::<false>(template, maximum, source).is_some()
}

fn straight_descent_clear_mask<const RETAIN_GUARD: bool>(
    template: &ReachabilityTemplate,
    maximum: u64,
    source: usize,
) -> Option<u64> {
    let source_pose = state_from_index(template.width, template.ceiling, source);
    let seed_pose = State {
        y: source_pose.y.max(template.height as i8),
        ..source_pose
    };
    let mut pose = state_index(template.width, template.ceiling, seed_pose)?;
    if !template.is_sky_seed(pose) {
        return None;
    }
    let mut footprint = 0;
    for _ in source_pose.y..=seed_pose.y {
        let mask = template.state_masks[pose];
        if mask == INVALID_STATE_MASK || maximum & mask != 0 {
            return None;
        }
        if RETAIN_GUARD {
            footprint |= mask;
        }
        if pose == source {
            return Some(footprint);
        }
        let down = template.translation_targets[pose][0];
        if down == INVALID_STATE_INDEX {
            return None;
        }
        pose = usize::from(down);
    }
    None
}

/// Constructible only after the registry/profile monotonicity proof succeeds.
/// The proof is independent of width, height, board occupancy and target pose.
#[derive(Debug)]
pub(super) struct MonotoneOCapability {
    _private: (),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct MonotoneOResult {
    pub reachable: bool,
    /// Number of physical anchor rows evaluated, not primitive pose states.
    pub row_steps: u8,
    /// Holding this occupancy fixed preserves the Boolean result. Only this
    /// query's fixed physical board/frame and admitted profile are covered.
    pub influence_mask: u64,
}

/// Positive-only interval admission. `proven == false` is Unknown and must
/// return to another proof or the ordinary exact producer; it is not a claim
/// that any board in the interval is unreachable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct MonotoneOIntervalAttempt {
    pub proven: bool,
    /// Includes rows spent on an unsuccessful maximum-board proof.
    pub row_steps: u8,
}

impl MonotoneOCapability {
    /// Cache the structural proof once per immutable connected profile.
    /// A matching name alone is never the proof: `compile` checks the shapes
    /// and all enabled O kick offsets before returning the capability.
    pub fn for_profile(profile_id: KickTableProfileId) -> Option<&'static Self> {
        macro_rules! admitted {
            () => {{
                static CAPABILITY: OnceLock<Option<MonotoneOCapability>> = OnceLock::new();
                CAPABILITY
                    .get_or_init(|| Self::compile(builtin_kick_profile(profile_id)?))
                    .as_ref()
            }};
        }
        match profile_id {
            KickTableProfileId::Srs90 => admitted!(),
            KickTableProfileId::SrsPlus => admitted!(),
            KickTableProfileId::SrsX => admitted!(),
            KickTableProfileId::Jstris180 => admitted!(),
            KickTableProfileId::NoKick => admitted!(),
            KickTableProfileId::Asc
            | KickTableProfileId::Ars
            | KickTableProfileId::Imported
            | KickTableProfileId::Custom => None,
        }
    }

    fn compile(profile: &KickTableProfile) -> Option<Self> {
        let definition = standard_tetromino_registry().get(PieceKind::O)?;
        for rotation in RotationState::ALL {
            let mut cells = 0_u8;
            for cell in definition.shape(rotation).cells() {
                if !(0..=1).contains(&cell.x()) || !(0..=1).contains(&cell.y()) {
                    return None;
                }
                cells |= 1 << (cell.y() * 2 + cell.x());
            }
            if cells != 0b1111 {
                return None;
            }
        }
        for entry in profile.entries() {
            let transition = entry.transition();
            if transition.piece() != PieceKind::O
                || (!profile.supports_180() && transition.is_180())
            {
                continue;
            }
            // Even a later nonzero offset is conservatively unsupported.
            // This proof does not assume an earlier origin attempt dominates
            // that offset under every future representation of the profile.
            for offset in entry.sequence().offsets() {
                if normalized_kick_delta(
                    PieceKind::O,
                    transition.from(),
                    transition.to(),
                    offset.dx(),
                    offset.dy(),
                ) != (0, 0)
                {
                    return None;
                }
            }
        }
        Some(Self { _private: () })
    }

    /// Return an exact lock answer and an occupancy-invariance certificate.
    /// Invalid dimensions/poses are Unknown (`None`), not negative pruning.
    /// All four normalized rotations share this physical result by admission.
    pub fn lock_reachable(
        &self,
        width: u8,
        height: u8,
        board: u64,
        _rotation: RotationState,
        x: i8,
        y: i8,
    ) -> Option<MonotoneOResult> {
        if !(2..=10).contains(&width)
            || !(1..=6).contains(&height)
            || x < 0
            || x >= width as i8 - 1
            || y < 0
            || y >= height as i8
        {
            return None;
        }
        let board_bits = u32::from(width) * u32::from(height);
        if board_bits >= 64 || board >> board_bits != 0 {
            return None;
        }
        let row_cells = (1_u16 << width) - 1;
        let anchor_mask = (1_u16 << (width - 1)) - 1;
        let target_anchor = 1_u16 << x;
        let support = if y == 0 {
            0
        } else {
            u64::from(target_anchor | (target_anchor << 1)) << (u32::from(width) * (y as u32 - 1))
        };
        if y > 0 && board & support == 0 {
            return Some(MonotoneOResult {
                reachable: false,
                row_steps: 0,
                influence_mask: support,
            });
        }

        let mut influence_mask = support;
        let mut frontier = target_anchor;
        let mut row_steps = 0;
        for row in y as u8..height {
            let occupied = board_row(board, width, height, row, row_cells)
                | board_row(board, width, height, row + 1, row_cells);
            let free_anchors = !(occupied | (occupied >> 1)) & anchor_mask;
            let incoming = frontier;
            frontier = horizontal_closure(incoming, free_anchors);
            row_steps += 1;

            // The closure is the union of free intervals touched by incoming
            // anchors. Preserve every such interval, its adjacent blocked
            // anchors, and every blocked incoming anchor. Changes outside
            // these cells cannot change the selected horizontal components.
            let inspected = (incoming | frontier | (frontier << 1) | (frontier >> 1)) & anchor_mask;
            let inspected_cells = (inspected | (inspected << 1)) & row_cells;
            influence_mask |= cells_in_row(inspected_cells, width, height, row);
            influence_mask |= cells_in_row(inspected_cells, width, height, row + 1);

            if frontier == 0 {
                return Some(MonotoneOResult {
                    reachable: false,
                    row_steps,
                    influence_mask,
                });
            }
            // Reversing a downward step keeps the anchor x. At the next row
            // only its free-anchor test remains before horizontal closure.
        }
        // Above the last physical row all admissible x/rotations are sky
        // seeds; a nonempty frontier therefore reaches a real source.
        Some(MonotoneOResult {
            reachable: true,
            row_steps,
            influence_mask,
        })
    }

    /// Certify every physical board `minimum <= board <= maximum` without a
    /// cached observation or intermediate-board enumeration.
    ///
    /// With O's admitted zero-translation rotations, removing obstacles cannot
    /// destroy a movement path. A reachable lock on the maximum board supplies
    /// a path on every smaller board, while grounding on the minimum board
    /// supplies support on every larger board. These are different obligations:
    /// maximum-board grounding alone would be unsound.
    ///
    /// The caller supplies one ILC realization's exact four-cell lock mask and
    /// must separately establish the original-row contributor conditions. This
    /// method checks physical frame capacity and pose/mask equality, not those
    /// candidate-specific contributor conditions. The current shared
    /// `LocalRelationRowFrame` contract supports ten columns only; unlike the
    /// single-board primitive, this interval API preserves that restriction.
    #[allow(clippy::too_many_arguments)]
    pub fn proves_lock_interval(
        &self,
        width: u8,
        height: u8,
        minimum: u64,
        maximum: u64,
        rotation: RotationState,
        x: i8,
        y: i8,
        expected_lock_mask: u64,
        frame: LocalRelationRowFrame,
    ) -> Option<MonotoneOIntervalAttempt> {
        if width != 10
            || !(1..=6).contains(&height)
            || x < 0
            || x >= width as i8 - 1
            || y < 0
            || y >= height as i8 - 1
            || minimum & !maximum != 0
            || frame.target_height() != height
            || !frame.accepts_physical_board(width, minimum)
            || !frame.accepts_physical_board(width, maximum)
        {
            return None;
        }
        let bottom = 3_u64 << (x as u32 + y as u32 * u32::from(width));
        let lock_mask = bottom | (bottom << width);
        if expected_lock_mask != lock_mask || !frame.accepts_physical_board(width, lock_mask) {
            return None;
        }
        if y > 0 && minimum & (bottom >> width) == 0 {
            return Some(MonotoneOIntervalAttempt {
                proven: false,
                row_steps: 0,
            });
        }
        let maximum_result = self.lock_reachable(width, height, maximum, rotation, x, y)?;
        Some(MonotoneOIntervalAttempt {
            proven: maximum_result.reachable,
            row_steps: maximum_result.row_steps,
        })
    }
}

#[inline]
fn board_row(board: u64, width: u8, height: u8, row: u8, row_cells: u16) -> u16 {
    if row >= height {
        0
    } else {
        ((board >> (u32::from(width) * u32::from(row))) as u16) & row_cells
    }
}

#[inline]
fn cells_in_row(cells: u16, width: u8, height: u8, row: u8) -> u64 {
    if row >= height {
        0
    } else {
        u64::from(cells) << (u32::from(width) * u32::from(row))
    }
}

/// Bit-parallel horizontal component closure in four fixed doubling steps.
/// `left`/`right` certify every intervening free anchor for a jump, so this is
/// algebraic propagation through intervals, not a jump over blocked anchors.
#[inline]
fn horizontal_closure(seeds: u16, free: u16) -> u16 {
    let mut reached = seeds & free;
    let mut left = free;
    let mut right = free;
    for shift in [1, 2, 4, 8] {
        reached |= ((reached << shift) & left) | ((reached >> shift) & right);
        left &= left << shift;
        right &= right >> shift;
    }
    reached
}

#[cfg(test)]
mod tests {
    use std::collections::{HashSet, VecDeque};

    use super::*;
    use clearra_rules::{
        kicks::{KickOffset, KickOffsetSequence, KickTableEntry, KickTransition},
        profile::rule_profile::RuleProfileId,
    };

    const PROFILES: [KickTableProfileId; 5] = [
        KickTableProfileId::Srs90,
        KickTableProfileId::SrsPlus,
        KickTableProfileId::SrsX,
        KickTableProfileId::Jstris180,
        KickTableProfileId::NoKick,
    ];

    #[test]
    fn ordered_kick_interval_requires_both_prior_blockers_on_minimum() {
        use crate::reachability_reference::reference_spawn_lock_anchors;

        const TARGET: u64 = 0x201c;
        const LEFT_PRIOR_BLOCKER: u64 = 1 << 11;
        const RIGHT_PRIOR_BLOCKER: u64 = 1 << 14;
        const MINIMUM: u64 = 0x4802;
        const MAXIMUM: u64 = MINIMUM | (1 << 29);
        let profile = KickTableProfileId::Srs90;
        let catalog = super::super::GeometryCatalog::compile_for_required_cells_on_dimensions(
            10, 3, MINIMUM, TARGET,
        )
        .unwrap();
        let row = catalog.skeleton_id(PieceKind::T, TARGET).unwrap();
        let lock = catalog
            .instantiations(row, 0)
            .find(|lock| lock.rotation == RotationState::Zero && lock.x == 2 && lock.lock_y == 0)
            .unwrap();
        assert_eq!(lock.lock_mask, TARGET);
        let frame = LocalRelationRowFrame::new(3, 0);
        let mut workspace = super::super::ReachabilityWorkspace::default();
        workspace.configure_kick_profile(profile, false);
        assert!(!workspace.try_prove_first_kick_lock_interval(
            &catalog,
            MINIMUM,
            MAXIMUM,
            PieceKind::T,
            profile,
            lock,
            frame,
        ));
        assert!(workspace.try_prove_ordered_kick_lock_interval(
            &catalog,
            MINIMUM,
            MAXIMUM,
            PieceKind::T,
            profile,
            lock,
            frame,
        ));
        for board in [MINIMUM, MAXIMUM] {
            let exact = reference_spawn_lock_anchors(10, 3, board, PieceKind::T, profile).unwrap();
            assert_ne!(exact[0] & (1 << 2), 0, "board {board:#x}");
        }

        // The two preceding kicks are permanently blocked only if both
        // roof cells belong to minimum. A blocker present only on maximum
        // cannot be used to prove the third kick throughout the interval.
        let without_left = MINIMUM & !LEFT_PRIOR_BLOCKER;
        let exact =
            reference_spawn_lock_anchors(10, 3, without_left, PieceKind::T, profile).unwrap();
        assert_eq!(exact[0] & (1 << 2), 0);
        for blocker in [LEFT_PRIOR_BLOCKER, RIGHT_PRIOR_BLOCKER] {
            assert!(!workspace.try_prove_ordered_kick_lock_interval(
                &catalog,
                MINIMUM & !blocker,
                MAXIMUM,
                PieceKind::T,
                profile,
                lock,
                frame,
            ));
        }

        let template = workspace.template(&catalog, PieceKind::T);
        let source = state_index(
            10,
            template.ceiling,
            State {
                rotation: RotationState::Right,
                x: 2,
                y: 0,
            },
        )
        .unwrap();
        let target = state_index(
            10,
            template.ceiling,
            State {
                rotation: lock.rotation,
                x: lock.x,
                y: lock.lock_y,
            },
        )
        .unwrap() as u16;
        let start = template.rotation_target_offsets[source * 3 + 1] as usize;
        assert!(has_straight_descent_from_sky(template, MAXIMUM, source));
        assert_eq!(template.rotation_kick_indices[start..start + 3], [0, 1, 2]);
        assert_eq!(template.rotation_targets[start + 2], target);
        assert_eq!(
            super::super::first_successful_kick(template, MINIMUM, source, 1),
            Some(target)
        );
        for blocker in [LEFT_PRIOR_BLOCKER, RIGHT_PRIOR_BLOCKER] {
            assert_ne!(
                super::super::first_successful_kick(template, MINIMUM & !blocker, source, 1),
                Some(target)
            );
        }
    }

    #[test]
    fn ordered_kick_witness_guard_reuses_incomparable_boards_without_weakening_first_success() {
        use crate::reachability_reference::reference_spawn_lock_anchors;

        const TARGET: u64 = 0x201c;
        const LEFT_PRIOR: u64 = 1 << 11;
        const RIGHT_PRIOR: u64 = 1 << 14;
        const BLOCKERS: u64 = LEFT_PRIOR | RIGHT_PRIOR;
        const BASE: u64 = 0x4802;
        const OBSERVED: u64 = BASE | (1 << 28);
        const SIBLING: u64 = BASE | (1 << 29);
        const PATH_CELL: u64 = 1 << 22;
        let profile = KickTableProfileId::Srs90;
        let catalog = super::super::GeometryCatalog::compile_for_required_cells_on_dimensions(
            10, 3, BASE, TARGET,
        )
        .unwrap();
        let lock = super::super::InstantiatedRealization {
            lock_mask: TARGET,
            rotation: RotationState::Zero,
            x: 2,
            lock_y: 0,
        };
        let frame = LocalRelationRowFrame::new(3, 0);
        let mut workspace = super::super::ReachabilityWorkspace::default();
        workspace.configure_kick_profile(profile, false);
        let mut witness = |minimum, maximum, order| {
            workspace.try_kick_witness_guard(
                &catalog,
                minimum,
                maximum,
                PieceKind::T,
                profile,
                lock,
                frame,
                order,
            )
        };
        let guard = witness(OBSERVED, OBSERVED, KickWitnessOrder::LaterOnly).unwrap();
        assert_eq!(guard.minimum, BLOCKERS);
        assert_eq!(
            Some(guard),
            witness(OBSERVED, OBSERVED, KickWitnessOrder::Any),
        );
        assert_eq!(
            None,
            witness(OBSERVED, OBSERVED, KickWitnessOrder::RawFirst),
        );
        let guard_accepts = |board| guard.minimum & !board == 0 && board & !guard.maximum == 0;
        assert_ne!(OBSERVED & !SIBLING, 0);
        assert_ne!(SIBLING & !OBSERVED, 0);
        for board in [OBSERVED, SIBLING, BLOCKERS, BASE] {
            assert!(guard_accepts(board), "board {board:#x}");
            let exact = reference_spawn_lock_anchors(10, 3, board, PieceKind::T, profile).unwrap();
            assert_ne!(exact[0] & (1 << 2), 0, "board {board:#x}");
        }
        for blocker in [LEFT_PRIOR, RIGHT_PRIOR] {
            let without = SIBLING & !blocker;
            assert!(!guard_accepts(without));
            assert_eq!(None, witness(without, SIBLING, KickWitnessOrder::Any));
        }
        // A successful endpoint is existential evidence only. The lower
        // endpoint lacks the necessary first blocker despite upper success.
        let missing_left = SIBLING & !LEFT_PRIOR;
        let exact =
            reference_spawn_lock_anchors(10, 3, missing_left, PieceKind::T, profile).unwrap();
        assert_eq!(exact[0] & (1 << 2), 0);
        assert_eq!(SIBLING & PATH_CELL, 0);
        assert_eq!(TARGET & PATH_CELL, 0);
        assert!(!guard_accepts(SIBLING | PATH_CELL));
        assert!(!guard_accepts(SIBLING | (1 << 30)), "outside frame");
    }

    #[test]
    fn ordered_kick_witness_guard_retains_naturally_nonfloor_support() {
        use crate::reachability_reference::reference_spawn_lock_anchors;

        // Translate the same ordered-kick cavity up one row and leave the
        // concrete target supported by a single unrelated cell beneath it.
        const SUPPORT: u64 = 1 << 2;
        const BASE: u64 = (0x4802_u64 << 10) | SUPPORT;
        const TARGET: u64 = 0x201c_u64 << 10;
        let profile = KickTableProfileId::Srs90;
        let catalog = super::super::GeometryCatalog::compile_for_required_cells_on_dimensions(
            10, 4, BASE, TARGET,
        )
        .unwrap();
        let lock = super::super::InstantiatedRealization {
            lock_mask: TARGET,
            rotation: RotationState::Zero,
            x: 2,
            lock_y: 1,
        };
        let frame = LocalRelationRowFrame::new(4, 0);
        let mut workspace = super::super::ReachabilityWorkspace::default();
        workspace.configure_kick_profile(profile, false);
        let guard = workspace
            .try_kick_witness_guard(
                &catalog,
                BASE,
                BASE,
                PieceKind::T,
                profile,
                lock,
                frame,
                KickWitnessOrder::LaterOnly,
            )
            .unwrap();
        assert_eq!(guard.minimum, ((1 << 11 | 1 << 14) << 10) | SUPPORT);
        assert_eq!(BASE & !guard.maximum, 0);
        let exact = reference_spawn_lock_anchors(10, 4, BASE, PieceKind::T, profile).unwrap();
        assert_ne!(exact[0] & (1 << 12), 0);
        let unsupported = BASE & !SUPPORT;
        assert_ne!(guard.minimum & !unsupported, 0);
        assert_eq!(
            None,
            workspace.try_kick_witness_guard(
                &catalog,
                unsupported,
                BASE,
                PieceKind::T,
                profile,
                lock,
                frame,
                KickWitnessOrder::Any,
            ),
        );
        let exact =
            reference_spawn_lock_anchors(10, 4, unsupported, PieceKind::T, profile).unwrap();
        assert_eq!(exact[0] & (1 << 12), 0);
    }

    #[test]
    fn first_kick_interval_preserves_the_discovered_t_lock_on_both_boards() {
        use crate::reachability_reference::reference_spawn_lock_anchors;

        const INITIAL: u64 = 0x3f0f_83f4;
        const I: u64 = 0x00f0_0000;
        const O: u64 = 0x0c03;
        const T: u64 = 0x7008;
        let catalog = super::super::GeometryCatalog::compile_for_required_cells_on_dimensions(
            10,
            3,
            INITIAL,
            I | O | T,
        )
        .unwrap();
        let row = catalog.skeleton_id(PieceKind::T, T).unwrap();
        let lock = catalog
            .instantiations(row, 0)
            .find(|lock| lock.rotation == RotationState::Two && lock.x == 2 && lock.lock_y == 0)
            .unwrap();
        assert_eq!(lock.lock_mask, T);
        let frame = LocalRelationRowFrame::new(3, 0);
        for profile in PROFILES {
            let mut workspace = super::super::ReachabilityWorkspace::default();
            workspace.configure_kick_profile(profile, false);
            let translation = proves_translation_lock_interval(
                PieceKind::T,
                10,
                3,
                INITIAL,
                INITIAL | O,
                lock.rotation,
                lock.x,
                lock.lock_y,
                T,
                frame.unwrap(),
            )
            .unwrap();
            assert!(!translation.proven);
            assert!(workspace.try_prove_first_kick_lock_interval(
                &catalog,
                INITIAL,
                INITIAL | O,
                PieceKind::T,
                profile,
                lock,
                frame,
            ));
            let guard = workspace
                .try_kick_witness_guard(
                    &catalog,
                    INITIAL,
                    INITIAL | O,
                    PieceKind::T,
                    profile,
                    lock,
                    frame,
                    KickWitnessOrder::RawFirst,
                )
                .unwrap();
            assert_eq!(guard.minimum, 0, "raw first kick and floor support");
            assert_eq!((INITIAL | O) & !guard.maximum, 0);
            for board in [INITIAL, INITIAL | O] {
                let exact = reference_spawn_lock_anchors(10, 3, board, PieceKind::T, profile)
                    .expect("supported independent oracle");
                assert_ne!(
                    exact[RotationState::Two.quarter_turns() as usize] & (1 << 2),
                    0
                );
            }

            // The target stays free, but an extra cell above it blocks the
            // same-column descent. Do not reuse the old positive result.
            let path_blocker = 1_u64 << 23;
            assert_eq!((INITIAL | O | T) & path_blocker, 0);
            assert!(!workspace.try_prove_first_kick_lock_interval(
                &catalog,
                INITIAL,
                INITIAL | O | path_blocker,
                PieceKind::T,
                profile,
                lock,
                frame,
            ));
            assert!(!workspace.try_prove_first_kick_lock_interval(
                &catalog,
                INITIAL | O,
                INITIAL,
                PieceKind::T,
                profile,
                lock,
                frame,
            ));
            assert!(!workspace.try_prove_first_kick_lock_interval(
                &catalog,
                INITIAL,
                INITIAL | O,
                PieceKind::T,
                profile,
                lock,
                LocalRelationRowFrame::new(3, 1),
            ));
            assert_eq!(workspace.generated_state_count(), 0);
        }
    }

    #[test]
    fn capability_checks_actual_profile_offsets_not_the_profile_name() {
        for profile in PROFILES {
            assert!(MonotoneOCapability::for_profile(profile).is_some());
        }
        for profile in [
            KickTableProfileId::Asc,
            KickTableProfileId::Ars,
            KickTableProfileId::Imported,
            KickTableProfileId::Custom,
        ] {
            assert!(MonotoneOCapability::for_profile(profile).is_none());
        }
        for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let counterfeit = KickTableProfile::new(
                KickTableProfileId::Srs90,
                RuleProfileId::Srs,
                vec![KickTableEntry::new(
                    KickTransition::new(PieceKind::O, RotationState::Zero, RotationState::Right),
                    KickOffsetSequence::new(vec![KickOffset::new(dx, dy)]),
                )],
            );
            assert!(MonotoneOCapability::compile(&counterfeit).is_none());
        }
    }

    #[test]
    fn horizontal_interval_closure_matches_every_ten_bit_graph() {
        // A vertical I has ten anchors on a width-ten board, including x=9.
        for free in 0..1_u16 << 10 {
            for seeds in 0..1_u16 << 10 {
                let mut expected = seeds & free;
                loop {
                    let next = (expected | (expected << 1) | (expected >> 1)) & free;
                    if next == expected {
                        break;
                    }
                    expected = next;
                }
                assert_eq!(horizontal_closure(seeds, free), expected);
            }
        }
    }

    /// A deliberately separate cell-based BFS. It does not use compiled shape
    /// operands, free-anchor masks, horizontal closure or production traversal.
    fn reference_translation_anchors(
        width: u8,
        height: u8,
        board: u64,
        piece: PieceKind,
        rotation: RotationState,
    ) -> u64 {
        let cells = standard_tetromino_registry()
            .get(piece)
            .unwrap()
            .shape(rotation)
            .cells();
        let placeable = |x: i8, y: i8| {
            cells.iter().all(|cell| {
                let cell_x = i16::from(x) + i16::from(cell.x());
                let cell_y = i16::from(y) + i16::from(cell.y());
                cell_x >= 0
                    && cell_x < i16::from(width)
                    && cell_y >= 0
                    && (cell_y >= i16::from(height)
                        || board & (1_u64 << (cell_y as u32 * u32::from(width) + cell_x as u32))
                            == 0)
            })
        };
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        for x in 0..width as i8 {
            if placeable(x, height as i8) {
                visited.insert((x, height as i8));
                queue.push_back((x, height as i8));
            }
        }
        let mut anchors = 0;
        while let Some((x, y)) = queue.pop_front() {
            if y < height as i8 {
                anchors |= 1_u64 << (y as u32 * u32::from(width) + x as u32);
            }
            for next in [(x - 1, y), (x + 1, y), (x, y - 1)] {
                if next.0 >= 0
                    && next.0 < width as i8
                    && next.1 >= 0
                    && placeable(next.0, next.1)
                    && visited.insert(next)
                {
                    queue.push_back(next);
                }
            }
        }
        anchors
    }

    fn reference_lock_mask(
        width: u8,
        piece: PieceKind,
        rotation: RotationState,
        x: u8,
        y: u8,
    ) -> u64 {
        standard_tetromino_registry()
            .get(piece)
            .unwrap()
            .shape(rotation)
            .cells()
            .into_iter()
            .fold(0, |mask, cell| {
                mask | (1_u64
                    << ((u32::from(y) + cell.y() as u32) * u32::from(width)
                        + u32::from(x)
                        + cell.x() as u32))
            })
    }

    #[test]
    fn every_standard_shape_matches_independent_translation_bfs_on_small_boards() {
        for piece in PieceKind::STANDARD_TETROMINOES {
            for rotation in RotationState::ALL {
                let shape = TranslationShape::for_piece(piece, rotation).unwrap();
                for (width, height) in [(1_u8, 4_u8), (3, 3), (4, 3)] {
                    if shape.width > width {
                        continue;
                    }
                    for board in 0..1_u64 << (width * height) {
                        let expected =
                            reference_translation_anchors(width, height, board, piece, rotation);
                        for y in 0..height {
                            for x in 0..=width - shape.width {
                                let actual = shape
                                    .path_from_sky(width, height, board, x as i8, y as i8)
                                    .unwrap();
                                let anchor = 1_u64 << (y * width + x);
                                assert_eq!(
                                    actual.exists,
                                    expected & anchor != 0,
                                    "piece={piece:?}, rotation={rotation:?}, {width}x{height}, board={board:#x}, target=({x},{y})"
                                );
                                assert!((1..=height - y).contains(&actual.row_steps));
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn every_shape_interval_covers_all_intermediate_boards_and_clear_frames() {
        let (width, height) = (10, 6);
        for piece in PieceKind::STANDARD_TETROMINOES {
            for rotation in RotationState::ALL {
                let cells = standard_tetromino_registry()
                    .get(piece)
                    .unwrap()
                    .shape(rotation)
                    .cells();
                let shape_width = cells.iter().map(|cell| cell.x()).max().unwrap() as u8 + 1;
                let shape_height = cells.iter().map(|cell| cell.y()).max().unwrap() as u8 + 1;
                let support_x = cells.iter().find(|cell| cell.y() == 0).unwrap().x() as u8;
                for (y, deleted_rows, variable_row) in [(1_u8, 0_u16, 4_u8), (0, 1, 3)] {
                    let frame = LocalRelationRowFrame::new(height, deleted_rows).unwrap();
                    let support = if y == 0 { 0 } else { 1_u64 << support_x };
                    // Every vertical drop at the target x crosses this roof.
                    // Entry beside it followed by a horizontal slide succeeds.
                    let roof = ((1_u64 << shape_width) - 1) << ((y + shape_height) * width);
                    let minimum = support | roof;
                    let variable = (3_u64 << (variable_row * width + 8))
                        | (3_u64 << ((variable_row + 1) * width + 8));
                    let maximum = minimum | variable;
                    let lock_mask = reference_lock_mask(width, piece, rotation, 0, y);
                    let attempt = proves_translation_lock_interval(
                        piece, width, height, minimum, maximum, rotation, 0, y as i8, lock_mask,
                        frame,
                    )
                    .unwrap();
                    assert!(
                        attempt.proven,
                        "piece={piece:?}, rotation={rotation:?}, deleted_rows={deleted_rows:#b}"
                    );
                    assert!(attempt.row_steps > 0);
                    let mut assignment = variable;
                    let mut count = 0;
                    loop {
                        let board = minimum | assignment;
                        assert!(frame.accepts_physical_board(width, board));
                        for profile in PROFILES {
                            let expected =
                                crate::reachability_reference::reference_spawn_lock_anchors(
                                    width, height, board, piece, profile,
                                )
                                .unwrap();
                            assert_ne!(
                                expected[rotation.quarter_turns() as usize]
                                    & (1_u64 << (y * width)),
                                0,
                                "piece={piece:?}, rotation={rotation:?}, profile={profile:?}, board={board:#x}, deleted_rows={deleted_rows:#b}"
                            );
                        }
                        count += 1;
                        if assignment == 0 {
                            break;
                        }
                        assignment = (assignment - 1) & variable;
                    }
                    assert_eq!(count, 16);
                }
            }
        }
    }

    #[test]
    fn every_shape_requires_minimum_grounding_and_vertical_i_admits_x_nine() {
        let frame = LocalRelationRowFrame::new(6, 0).unwrap();
        for piece in PieceKind::STANDARD_TETROMINOES {
            for rotation in RotationState::ALL {
                let cells = standard_tetromino_registry()
                    .get(piece)
                    .unwrap()
                    .shape(rotation)
                    .cells();
                let support_x = cells.iter().find(|cell| cell.y() == 0).unwrap().x() as u8;
                let lock_mask = reference_lock_mask(10, piece, rotation, 0, 1);
                for maximum in [0, 1_u64 << support_x] {
                    let attempt = proves_translation_lock_interval(
                        piece, 10, 6, 0, maximum, rotation, 0, 1, lock_mask, frame,
                    )
                    .unwrap();
                    assert!(!attempt.proven);
                    assert_eq!(attempt.row_steps, 0);
                }
            }
        }
        for rotation in [RotationState::Right, RotationState::Left] {
            let expected_lock_mask = reference_lock_mask(10, PieceKind::I, rotation, 9, 0);
            assert!(
                proves_translation_lock_interval(
                    PieceKind::I,
                    10,
                    6,
                    0,
                    0,
                    rotation,
                    9,
                    0,
                    expected_lock_mask,
                    frame,
                )
                .unwrap()
                .proven
            );
        }
    }

    #[test]
    fn distant_translation_entry_and_joint_blockers_remain_relevant() {
        let frame = LocalRelationRowFrame::new(6, 0).unwrap();
        let rotation = RotationState::Zero;
        let attempt = |minimum, maximum, x| {
            proves_translation_lock_interval(
                PieceKind::T,
                10,
                6,
                minimum,
                maximum,
                rotation,
                x,
                0,
                reference_lock_mask(10, PieceKind::T, rotation, x as u8, 0),
                frame,
            )
            .unwrap()
        };
        let roof = 0x7f_u64 << 20;
        let distant_i = (2..6).fold(0_u64, |mask, y| mask | (1_u64 << (10 * y + 7)));
        assert!(attempt(roof, roof, 0).proven);
        assert!(!attempt(roof, roof | distant_i, 0).proven);
        let local_cells = (0..6).fold(0_u64, |mask, y| mask | (0xf_u64 << (10 * y)));
        assert_eq!(roof & local_cells, (roof | distant_i) & local_cells);

        // Each entrance survives either blocker alone. Both must be retained
        // in any independence argument; an individual-pair test is insufficient.
        let middle_roof = 7_u64 << 23;
        let left_blocker = 1_u64 << 21;
        let right_blocker = 1_u64 << 27;
        for extra in [0, left_blocker, right_blocker] {
            assert!(attempt(middle_roof, middle_roof | extra, 3).proven);
        }
        assert!(!attempt(middle_roof, middle_roof | left_blocker | right_blocker, 3).proven);
    }

    #[test]
    fn translation_miss_is_unknown_even_when_rotation_proves_the_lock() {
        // Only a two-cell-wide entrance remains. Horizontal T cannot cross
        // the roof, but Right T enters at x=8 and rotates below it, then slides
        // to the target. This also works with zero kick offsets: rotations do
        // not disappear merely because a profile is named NoKick.
        let board = 0xff_u64 << 20;
        let rotation = RotationState::Zero;
        let attempt = proves_translation_lock_interval(
            PieceKind::T,
            10,
            6,
            board,
            board,
            rotation,
            0,
            0,
            reference_lock_mask(10, PieceKind::T, rotation, 0, 0),
            LocalRelationRowFrame::new(6, 0).unwrap(),
        )
        .unwrap();
        assert!(!attempt.proven);
        assert!(attempt.row_steps > 0);
        for profile in PROFILES {
            let expected = crate::reachability_reference::reference_spawn_lock_anchors(
                10,
                6,
                board,
                PieceKind::T,
                profile,
            )
            .unwrap();
            assert_ne!(expected[rotation.quarter_turns() as usize] & 1, 0);
        }
    }

    #[test]
    fn translation_interval_rejects_wrong_frame_bounds_ilc_and_partial_target() {
        let piece = PieceKind::L;
        let rotation = RotationState::Right;
        let lock_mask = reference_lock_mask(10, piece, rotation, 0, 0);
        let frame = LocalRelationRowFrame::new(6, 0).unwrap();
        let attempt = |minimum, maximum, expected, candidate_frame| {
            proves_translation_lock_interval(
                piece,
                10,
                6,
                minimum,
                maximum,
                rotation,
                0,
                0,
                expected,
                candidate_frame,
            )
        };
        assert!(attempt(0, 0, lock_mask, frame).unwrap().proven);
        assert!(attempt(1, 0, lock_mask, frame).is_none());
        assert!(attempt(0, 0, lock_mask ^ 1, frame).is_none());
        assert!(attempt(0, 0, lock_mask, LocalRelationRowFrame::new(5, 0).unwrap()).is_none());
        assert!(attempt(
            0,
            1_u64 << 50,
            lock_mask,
            LocalRelationRowFrame::new(6, 1).unwrap(),
        )
        .is_none());
        assert!(attempt(0, 1_u64 << 60, lock_mask, frame).is_none());
        assert!(attempt(0, lock_mask, lock_mask, frame).is_some_and(|attempt| !attempt.proven));
        assert!(proves_translation_lock_interval(
            PieceKind::I,
            10,
            6,
            0,
            0,
            RotationState::Right,
            0,
            0,
            reference_lock_mask(10, PieceKind::I, RotationState::Right, 0, 0),
            LocalRelationRowFrame::new(6, 0b111).unwrap(),
        )
        .is_none());
        for (width, height, x, y) in [
            (6, 6, 0, 0),
            (10, 0, 0, 0),
            (10, 7, 0, 0),
            (10, 6, -1, 0),
            (10, 6, 9, 0),
            (10, 6, 0, -1),
            (10, 6, 0, 5),
        ] {
            assert!(proves_translation_lock_interval(
                piece, width, height, 0, 0, rotation, x, y, lock_mask, frame,
            )
            .is_none());
        }
    }

    #[test]
    fn all_small_boards_match_independent_profile_bfs_in_every_rotation() {
        for profile in PROFILES {
            let capability = MonotoneOCapability::for_profile(profile).unwrap();
            for (width, height) in [(2, 3), (3, 4), (4, 3)] {
                for board in 0..1_u64 << (width * height) {
                    let expected = crate::reachability_reference::reference_spawn_lock_anchors(
                        width,
                        height,
                        board,
                        PieceKind::O,
                        profile,
                    )
                    .unwrap();
                    for rotation in RotationState::ALL {
                        for y in 0..height {
                            for x in 0..width - 1 {
                                let result = capability
                                    .lock_reachable(
                                        width, height, board, rotation, x as i8, y as i8,
                                    )
                                    .unwrap();
                                let anchor = 1_u64 << (y * width + x);
                                assert_eq!(
                                    result.reachable,
                                    expected[rotation.quarter_turns() as usize] & anchor != 0,
                                    "profile={profile:?}, {width}x{height}, board={board:#x}, ({x},{y}) {rotation:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn footprint_excludes_only_cells_proven_irrelevant_for_all_changes() {
        let profile = KickTableProfileId::Jstris180;
        let capability = MonotoneOCapability::for_profile(profile).unwrap();
        let (width, height) = (3, 3);
        let board_mask = (1_u64 << (width * height)) - 1;
        let expected = (0..=board_mask)
            .map(|board| {
                crate::reachability_reference::reference_spawn_lock_anchors(
                    width,
                    height,
                    board,
                    PieceKind::O,
                    profile,
                )
                .unwrap()[0]
            })
            .collect::<Vec<_>>();
        for board in 0..=board_mask {
            for y in 0..height {
                for x in 0..width - 1 {
                    let result = capability
                        .lock_reachable(width, height, board, RotationState::Zero, x as i8, y as i8)
                        .unwrap();
                    assert_eq!(result.influence_mask & !board_mask, 0);
                    let variable = board_mask & !result.influence_mask;
                    let fixed = board & result.influence_mask;
                    let mut assignment = variable;
                    loop {
                        let other = fixed | assignment;
                        let anchor = 1_u64 << (y * width + x);
                        assert_eq!(
                            result.reachable,
                            expected[other as usize] & anchor != 0,
                            "board={board:#x}, other={other:#x}, mask={:#x}, target=({x},{y})",
                            result.influence_mask
                        );
                        if assignment == 0 {
                            break;
                        }
                        assignment = (assignment - 1) & variable;
                    }
                }
            }
        }
    }

    #[test]
    fn distant_roof_entry_is_relevant_but_lower_disconnected_cells_are_not() {
        let profile = KickTableProfileId::Jstris180;
        let capability = MonotoneOCapability::for_profile(profile).unwrap();
        let roof = 0xff_u64 << 20;
        let remote_i = (2..6).fold(0_u64, |mask, y| mask | (1_u64 << (y * 10 + 8)));
        let open = capability
            .lock_reachable(10, 6, roof, RotationState::Zero, 0, 0)
            .unwrap();
        let closed = capability
            .lock_reachable(10, 6, roof | remote_i, RotationState::Zero, 0, 0)
            .unwrap();
        assert!(open.reachable);
        assert!(!closed.reachable);
        assert_ne!(open.influence_mask & remote_i, 0);
        // The board inside x=0..3 is unchanged. A fixed two-cell neighborhood
        // of the target cannot see the I that closes the distant entry.
        let local = (0..6).fold(0_u64, |mask, y| mask | (0xf_u64 << (y * 10)));
        assert_eq!(roof & local, (roof | remote_i) & local);
        for (board, actual) in [(roof, open), (roof | remote_i, closed)] {
            let expected = crate::reachability_reference::reference_spawn_lock_anchors(
                10,
                6,
                board,
                PieceKind::O,
                profile,
            )
            .unwrap();
            assert_eq!(actual.reachable, expected[0] & 1 != 0);
        }

        let target = capability
            .lock_reachable(10, 6, 1_u64 << 20, RotationState::Zero, 0, 3)
            .unwrap();
        assert!(target.reachable);
        assert_eq!(target.influence_mask & ((1_u64 << 20) - 1), 0);
        assert_eq!(target.influence_mask & (0xff_u64 << 22), 0);
    }

    #[test]
    fn invalid_dimensions_and_poses_are_unknown() {
        let capability = MonotoneOCapability::for_profile(KickTableProfileId::Srs90).unwrap();
        for (width, height, board, x, y) in [
            (1, 4, 0, 0, 0),
            (11, 4, 0, 0, 0),
            (10, 0, 0, 0, 0),
            (10, 7, 0, 0, 0),
            (10, 4, 1_u64 << 40, 0, 0),
            (10, 4, 0, -1, 0),
            (10, 4, 0, 9, 0),
            (10, 4, 0, 0, -1),
            (10, 4, 0, 0, 4),
        ] {
            assert!(capability
                .lock_reachable(width, height, board, RotationState::Zero, x, y)
                .is_none());
        }
    }

    #[test]
    fn interval_without_cache_covers_all_sixteen_intermediate_boards() {
        // A non-floor target with guaranteed minimum support, and a second
        // fixture with a nonzero original-row deletion frame. Each has four
        // independently optional cells, all covered by one maximum-board proof.
        let cases = [
            (
                10_u8,
                4_u8,
                1_u64 | (3_u64 << 30),
                (1_u64 << 2) | (1_u64 << 3) | (1_u64 << 32) | (1_u64 << 33),
                1_i8,
                LocalRelationRowFrame::new(4, 0).unwrap(),
            ),
            (
                10,
                4,
                3_u64 << 20,
                0xf_u64 << 22,
                0,
                LocalRelationRowFrame::new(4, 1).unwrap(),
            ),
        ];
        for profile in PROFILES {
            let capability = MonotoneOCapability::for_profile(profile).unwrap();
            for &(width, height, minimum, variable, y, frame) in &cases {
                let bottom = 3_u64 << (y as u32 * u32::from(width));
                let expected_lock_mask = bottom | (bottom << width);
                let maximum = minimum | variable;
                let attempt = capability
                    .proves_lock_interval(
                        width,
                        height,
                        minimum,
                        maximum,
                        RotationState::Zero,
                        0,
                        y,
                        expected_lock_mask,
                        frame,
                    )
                    .unwrap();
                assert!(attempt.proven);
                assert!(attempt.row_steps > 0);
                let mut assignment = variable;
                let mut count = 0;
                loop {
                    let board = minimum | assignment;
                    assert!(frame.accepts_physical_board(width, board));
                    let expected = crate::reachability_reference::reference_spawn_lock_anchors(
                        width,
                        height,
                        board,
                        PieceKind::O,
                        profile,
                    )
                    .unwrap();
                    let anchor = 1_u64 << (y as u32 * u32::from(width));
                    for rotation in RotationState::ALL {
                        assert_ne!(expected[rotation.quarter_turns() as usize] & anchor, 0);
                    }
                    count += 1;
                    if assignment == 0 {
                        break;
                    }
                    assignment = (assignment - 1) & variable;
                }
                assert_eq!(count, 16);
            }
        }
    }

    #[test]
    fn interval_cannot_borrow_grounding_from_the_maximum_board() {
        let capability = MonotoneOCapability::for_profile(KickTableProfileId::Jstris180).unwrap();
        let (width, height) = (10, 4);
        let expected_lock_mask = (3_u64 << 10) | (3_u64 << 20);
        assert!(
            capability
                .lock_reachable(width, height, 1, RotationState::Zero, 0, 1)
                .unwrap()
                .reachable
        );
        let attempt = capability
            .proves_lock_interval(
                width,
                height,
                0,
                1,
                RotationState::Zero,
                0,
                1,
                expected_lock_mask,
                LocalRelationRowFrame::new(height, 0).unwrap(),
            )
            .unwrap();
        assert!(!attempt.proven);
        assert_eq!(attempt.row_steps, 0);
    }

    #[test]
    fn interval_blocked_maximum_is_unknown_even_when_minimum_is_reachable() {
        let capability = MonotoneOCapability::for_profile(KickTableProfileId::Jstris180).unwrap();
        let minimum = 0xff_u64 << 20;
        let maximum = minimum | (2..6).fold(0_u64, |mask, y| mask | (1_u64 << (y * 10 + 8)));
        assert!(
            capability
                .lock_reachable(10, 6, minimum, RotationState::Zero, 0, 0)
                .unwrap()
                .reachable
        );
        let attempt = capability
            .proves_lock_interval(
                10,
                6,
                minimum,
                maximum,
                RotationState::Zero,
                0,
                0,
                3 | (3 << 10),
                LocalRelationRowFrame::new(6, 0).unwrap(),
            )
            .unwrap();
        assert!(!attempt.proven);
        assert!(attempt.row_steps > 0);
    }

    #[test]
    fn interval_rejects_wrong_frame_inclusion_and_ilc_mask() {
        let capability = MonotoneOCapability::for_profile(KickTableProfileId::Srs90).unwrap();
        let lock_mask = 3 | (3 << 10);
        let valid_frame = LocalRelationRowFrame::new(4, 0).unwrap();
        let attempt = |minimum, maximum, mask, frame| {
            capability.proves_lock_interval(
                10,
                4,
                minimum,
                maximum,
                RotationState::Zero,
                0,
                0,
                mask,
                frame,
            )
        };
        assert!(attempt(0, 0, lock_mask, valid_frame).unwrap().proven);
        assert!(attempt(1, 0, lock_mask, valid_frame).is_none());
        assert!(attempt(0, 0, lock_mask ^ 1, valid_frame).is_none());
        assert!(attempt(0, 0, lock_mask, LocalRelationRowFrame::new(3, 0).unwrap()).is_none());
        assert!(attempt(
            0,
            1_u64 << 30,
            lock_mask,
            LocalRelationRowFrame::new(4, 1).unwrap(),
        )
        .is_none());
        // Even empty bounds cannot admit an ILC lock above this frame's
        // surviving physical rows.
        assert!(capability
            .proves_lock_interval(
                10,
                4,
                0,
                0,
                RotationState::Zero,
                0,
                1,
                (3 << 10) | (3 << 20),
                LocalRelationRowFrame::new(4, 0b0011).unwrap(),
            )
            .is_none());
        // The small-width single-board primitive is valid, but the current
        // shared compacted-row-frame API explicitly admits only width 10.
        assert!(
            capability
                .lock_reachable(6, 4, 0, RotationState::Zero, 0, 0)
                .unwrap()
                .reachable
        );
        assert!(capability
            .proves_lock_interval(
                6,
                4,
                0,
                0,
                RotationState::Zero,
                0,
                0,
                3 | (3 << 6),
                valid_frame,
            )
            .is_none());
    }
}
