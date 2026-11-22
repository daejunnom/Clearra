//! Structural qualifier for the finite graph trusted by the frontier cut.
//!
//! Read the four normalized shape cells and the rule's source-ordered kick
//! offsets, not the compiler's coordinate, normalization, seed, forward,
//! reverse, or traversal helpers. This checks every symbolic edge, even if
//! no sampled occupancy happens to use it. The builtin rule data and shape
//! registry are shared authority; this is not a proof of external game data.

use super::{
    KickTableProfile, KickTableProfileId, KickTransition, PieceKind, ReachabilityTemplate,
};
use clearra_core_domain::piece::rotation::RotationState;
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_rules::kicks::{NoKick, SrsKicks};

const PROFILES: [(KickTableProfileId, fn() -> KickTableProfile); 5] = [
    (KickTableProfileId::Srs90, SrsKicks::profile),
    (KickTableProfileId::SrsPlus, SrsKicks::srs_plus_profile),
    (KickTableProfileId::SrsX, SrsKicks::srs_x_profile),
    (KickTableProfileId::Jstris180, SrsKicks::jstris_180_profile),
    (KickTableProfileId::NoKick, NoKick::profile),
];

const ROTATIONS: [RotationState; 4] = [
    RotationState::Zero,
    RotationState::Right,
    RotationState::Two,
    RotationState::Left,
];
const TRANSLATIONS: [(i16, i16); 3] = [(0, -1), (-1, 0), (1, 0)];
type Cells = [[(i16, i16); 4]; 4];
type Offset = (i16, i16);
type InverseRotationEdge = (u16, u8, u32);

// Independent footprint-anchor offsets from the scalar geometry convention:
// raw offset + source anchor offset − target anchor offset.
fn anchor_offsets(piece: PieceKind) -> [Offset; 4] {
    match piece {
        PieceKind::I => [(0, 0), (-2, 2), (0, 1), (-1, 2)],
        PieceKind::O => [(0, 0); 4],
        _ => [(1, 0), (0, 1), (1, 1), (1, 1)],
    }
}

#[derive(Debug, Eq, PartialEq)]
enum StructuralFault {
    Domain,
    FiniteCeiling,
    StateMasks,
    Seeds,
    Translation,
    Rotation,
    InverseTranslation,
    InverseRotation,
}

#[derive(Clone, Copy)]
struct PoseDomain {
    width: i16,
    height: i16,
    ceiling: i16,
}

impl PoseDomain {
    fn index(self, rotation: usize, x: i16, y: i16) -> Option<u16> {
        if rotation >= 4 || !(0..self.width).contains(&x) || !(0..=self.ceiling).contains(&y) {
            return None;
        }
        let plane = usize::try_from(self.width * (self.ceiling + 1)).ok()?;
        let row = usize::try_from(self.width * y + x).ok()?;
        u16::try_from(rotation * plane + row).ok()
    }

    fn mask(self, cells: &[(i16, i16); 4], x: i16, y: i16) -> Option<u64> {
        let mut result = 0_u64;
        for &(dx, dy) in cells {
            let column = x + dx;
            let row = y + dy;
            if !(0..self.width).contains(&column) || row < 0 {
                return None;
            }
            if row < self.height {
                result |= 1_u64 << (row * self.width + column);
            }
        }
        Some(result)
    }
}

