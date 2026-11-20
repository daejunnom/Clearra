//! Scoped collision certificates for canonical spawn-to-lock queries.
//!
//! This optional prototype compiles a uniform condition before a tiling or a
//! physical board is known. It does not declare a lock reachable, authorize a
//! negative prune, or replace a completed BuildUp check. Its result says only:
//! under this same template and physical target pose, changing board cells
//! outside the returned mask cannot change whether the target lock is reachable.
//! The separate bounded no-entry check can exclude one exact target pose over
//! a fixed-frame occupancy interval. The caller must preserve the canonical
//! operation and original-row frame when lifting either physical result.

use super::*;

// The prototype is deliberately bounded independently of caller allocation
// policy. Unsupported or malformed templates return no certificate.
const MAX_DEPENDENCY_STATES: usize = 8 * 1024;
const MAX_DEPENDENCY_EDGES: usize = 1024 * 1024;

// Most negatives should have no incoming edge at all. Also allow a few
// mutually reachable poses (including rotation cycles) in a sealed cavity,
// without turning a useful small certificate into another full search.
const MAX_NO_ENTRY_POSES: usize = 16;

/// Absence of any canonical sky seed in a small, closed optimistic predecessor
/// set proves that the exact target pose cannot be reached. The already-owned
/// reverse tables were compiled from the solver's actual forward transitions.
/// They include all legal rotation slots and every geometrically valid kick,
/// even kicks suppressed on the actual board by first-success priority.
///
/// Any board containing `minimum_board` can only remove vertices from this
/// optimistic graph: a predecessor colliding with minimum occupancy remains
/// blocked. This is not a monotonicity argument for the ordered-kick solver
/// itself. No target grounding, positive witness, or higher-level
/// unreachability is inferred. A seed, unsupported pose, or work beyond the
/// fixed-size closure yields Unknown.
pub(super) fn bounded_no_entry_on_minimum_board(
    template: &ReachabilityTemplate,
    minimum_board: u64,
    target: usize,
) -> bool {
    bounded_no_entry(template, minimum_board, target, |_| true).0
}

/// Experimental fixed-frame refinement: an earlier kick that is free on
/// the maximum board is also free on every board in the interval. This
/// suppresses the later candidate on all those boards. No inference of
/// monotonicity for the actual ordered-kick graph is needed.
///
/// This proof is valid only inside [minimum_board, maximum_board]. It must
/// not become the all-supersets `MovementProof::Closed` used by BuildUp.
pub(super) fn bounded_no_entry_on_ordered_interval(
    template: &ReachabilityTemplate,
    minimum_board: u64,
    maximum_board: u64,
    target: usize,
) -> (bool, usize) {
    let bits = u32::from(template.width) * u32::from(template.height);
    if !(1..=64).contains(&bits)
        || minimum_board & !maximum_board != 0
        || (bits < 64 && maximum_board >> bits != 0)
    {
        return (false, 0);
    }
    bounded_no_entry(template, minimum_board, target, |source| {
        reverse_rotation_is_first_success(template, maximum_board, source)
    })
}

/// From a successful point proof, retain only the earlier kick footprints
/// whose absence excluded an otherwise placeable predecessor. All minimum
/// occupancy is kept, so every blocked predecessor stays blocked. Every
/// retained earlier footprint must stay vacant, so each suppressed later
/// kick stays unavailable. The exact same closure then excludes every sky
/// entry throughout the resulting interval.
pub(super) fn bounded_ordered_no_entry_clear_mask(
    template: &ReachabilityTemplate,
    board: u64,
    target: usize,
) -> (Option<u64>, usize) {
    let mut must_remain_clear = 0;
    let (proven, checked_poses) = bounded_no_entry(template, board, target, |source| {
        // A collision at minimum already excludes the predecessor without
        // requiring any of its earlier kick targets to remain clear.
        let source_mask = template.state_masks[usize::from(source.source)];
        if source_mask == INVALID_STATE_MASK || board & source_mask != 0 {
            return false;
        }
        let earlier = reverse_rotation_prior_targets(template, source)
            .iter()
            .map(|&earlier| template.state_masks[usize::from(earlier)])
            .find(|&mask| board & mask == 0);
        if let Some(mask) = earlier {
            must_remain_clear |= mask;
            false
        } else {
            true
        }
    });
    (proven.then_some(must_remain_clear), checked_poses)
}

