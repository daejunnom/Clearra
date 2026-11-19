//! Board-independent collision dependencies for one canonical spawn-to-lock query.
//!
//! This optional prototype compiles a uniform condition before a tiling or a
//! physical board is known. It does not declare a lock reachable, authorize a
//! negative prune, or replace a completed BuildUp check. Its result says only:
//! under this same template and physical target pose, changing board cells
//! outside the returned mask cannot change whether the target lock is reachable.
//! The caller must separately preserve the canonical operation and original-row
//! frame when lifting this physical condition into a placement relation graph.

use super::*;

// The prototype is deliberately bounded independently of caller allocation
// policy. Unsupported or malformed templates return no certificate.
const MAX_DEPENDENCY_STATES: usize = 8 * 1024;
const MAX_DEPENDENCY_EDGES: usize = 1024 * 1024;

/// Additional bounded scratch for one serial static-mask compilation.
///
/// Every vector is freshly allocated once at its checked final size, and the
/// queue visits each pose at most once. No old/grown allocation pair is kept.
/// Include vector headers as well as all five buffers; this is separate from
/// the already-owned forward template and any retained relation-cache storage.
pub(in super::super) const fn static_dependency_peak_upper_bound() -> usize {
    (MAX_DEPENDENCY_STATES + 1) * core::mem::size_of::<usize>()
        + MAX_DEPENDENCY_STATES * core::mem::size_of::<usize>()
        + MAX_DEPENDENCY_EDGES * core::mem::size_of::<u16>()
        + MAX_DEPENDENCY_STATES * core::mem::size_of::<u8>()
        + MAX_DEPENDENCY_STATES * core::mem::size_of::<u16>()
        + 5 * core::mem::size_of::<Vec<u8>>()
}

/// Compile a sufficient, board-independent dependency mask for one lock.
///
/// We reverse-close the target over *all* potential movement edges, ignoring
/// collision and first-success restrictions. Every concrete path to the target
/// is contained in this optimistic predecessor set. For each predecessor we
/// include every collision test that could change an outgoing edge, including
/// ordered kick competitors whose destinations are outside the predecessor set.
/// Target grounding and canonical sky-seed admission are included as well.
///
/// Thus `(a ^ b) & mask == 0` preserves target-lock membership for valid boards
/// `a` and `b` under the same template. This is a conservative sufficient
/// condition: an intersecting change is unknown, not proof of dependence. The
/// mask may cover the whole board, especially for profiles with upward kicks.
///
/// Reverse adjacency is derived from the forward tables actually consumed by
/// the solver. A stale or truncated auxiliary reverse index cannot silently
/// omit a predecessor from this certificate.
pub(in super::super) fn static_lock_dependency_mask(
    template: &ReachabilityTemplate,
    rotation: RotationState,
    x: i8,
    y: i8,
) -> Option<u64> {
    let (state_count, board_mask) = validate_forward_template(template)?;
    if y < 0 || y >= template.height as i8 {
        return None;
    }
    let goal = state_index(template.width, template.ceiling, State { rotation, x, y })?;
    if *template.state_masks.get(goal)? == INVALID_STATE_MASK {
        return None;
    }

    // CSR predecessor lists keep storage linear in the bounded potential graph.
    let mut offsets = zeroed_vec::<usize>(state_count.checked_add(1)?)?;
    let mut edge_count = 0_usize;
    visit_forward_edges(template, |_, target| {
        edge_count = edge_count.checked_add(1)?;
        if edge_count > MAX_DEPENDENCY_EDGES {
            return None;
        }
        offsets[target + 1] = offsets[target + 1].checked_add(1)?;
        Some(())
    })?;
    for index in 1..offsets.len() {
        offsets[index] = offsets[index].checked_add(offsets[index - 1])?;
    }
    let mut parents = zeroed_vec::<u16>(edge_count)?;
    let mut cursors = zeroed_vec::<usize>(state_count)?;
    cursors.copy_from_slice(&offsets[..state_count]);
    visit_forward_edges(template, |source, target| {
        let cursor = *cursors.get(target)?;
        *parents.get_mut(cursor)? = u16::try_from(source).ok()?;
        cursors[target] = cursor.checked_add(1)?;
        Some(())
    })?;

    let mut seen = zeroed_vec::<u8>(state_count)?;
    let mut queue = Vec::new();
    queue.try_reserve_exact(state_count).ok()?;
    seen[goal] = 1;
    queue.push(u16::try_from(goal).ok()?);
    let mut cursor = 0;
    while cursor < queue.len() {
        let target = usize::from(queue[cursor]);
        cursor += 1;
        for &parent in &parents[offsets[target]..offsets[target + 1]] {
            let source = usize::from(parent);
            if seen[source] == 0 {
                seen[source] = 1;
                queue.push(parent);
            }
        }
    }

    let mut dependency_mask = 0_u64;
    // These are normally empty sky masks, but seed admission is an explicit
    // part of the proof rather than an assumption about the current profile.
    for &seed in &template.sky_seeds {
        dependency_mask |= template.state_masks[usize::from(seed)];
    }
    for source in queue.into_iter().map(usize::from) {
        dependency_mask |= template.state_masks[source];
        for &target in &template.translation_targets[source] {
            if target != INVALID_STATE_INDEX {
                dependency_mask |= template.state_masks[usize::from(target)];
            }
        }
        for slot in 0..if template.allow_180 { 3 } else { 2 } {
            let transition = source * 3 + slot;
            let begin = template.rotation_target_offsets[transition] as usize;
            let end = template.rotation_target_offsets[transition + 1] as usize;
            // Include the entire ordered sequence, not only targets that can
            // themselves reach the goal. An earlier successful competitor can
            // suppress a later kick into the goal's predecessor closure.
            for &target in &template.rotation_targets[begin..end] {
                dependency_mask |= template.state_masks[usize::from(target)];
            }
        }
    }
    // Grounding reads the downward pose even when that pose is unreachable.
    // It is also covered above, but retain this explicit output precondition.
    let below = template.translation_targets[goal][0];
    if y != 0 && below != INVALID_STATE_INDEX {
        dependency_mask |= template.state_masks[usize::from(below)];
    }
    (dependency_mask & !board_mask == 0).then_some(dependency_mask)
}