fn verify_template(
    width: u8,
    height: u8,
    piece: PieceKind,
    profile: &KickTableProfile,
    template: &ReachabilityTemplate,
) -> Result<(), StructuralFault> {
    use StructuralFault::*;

    if !(1..=10).contains(&width)
        || !(1..=6).contains(&height)
        || template.width != width
        || template.height != height
        || template.allow_180 != profile.supports_180()
    {
        return Err(Domain);
    }

    let definition = standard_tetromino_registry().get(piece).ok_or(Domain)?;
    let cells: Cells = ROTATIONS.map(|rotation| {
        definition
            .shape(rotation)
            .cells()
            .map(|cell| (i16::from(cell.x()), i16::from(cell.y())))
    });
    for shape in &cells {
        for (index, &(dx, dy)) in shape.iter().enumerate() {
            if dx < 0 || dy < 0 || shape[..index].contains(&(dx, dy)) {
                return Err(Domain);
            }
        }
    }

    let anchors = anchor_offsets(piece);
    let to_rotation = |from, slot| (from + [1, 3, 2][slot]) % 4;
    let deltas: [Vec<Offset>; 12] = std::array::from_fn(|transition| {
        let from = transition / 3;
        let slot = transition % 3;
        if slot == 2 && !profile.supports_180() {
            return Vec::new();
        }
        let to = to_rotation(from, slot);
        let Some(sequence) =
            profile.sequence_for(KickTransition::new(piece, ROTATIONS[from], ROTATIONS[to]))
        else {
            return Vec::new();
        };
        let (ax, ay) = anchors[from];
        let (bx, by) = anchors[to];
        sequence
            .offsets()
            .iter()
            .map(|offset| {
                (
                    i16::from(offset.dx()) + ax - bx,
                    i16::from(offset.dy()) + ay - by,
                )
            })
            .collect()
    });
    let maximum_kick_drop = deltas
        .iter()
        .flatten()
        .fold(0, |maximum, &(_, dy)| maximum.max(-dy));

    // With nonnegative normalized cell offsets and occupancy only below H,
    // any path from above H to a below-H target has a *last* entry below H.
    // The source of that edge is at most H−1+max(1,max_kick_drop), hence is
    // within this ceiling and geometrically valid, hence is a seed. The
    // suffix always stays below H. Hard drop is a sequence of unit descents
    // for Boolean reachability only; an unmodeled long jump is out of scope.
    // Filtering out-of-ceiling kick destinations cannot disable a genuine
    // successful suffix edge (it only removes priority candidates).
    // This hypothesis concerns negatives, not exact spawn/positive inputs.
    let top = i16::from(height) + maximum_kick_drop;
    let highest_crossing_source = i16::from(height) - 1 + maximum_kick_drop.max(1);
    if !(highest_crossing_source..=i16::from(i8::MAX)).contains(&top)
        || i16::from(template.ceiling) != top
    {
        return Err(FiniteCeiling);
    }

    let bounds = PoseDomain {
        width: i16::from(width),
        height: i16::from(height),
        ceiling: top,
    };
    let poses = 4 * usize::from(width) * (top as usize + 1);
    let mut masks = Vec::with_capacity(poses);
    let mut seeds = vec![false; poses];
    for (rotation, shape) in cells.iter().enumerate() {
        for y in 0..=top {
            for x in 0..bounds.width {
                let mask = bounds.mask(shape, x, y);
                if mask.is_some() && y >= bounds.height {
                    seeds[usize::from(bounds.index(rotation, x, y).ok_or(StateMasks)?)] = true;
                }
                masks.push(mask);
            }
        }
    }
    if template.state_masks.len() != poses
        || !masks
            .iter()
            .zip(&template.state_masks)
            .all(|(expected, &actual)| expected.unwrap_or(u64::MAX) == actual)
    {
        return Err(StateMasks);
    }
    if template.sky_seeds.len() > poses {
        return Err(Seeds);
    }
    let mut list_seeds = vec![false; poses];
    for &seed in &template.sky_seeds {
        let present = list_seeds.get_mut(usize::from(seed)).ok_or(Seeds)?;
        if *present {
            return Err(Seeds);
        }
        *present = true;
    }
    let mut words = vec![0_u64; poses.div_ceil(64)];
    for (index, &present) in seeds.iter().enumerate() {
        if present {
            words[index / 64] |= 1 << (index % 64);
        }
    }
    if list_seeds != seeds || words != template.sky_seed_words {
        return Err(Seeds);
    }

    let valid_destination = |rotation, x, y| {
        let index = bounds.index(rotation, x, y)?;
        masks[usize::from(index)].map(|_| index)
    };
    let mut expected_inv_translations = vec![[u16::MAX; 3]; poses];
    let mut expected_inv_rotations = vec![Vec::<InverseRotationEdge>::new(); poses];
    let filtered = template.globally_pruned_rotation_sources != 0;
    let mut impossible_rotations = 0;
    if template.translation_targets.len() != poses {
        return Err(Translation);
    }
    if template.rotation_target_offsets.len() != poses * 3 + 1
        || template.rotation_target_offsets.first() != Some(&0)
        || template.rotation_target_offsets.last().copied()
            != u32::try_from(template.rotation_targets.len()).ok()
        || template.rotation_targets.len() != template.rotation_kick_indices.len()
    {
        return Err(Rotation);
    }
    for rotation in 0..4 {
        for y in 0..=top {
            for x in 0..bounds.width {
                let source = bounds.index(rotation, x, y).ok_or(StateMasks)?;
                let source_index = usize::from(source);
                // Validate all raw slots—even outgoing slots of geometrically
                // invalid sources are stored, but such sources cannot move.
                for (slot, (dx, dy)) in TRANSLATIONS.into_iter().enumerate() {
                    let expected = valid_destination(rotation, x + dx, y + dy);
                    if template.translation_targets[source_index][slot]
                        != expected.unwrap_or(u16::MAX)
                    {
                        return Err(Translation);
                    }
                    if let Some(destination) = expected {
                        let incoming =
                            &mut expected_inv_translations[usize::from(destination)][slot];
                        if *incoming != u16::MAX {
                            return Err(InverseTranslation);
                        }
                        *incoming = source;
                    }
                }
                for slot in 0..3 {
                    let transition = source_index * 3 + slot;
                    let from = template.rotation_target_offsets[transition] as usize;
                    let to = template.rotation_target_offsets[transition + 1] as usize;
                    let targets = template.rotation_targets.get(from..to).ok_or(Rotation)?;
                    let original_indices = template
                        .rotation_kick_indices
                        .get(from..to)
                        .ok_or(Rotation)?;
                    let mut packed = 0;
                    let mut earlier_masks = Vec::new();
                    for (original, &(dx, dy)) in deltas[rotation * 3 + slot].iter().enumerate() {
                        let Some(destination) =
                            valid_destination(to_rotation(rotation, slot), x + dx, y + dy)
                        else {
                            continue;
                        };
                        if targets.get(packed).copied() != Some(destination)
                            || original_indices.get(packed).copied()
                                != Some(u32::try_from(original).map_err(|_| Rotation)?)
                        {
                            return Err(Rotation);
                        }
                        let target_mask = masks[usize::from(destination)].ok_or(Rotation)?;
                        // Independent E2 qualification from reconstructed raw
                        // footprints: an edge exists on some occupancy iff
                        // every earlier candidate has a potential blocker
                        // outside the collision-free source/destination union.
                        // The earlier list is never shortened by this test.
                        let possible = masks[source_index].is_some_and(|source_mask| {
                            let empty = source_mask | target_mask;
                            earlier_masks.iter().all(|prior: &u64| prior & !empty != 0)
                        });
                        impossible_rotations += usize::from(!possible);
                        if !filtered || possible {
                            expected_inv_rotations[usize::from(destination)].push((
                                source,
                                slot as u8,
                                u32::try_from(packed).map_err(|_| InverseRotation)?,
                            ));
                        }
                        earlier_masks.push(target_mask);
                        packed += 1;
                    }
                    if packed != targets.len() {
                        return Err(Rotation);
                    }
                }
            }
        }
    }
    if expected_inv_translations != template.reverse_translation_sources {
        return Err(InverseTranslation);
    }
    if template.reverse_rotation_offsets.len() != poses + 1
        || template.reverse_rotation_offsets.first() != Some(&0)
        || template.reverse_rotation_offsets.last().copied()
            != u32::try_from(template.reverse_rotation_sources.len()).ok()
        || (filtered && template.globally_pruned_rotation_sources != impossible_rotations)
        || template
            .rotation_targets
            .len()
            .checked_sub(template.reverse_rotation_sources.len())
            != Some(template.globally_pruned_rotation_sources)
    {
        return Err(InverseRotation);
    }
    let mut seen = Vec::new();
    for (destination, expected) in expected_inv_rotations.iter().enumerate() {
        let from = template.reverse_rotation_offsets[destination] as usize;
        let to = template.reverse_rotation_offsets[destination + 1] as usize;
        let incoming = template
            .reverse_rotation_sources
            .get(from..to)
            .ok_or(InverseRotation)?;
        seen.clear();
        seen.extend(
            incoming
                .iter()
                .map(|edge| (edge.source, edge.slot, edge.prior_target_count)),
        );
        // Incoming enumeration order has no meaning; slot and packed prefix
        // identity do, including duplicates that would hide an omitted edge.
        seen.sort_unstable();
        if seen.as_slice() != expected.as_slice() {
            return Err(InverseRotation);
        }
    }
    Ok(())
}