fn bounded_no_entry(
    template: &ReachabilityTemplate,
    minimum_board: u64,
    target: usize,
    mut rotation_can_enter: impl FnMut(ReverseRotationSource) -> bool,
) -> (bool, usize) {
    let Some(&target_mask) = template.state_masks.get(target) else {
        return (false, 0);
    };
    let Ok(target) = u16::try_from(target) else {
        return (false, 0);
    };
    if target == INVALID_STATE_INDEX
        || target_mask == INVALID_STATE_MASK
        || (minimum_board & target_mask) != 0
    {
        return (false, 0);
    }

    let mut poses = [target; MAX_NO_ENTRY_POSES];
    let mut count = 1;
    let mut cursor = 0;
    while cursor < count {
        let pose = usize::from(poses[cursor]);
        cursor += 1;
        if template.is_sky_seed(pose) {
            return (false, cursor);
        }

        let begin = template.reverse_rotation_offsets[pose] as usize;
        let end = template.reverse_rotation_offsets[pose + 1] as usize;
        let rotations = template.reverse_rotation_sources[begin..end]
            .iter()
            .filter(|&&source| rotation_can_enter(source))
            .map(|source| source.source);
        let translations = template.reverse_translation_sources[pose].into_iter();
        for source in translations.chain(rotations) {
            if source == INVALID_STATE_INDEX {
                continue;
            }
            let mask = template.state_masks[usize::from(source)];
            if mask == INVALID_STATE_MASK || (minimum_board & mask) != 0 {
                continue;
            }
            if poses[..count].contains(&source) {
                continue;
            }
            let Some(next) = poses.get_mut(count) else {
                return (false, cursor);
            };
            *next = source;
            count += 1;
        }
    }
    (true, cursor)
}

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
/// is contained in this optimistic predecessor set. Preserve the induced
/// transition graph on that set: its pose masks cover all translation tests,
/// while a rotation also reads every earlier first-success competitor before
/// its last candidate inside the set. Competitors may lie outside the set.
/// Later kicks and independent translations that leave the predecessor set
/// cannot affect target membership. Target grounding is included separately.
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
    // Every concrete path from a seed to the goal stays in this reverse
    // closure: a pose outside it has no potential path to the goal at all.
    // Seed admission inside the closure is covered by its pose mask below;
    // admitting or blocking any other seed cannot affect this Boolean query.
    for source in queue.into_iter().map(usize::from) {
        dependency_mask |= template.state_masks[source];
        // Translation inputs do not compete with one another. A translation
        // whose destination is inside the closure reads an already included
        // pose mask; one leaving it can never return and need not be read.
        for slot in 0..if template.allow_180 { 3 } else { 2 } {
            let transition = source * 3 + slot;
            let begin = template.rotation_target_offsets[transition] as usize;
            let end = template.rotation_target_offsets[transition + 1] as usize;
            let targets = &template.rotation_targets[begin..end];
            let Some(last_relevant) = targets
                .iter()
                .rposition(|&target| seen[usize::from(target)] != 0)
            else {
                continue;
            };
            // An earlier success outside the closure suppresses a later
            // relevant kick and MUST stay in the footprint. Once all relevant
            // candidates have failed, the eventual result is outside the
            // closure regardless of later collisions, so that suffix is free.
            for &target in &targets[..=last_relevant] {
                dependency_mask |= template.state_masks[usize::from(target)];
            }
        }
    }
    // Grounding reads the downward pose even when that pose is unreachable.
    // That translation can leave the reverse closure and is an explicit
    // output precondition, unlike irrelevant outgoing translation inputs.
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
    fn bounded_no_entry_proves_t_blocked_after_l_clear_not_before() {
        const BEFORE_CLEAR: u64 = 0x3f1fc3f7;
        const AFTER_CLEAR: u64 = 0x000f_f7f8;
        for profile in PROFILES {
            let template = ReachabilityTemplate::compile(10, 3, PieceKind::T, profile);
            let target = |y| {
                state_index(
                    template.width,
                    template.ceiling,
                    State {
                        rotation: RotationState::Zero,
                        x: 0,
                        y,
                    },
                )
                .unwrap()
            };
            assert!(bounded_no_entry_on_minimum_board(
                &template,
                AFTER_CLEAR,
                target(0)
            ));
            assert!(!oracle_lock(
                10,
                3,
                AFTER_CLEAR,
                PieceKind::T,
                profile,
                RotationState::Zero,
                0,
                0
            ));
            assert!(!bounded_no_entry_on_minimum_board(
                &template,
                BEFORE_CLEAR,
                target(1)
            ));
            assert!(oracle_lock(
                10,
                3,
                BEFORE_CLEAR,
                PieceKind::T,
                profile,
                RotationState::Zero,
                0,
                1
            ));
        }
    }

    #[test]
    fn ordered_negative_eliminates_a_later_kick_only_inside_its_interval() {
        const MINIMUM: u64 = 0x4002;
        const WITH_IRRELEVANT_BLOCKER: u64 = MINIMUM | (1 << 20);
        const WITH_EARLIER_KICK_BLOCKED: u64 = MINIMUM | (1 << 11);
        let profile = KickTableProfileId::Srs90;
        let piece = PieceKind::T;
        let rotation = RotationState::Zero;
        let template = ReachabilityTemplate::compile(10, 3, piece, profile);
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

        // The target's left, right and upper predecessors collide at min.
        // Its only clear rotation predecessor is Right(2,0), aimed at the
        // target by kick #2. Kick #0 (Zero(1,1)) wins throughout the narrower
        // interval. Occupying (1,1) removes that earlier winner instead.
        assert!(!bounded_no_entry_on_minimum_board(
            &template, MINIMUM, target
        ));
        let attempt = |minimum, maximum| {
            bounded_no_entry_on_ordered_interval(&template, minimum, maximum, target)
        };
        assert_eq!(attempt(MINIMUM, MINIMUM), (true, 1));
        assert_eq!(attempt(MINIMUM, WITH_IRRELEVANT_BLOCKER), (true, 1));
        assert!(!attempt(MINIMUM, WITH_EARLIER_KICK_BLOCKED).0);
        assert_eq!(attempt(WITH_IRRELEVANT_BLOCKER, MINIMUM), (false, 0));
        assert_eq!(attempt(MINIMUM, MINIMUM | (1 << 30)), (false, 0));
        let (required_clear, visited) =
            bounded_ordered_no_entry_clear_mask(&template, MINIMUM, target);
        assert_eq!(visited, 1);
        let maximum =
            ((1_u64 << 30) - 1) & !(required_clear.unwrap() | template.state_masks[target]);
        assert_eq!(attempt(MINIMUM, maximum), (true, 1));
        assert_ne!(maximum & (1 << 20), 0, "irrelevant blocker can vary");
        assert_eq!(maximum & (1 << 11), 0, "earlier kick must stay clear");
        for (board, reachable) in [
            (MINIMUM, false),
            (WITH_IRRELEVANT_BLOCKER, false),
            (WITH_EARLIER_KICK_BLOCKED, true),
        ] {
            assert_eq!(
                oracle_lock(10, 3, board, piece, profile, rotation, 2, 0),
                reachable
            );
        }
    }

    #[test]
    fn ordered_negative_matches_independent_locks_across_small_intervals() {
        const OPTIONAL: [u64; 4] = [1 << 6, 1 << 11, 1 << 18, 1 << 20];
        let piece = PieceKind::T;
        let rotation = RotationState::Zero;
        let mut new_interval_proofs = 0;
        let mut completions = 0;
        for profile in PROFILES {
            let template = ReachabilityTemplate::compile(10, 3, piece, profile);
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
            for base in [0x4002, 0x10_5800, 0x03c4_0840] {
                let base_only = bounded_no_entry_on_minimum_board(&template, base, target);
                for choices in 0..1 << OPTIONAL.len() {
                    let extra = OPTIONAL.iter().enumerate().fold(0, |mask, (index, &bit)| {
                        if choices & (1 << index) == 0 {
                            mask
                        } else {
                            mask | bit
                        }
                    }) & !base;
                    if !bounded_no_entry_on_ordered_interval(&template, base, base | extra, target)
                        .0
                    {
                        continue;
                    }
                    new_interval_proofs += usize::from(!base_only && extra != 0);
                    let mut completion = extra;
                    loop {
                        let board = base | completion;
                        assert!(
                            !oracle_lock(10, 3, board, piece, profile, rotation, 2, 0),
                            "{profile:?}: target reached on {board:#x} in [{base:#x}, {:#x}]",
                            base | extra,
                        );
                        completions += 1;
                        if completion == 0 {
                            break;
                        }
                        completion = (completion - 1) & extra;
                    }
                }
            }
        }
        assert!(
            new_interval_proofs > 0,
            "must exercise nondegenerate new certificates"
        );
        assert!(completions > new_interval_proofs);
    }

    #[test]
    fn bounded_no_entry_requires_a_seedless_closed_predecessor_set() {
        let profile = KickTableProfileId::NoKick;
        let template = ReachabilityTemplate::compile(6, 5, PieceKind::O, profile);
        let index = |x, y| {
            state_index(
                template.width,
                template.ceiling,
                State {
                    rotation: RotationState::Zero,
                    x,
                    y,
                },
            )
            .unwrap()
        };
        let target = index(0, 0);
        let bottom_wall = 0b111000 | (0b111000 << 6);
        let sealed = bottom_wall | (0b000111 << 12);

        // This is not the one-pose proof: the x=1 pose is a valid predecessor
        // of x=0 and can be reached from x=0 in the optimistic graph.
        assert!(template.reverse_translation_sources[target].contains(&(index(1, 0) as u16)));
        assert!(template.reverse_translation_sources[index(1, 0)].contains(&(target as u16)));
        assert_eq!(template.state_masks[index(1, 0)] & sealed, 0);
        assert!(bounded_no_entry_on_minimum_board(&template, sealed, target));
        assert!(!oracle_lock(
            6,
            5,
            sealed,
            PieceKind::O,
            profile,
            RotationState::Zero,
            0,
            0
        ));

        // Removing the roof connects to the product's real sky seeds.
        assert!(!bounded_no_entry_on_minimum_board(
            &template,
            bottom_wall,
            target
        ));
        assert!(oracle_lock(
            6,
            5,
            bottom_wall,
            PieceKind::O,
            profile,
            RotationState::Zero,
            0,
            0
        ));
        assert!(!bounded_no_entry_on_minimum_board(
            &template,
            sealed,
            index(0, 5)
        ));
    }

    #[test]
    fn bounded_no_entry_does_not_turn_budget_exhaustion_into_a_negative() {
        let profile = KickTableProfileId::NoKick;
        let template = ReachabilityTemplate::compile(10, 6, PieceKind::O, profile);
        let target = state_index(
            template.width,
            template.ceiling,
            State {
                rotation: RotationState::Zero,
                x: 0,
                y: 0,
            },
        )
        .unwrap();
        // Only one cell is open in the roof. The wide cavity underneath
        // contains more optimistic predecessors than the bounded proof allows.
        let wide_cavity = 0x1ff_u64 << 40;
        assert!(!bounded_no_entry_on_minimum_board(
            &template,
            wide_cavity,
            target
        ));
        assert!(!oracle_lock(
            10,
            6,
            wide_cavity,
            PieceKind::O,
            profile,
            RotationState::Zero,
            0,
            0
        ));
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
    fn o_target_keeps_only_its_own_support_cells_below_the_ancestor_rows() {
        // With no translational O rotation or upward move, every possible
        // predecessor is at y >= 2. Horizontal moves can cross the full width,
        // so all cells in rows 2..4 stay relevant, but below them only the two
        // target support cells at (1,1), (2,1) affect this Boolean lock query.
        const EXPECTED: u64 = 0xff60;
        for profile in PROFILES {
            let template = ReachabilityTemplate::compile(4, 4, PieceKind::O, profile);
            for rotation in RotationState::ALL {
                assert_eq!(
                    static_lock_dependency_mask(&template, rotation, 1, 2),
                    Some(EXPECTED),
                    "{profile:?}/{rotation:?}"
                );
            }
        }
    }

    #[test]
    fn static_target_masks_preserve_every_outside_bit_change_across_profiles_and_kinds() {
        const WIDTH: u8 = 6;
        const HEIGHT: u8 = 4;
        const CELLS: usize = (WIDTH * HEIGHT) as usize;
        let full = (1_u64 << CELLS) - 1;
        let bases = [0, 1_u64 << 7, (0x1f_u64 << 12) | (1_u64 << 5)];
        let mut perturbations = 0;
        for profile in PROFILES {
            for piece in PieceKind::STANDARD_TETROMINOES {
                let template = ReachabilityTemplate::compile(WIDTH, HEIGHT, piece, profile);
                for board in bases {
                    let oracle =
                        reference_spawn_lock_anchors(WIDTH, HEIGHT, board, piece, profile).unwrap();
                    let mut changed_oracles: [Option<[u64; 4]>; CELLS] = [None; CELLS];
                    for rotation in RotationState::ALL {
                        let (x, y) = (1, 2);
                        let mask = static_lock_dependency_mask(&template, rotation, x, y).unwrap();
                        let anchor = 1_u64 << (y as usize * usize::from(WIDTH) + x as usize);
                        let expected = oracle[rotation.quarter_turns() as usize] & anchor != 0;
                        let mut outside = full & !mask;
                        while outside != 0 {
                            let cell = outside.trailing_zeros() as usize;
                            outside &= outside - 1;
                            let changed = *changed_oracles[cell].get_or_insert_with(|| {
                                reference_spawn_lock_anchors(
                                    WIDTH,
                                    HEIGHT,
                                    board ^ (1_u64 << cell),
                                    piece,
                                    profile,
                                )
                                .unwrap()
                            });
                            assert_eq!(
                                changed[rotation.quarter_turns() as usize] & anchor != 0,
                                expected,
                                "{profile:?}/{piece:?}/{rotation:?}, board={board:#x}, cell={cell}, mask={mask:#x}"
                            );
                            perturbations += 1;
                        }
                    }
                }
            }
        }
        assert!(perturbations > 100, "exercise nonempty outside regions");
    }

    #[test]
    fn a_remote_o_entry_barrier_is_not_a_fixed_two_cell_neighborhood() {
        const WIDTH: u8 = 6;
        const HEIGHT: u8 = 6;
        // Leave a one-cell opening, so this is not an uncleared full row. An O
        // cannot cross the barrier although the goal and two rows above it are
        // unchanged. A small local guard is exact only with a proved entry.
        let remote_barrier = 0x1f_u64 << (4 * WIDTH);
        for profile in PROFILES {
            assert!(oracle_lock(
                WIDTH,
                HEIGHT,
                0,
                PieceKind::O,
                profile,
                RotationState::Zero,
                0,
                0
            ));
            assert!(!oracle_lock(
                WIDTH,
                HEIGHT,
                remote_barrier,
                PieceKind::O,
                profile,
                RotationState::Zero,
                0,
                0
            ));
            let template = ReachabilityTemplate::compile(WIDTH, HEIGHT, PieceKind::O, profile);
            let mask = static_lock_dependency_mask(&template, RotationState::Zero, 0, 0).unwrap();
            assert_ne!(mask & remote_barrier, 0);
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
    fn later_kicks_outgoing_translations_and_unrelated_seeds_do_not_expand_the_mask() {
        let mut template =
            ReachabilityTemplate::compile(4, 4, PieceKind::T, KickTableProfileId::Srs90);
        let index = |rotation, x, y| {
            state_index(template.width, template.ceiling, State { rotation, x, y }).unwrap()
        };
        let source = index(RotationState::Zero, 1, 4);
        let earlier = index(RotationState::Right, 2, 3);
        let goal = index(RotationState::Right, 1, 3);
        let later = index(RotationState::Left, 0, 0);
        let transition = source * 3;

        // The exact finite movement table has one competing rotation and an
        // independent translation to a sink. The sink is also a separate
        // seed. None of its paths can ever enter the goal's predecessor set.
        template.translation_targets.fill([INVALID_STATE_INDEX; 3]);
        template.translation_targets[source][1] = later as u16;
        template.rotation_targets = vec![earlier as u16, goal as u16, later as u16];
        template.rotation_kick_indices = vec![0, 1, 2];
        template.rotation_target_offsets.fill(0);
        template.rotation_target_offsets[transition + 1..].fill(3);
        template.sky_seeds = vec![source as u16, later as u16];
        let earlier_only = template.state_masks[earlier] & !template.state_masks[goal];
        let later_only = template.state_masks[later]
            & !(template.state_masks[source]
                | template.state_masks[earlier]
                | template.state_masks[goal]);
        assert_ne!(earlier_only, 0);
        assert_ne!(later_only, 0);
        let mask = static_lock_dependency_mask(&template, RotationState::Right, 1, 3).unwrap();
        assert_eq!(mask & earlier_only, earlier_only);
        assert_eq!(mask & later_only, 0);

        let mut scratch = ReachabilityScratch::default();
        for block_earlier in [false, true] {
            let base = if block_earlier { earlier_only } else { 0 };
            for sink_occupancy in [0, later_only] {
                let result =
                    search_reachable_locks(&template, base | sink_occupancy, &mut scratch, None);
                assert_eq!(
                    result.locks.contains(4, RotationState::Right, 1, 3),
                    block_earlier
                );
            }
        }
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