fn zeroed_vec<T: Default + Clone>(len: usize) -> Option<Vec<T>> {
    let mut values = Vec::new();
    values.try_reserve_exact(len).ok()?;
    values.resize(len, T::default());
    Some(values)
}

fn validate_forward_template(template: &ReachabilityTemplate) -> Option<(usize, u64)> {
    let cells = usize::from(template.width).checked_mul(usize::from(template.height))?;
    if template.width == 0
        || template.width > i8::MAX as u8
        || template.height == 0
        || cells > u64::BITS as usize
        || template.ceiling < template.height as i8
    {
        return None;
    }
    let rows = usize::try_from(template.ceiling).ok()?.checked_add(1)?;
    let state_count = 4_usize
        .checked_mul(rows)?
        .checked_mul(usize::from(template.width))?;
    let transition_count = state_count.checked_mul(3)?;
    if state_count == 0
        || state_count > MAX_DEPENDENCY_STATES
        || template.state_masks.len() != state_count
        || template.translation_targets.len() != state_count
        || template.sky_seeds.len() > state_count
        || template.rotation_target_offsets.len() != transition_count.checked_add(1)?
        || template.rotation_targets.len() > MAX_DEPENDENCY_EDGES
        || template.rotation_kick_indices.len() != template.rotation_targets.len()
        || template.rotation_target_offsets.first().copied() != Some(0)
        || usize::try_from(*template.rotation_target_offsets.last()?).ok()?
            != template.rotation_targets.len()
    {
        return None;
    }
    let board_mask = if cells == u64::BITS as usize {
        u64::MAX
    } else {
        (1_u64 << cells) - 1
    };
    if template
        .state_masks
        .iter()
        .any(|&mask| mask != INVALID_STATE_MASK && mask & !board_mask != 0)
        || template
            .rotation_target_offsets
            .windows(2)
            .any(|pair| pair[0] > pair[1])
    {
        return None;
    }
    for &seed in &template.sky_seeds {
        if *template.state_masks.get(usize::from(seed))? == INVALID_STATE_MASK {
            return None;
        }
    }
    // Validate all table entries once, including entries on unreachable poses.
    // The optimistic graph below omits invalid sources, as the solver does.
    for &target in template.translation_targets.iter().flatten() {
        if target != INVALID_STATE_INDEX
            && *template.state_masks.get(usize::from(target))? == INVALID_STATE_MASK
        {
            return None;
        }
    }
    for &target in &template.rotation_targets {
        if *template.state_masks.get(usize::from(target))? == INVALID_STATE_MASK {
            return None;
        }
    }
    Some((state_count, board_mask))
}