#[test]
fn frontier_certificate_template_matches_all_compact_builtin_symbolic_edges() {
    for (id, rule) in PROFILES {
        let profile = rule();
        assert_eq!(profile.id(), id);
        for piece in PieceKind::STANDARD_TETROMINOES {
            for width in 1..=10 {
                for height in 1..=6 {
                    let template = ReachabilityTemplate::compile(width, height, piece, id);
                    assert_eq!(
                        verify_template(width, height, piece, &profile, &template),
                        Ok(()),
                        "{id:?}/{piece:?}, {width}×{height}"
                    );
                }
            }
        }
    }
}

#[test]
fn frontier_certificate_template_rejects_consistent_omissions_and_changed_priority() {
    const WIDTH: u8 = 10;
    const HEIGHT: u8 = 3;
    const PIECE: PieceKind = PieceKind::T;
    let profile = SrsKicks::profile();
    let compile = || ReachabilityTemplate::compile(WIDTH, HEIGHT, PIECE, KickTableProfileId::Srs90);
    let check = |template| verify_template(WIDTH, HEIGHT, PIECE, &profile, template);

    // Removing the same real descent from both compiled lists fools a
    // forward-vs-reverse-only comparer, but not the rule boundary.
    let mut template = compile();
    let source = usize::from(WIDTH) + 1;
    let target = 1;
    assert_eq!(template.translation_targets[source][0], target as u16);
    assert_eq!(
        template.reverse_translation_sources[target][0],
        source as u16
    );
    template.translation_targets[source][0] = u16::MAX;
    template.reverse_translation_sources[target][0] = u16::MAX;
    assert_eq!(check(&template), Err(StructuralFault::Translation));

    // Remove an actual seed from *both* representations.
    let mut template = compile();
    let seed = usize::from(template.sky_seeds.remove(0));
    template.sky_seed_words[seed / 64] &= !(1_u64 << (seed % 64));
    assert_eq!(check(&template), Err(StructuralFault::Seeds));

    let mut template = compile();
    template.ceiling -= 1;
    assert_eq!(check(&template), Err(StructuralFault::FiniteCeiling));

    let mut template = compile();
    let start = template
        .rotation_target_offsets
        .windows(2)
        .find_map(|pair| {
            let start = pair[0] as usize;
            (pair[1] - pair[0] >= 2
                && template.rotation_targets[start] != template.rotation_targets[start + 1])
                .then_some(start)
        })
        .expect("T has an ordered nontrivial kick slot");
    // The original ordinal array remains increasing, yet the first two
    // destinations no longer correspond to the rule's priority.
    template.rotation_targets.swap(start, start + 1);
    assert_eq!(check(&template), Err(StructuralFault::Rotation));

    let mut template = compile();
    let inverse = template
        .reverse_rotation_sources
        .iter_mut()
        .find(|edge| edge.prior_target_count > 0)
        .expect("T has a subsequent kick");
    inverse.prior_target_count -= 1;
    assert_eq!(check(&template), Err(StructuralFault::InverseRotation));

    // A removed legal inverse edge cannot masquerade as one more E2 removal,
    // even with consistent offsets and the declared removal count updated.
    let mut template = compile();
    let removed = template
        .reverse_rotation_sources
        .iter()
        .position(|edge| {
            edge.prior_target_count == 0
                && template.state_masks[usize::from(edge.source)] != u64::MAX
        })
        .expect("T has a feasible reverse candidate");
    template.reverse_rotation_sources.remove(removed);
    for offset in &mut template.reverse_rotation_offsets {
        if *offset as usize > removed {
            *offset -= 1;
        }
    }
    template.globally_pruned_rotation_sources += 1;
    assert_eq!(check(&template), Err(StructuralFault::InverseRotation));
}