fn visit_forward_edges(
    template: &ReachabilityTemplate,
    mut visit: impl FnMut(usize, usize) -> Option<()>,
) -> Option<()> {
    for source in 0..template.state_masks.len() {
        if template.state_masks[source] == INVALID_STATE_MASK {
            continue;
        }
        for &target in &template.translation_targets[source] {
            if target != INVALID_STATE_INDEX {
                visit(source, usize::from(target))?;
            }
        }
        for slot in 0..if template.allow_180 { 3 } else { 2 } {
            let transition = source * 3 + slot;
            let begin = template.rotation_target_offsets[transition] as usize;
            let end = template.rotation_target_offsets[transition + 1] as usize;
            for &target in &template.rotation_targets[begin..end] {
                visit(source, usize::from(target))?;
            }
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reachability_reference::reference_spawn_lock_anchors;

    const PROFILES: [KickTableProfileId; 5] = [
        KickTableProfileId::Srs90,
        KickTableProfileId::SrsPlus,
        KickTableProfileId::SrsX,
        KickTableProfileId::Jstris180,
        KickTableProfileId::NoKick,
    ];

    #[allow(clippy::too_many_arguments)]
    fn oracle_lock(
        width: u8,
        height: u8,
        board: u64,
        piece: PieceKind,
        profile: KickTableProfileId,
        rotation: RotationState,
        x: i8,
        y: i8,
    ) -> bool {
        let anchors = reference_spawn_lock_anchors(width, height, board, piece, profile)
            .expect("bounded independent primitive oracle");
        anchors[rotation.quarter_turns() as usize]
            & (1_u64
                << (usize::try_from(y).unwrap() * usize::from(width) + usize::try_from(x).unwrap()))
            != 0
    }

    #[test]
    fn outside_mask_completions_preserve_the_independent_lock_result() {
        const WIDTH: u8 = 4;
        const HEIGHT: u8 = 4;
        let piece = PieceKind::O;
        let rotation = RotationState::Zero;
        let (x, y) = (1, 2);
        let all_cells = (1_u64 << (WIDTH * HEIGHT)) - 1;
        for profile in PROFILES {
            let template = ReachabilityTemplate::compile(WIDTH, HEIGHT, piece, profile);
            let mask = static_lock_dependency_mask(&template, rotation, x, y)
                .expect("supported target lock");
            let outside = all_cells & !mask;
            assert_ne!(outside, 0, "fixture must exercise real don't-care cells");
            // The bottom row can vary while the goal's support, target and
            // paths above it are held fixed. Both true and false lock results
            // are checked, including target collision and absent support.
            let base_boards = [0, 1_u64 << 5, 1_u64 << 6, (1_u64 << 5) | (1_u64 << 9)];
            let mut checked = 0;
            for base in base_boards {
                let expected = oracle_lock(WIDTH, HEIGHT, base, piece, profile, rotation, x, y);
                let mut completion = outside;
                loop {
                    let board = (base & mask) | completion;
                    assert_eq!(
                        oracle_lock(WIDTH, HEIGHT, board, piece, profile, rotation, x, y),
                        expected,
                        "{profile:?} base={base:#x} completion={completion:#x} mask={mask:#x}"
                    );
                    let mut scratch = ReachabilityScratch::default();
                    let actual = search_reachable_locks(&template, board, &mut scratch, None);
                    assert!(actual.exhaustive);
                    assert_eq!(actual.locks.contains(WIDTH, rotation, x, y), expected);
                    checked += 1;
                    if completion == 0 {
                        break;
                    }
                    completion = (completion - 1) & outside;
                }
            }
            assert!(checked > base_boards.len());
        }
    }

    #[test]
    fn supported_profiles_and_piece_kinds_produce_bounded_masks() {
        for profile in PROFILES {
            for piece in PieceKind::STANDARD_TETROMINOES {
                let template = ReachabilityTemplate::compile(10, 4, piece, profile);
                for rotation in RotationState::ALL {
                    let mask = static_lock_dependency_mask(&template, rotation, 3, 0)
                        .expect("valid floor lock");
                    assert_ne!(mask, 0);
                    assert_eq!(mask >> 40, 0);
                }
            }
        }
    }

    #[test]
    fn triple_kick_j_surface_is_not_certified_independent_of_the_t_lock() {
        const WIDTH: u8 = 5;
        const HEIGHT: u8 = 6;
        let cells = |coordinates: &[(u8, u8)]| {
            coordinates.iter().fold(0_u64, |mask, &(x, y)| {
                mask | (1_u64 << (usize::from(y) * usize::from(WIDTH) + usize::from(x)))
            })
        };
        let without_j = cells(&[
            (0, 0),
            (1, 0),
            (0, 1),
            (0, 2),
            (3, 0),
            (4, 0),
            (2, 1),
            (3, 1),
            (3, 2),
            (4, 2),
            (2, 3),
            (3, 3),
            (4, 1),
            (4, 3),
        ]);
        let j = cells(&[(0, 3), (0, 4), (0, 5), (1, 5)]);
        let profile = KickTableProfileId::SrsPlus;
        let piece = PieceKind::T;
        let rotation = RotationState::Right;
        assert!(!oracle_lock(
            WIDTH, HEIGHT, without_j, piece, profile, rotation, 1, 1
        ));
        assert!(oracle_lock(
            WIDTH,
            HEIGHT,
            without_j | j,
            piece,
            profile,
            rotation,
            1,
            1
        ));
        let template = ReachabilityTemplate::compile(WIDTH, HEIGHT, piece, profile);
        let mask = static_lock_dependency_mask(&template, rotation, 1, 1).unwrap();
        assert_ne!(mask & j, 0, "J changes the exact ordered-kick result");
    }

    #[test]
    fn earlier_kick_competitor_outside_the_predecessor_set_is_still_observed() {
        let mut template =
            ReachabilityTemplate::compile(4, 4, PieceKind::T, KickTableProfileId::Srs90);
        let index = |rotation, x, y| {
            state_index(template.width, template.ceiling, State { rotation, x, y }).unwrap()
        };
        let source = index(RotationState::Zero, 1, 4);
        let earlier = index(RotationState::Right, 2, 3);
        let goal = index(RotationState::Right, 1, 3);
        let transition = source * 3;
        let start = template.rotation_target_offsets[transition] as usize;
        assert_eq!(template.rotation_targets[start], earlier as u16);
        assert_eq!(template.rotation_targets[start + 1], goal as u16);

        // Isolate a finite transition fragment using the real SRS-90 poses
        // and first two ordered kick candidates. The early destination has
        // no path back to the goal, so it is outside the reverse closure.
        // Its collision test nevertheless selects or suppresses the goal edge.
        template.translation_targets.fill([INVALID_STATE_INDEX; 3]);
        template.rotation_targets = vec![earlier as u16, goal as u16];
        template.rotation_kick_indices = vec![0, 1];
        template.rotation_target_offsets.fill(0);
        template.rotation_target_offsets[transition + 1..].fill(2);
        template.sky_seeds = vec![source as u16];
        // Auxiliary reverse metadata is intentionally stale. The certificate
        // must be compiled from the forward graph that the search consumes.
        let only_earlier = template.state_masks[earlier] & !template.state_masks[goal];
        assert_ne!(only_earlier, 0);
        let mask = static_lock_dependency_mask(&template, RotationState::Right, 1, 3).unwrap();
        assert_eq!(mask & only_earlier, only_earlier);

        let mut scratch = ReachabilityScratch::default();
        let open_earlier = search_reachable_locks(&template, 0, &mut scratch, None);
        let block_earlier = search_reachable_locks(&template, only_earlier, &mut scratch, None);
        assert!(!open_earlier.locks.contains(4, RotationState::Right, 1, 3));
        assert!(block_earlier.locks.contains(4, RotationState::Right, 1, 3));
    }

    #[test]
    fn invalid_goal_or_malformed_template_produces_no_certificate() {
        let mut template =
            ReachabilityTemplate::compile(4, 4, PieceKind::T, KickTableProfileId::SrsPlus);
        for (x, y) in [(-1, 0), (4, 0), (0, -1), (0, 4), (3, 0)] {
            assert!(static_lock_dependency_mask(&template, RotationState::Zero, x, y).is_none());
        }
        template.rotation_target_offsets.pop();
        assert!(static_lock_dependency_mask(&template, RotationState::Zero, 0, 0).is_none());
    }
}
