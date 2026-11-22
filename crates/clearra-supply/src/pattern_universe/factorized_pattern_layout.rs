use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::queue::queue_pattern_expression::FactorizedQueuePatternShape;

use super::materialized_pattern_universe::{
    MaterializedPatternUniverse, UniformCompactPatternSource,
};

/// Borrowed authority for the original ordinal layout of a complete, uniform
/// factorized source. Only the actual sequence/weight backing can create this
/// view; a label, universe ID, or equal pattern count cannot do so.
///
/// Prefix projection changes the visible length, not the full ranked layout.
/// Hidden suffix multiplicity therefore remains available to exact products.
/// This view and its segment iterator do not allocate.
#[derive(Clone, Copy, Debug)]
pub struct FactorizedPatternLayout<'a> {
    shape: FactorizedQueuePatternShape<'a>,
}

/// One independent draw-without-replacement segment in the original ranking.
/// Choices retain their stored order; bit masks alone are not ordinal evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FactorizedPatternSegment {
    choices: [PieceKind; 7],
    choice_count: u8,
    choice_mask: u8,
    draw_start: usize,
    draw_count: usize,
    variant_count: usize,
    suffix_variant_count: usize,
}

impl FactorizedPatternSegment {
    pub fn choices(&self) -> &[PieceKind] {
        &self.choices[..usize::from(self.choice_count)]
    }

    pub const fn choice_mask(self) -> u8 {
        self.choice_mask
    }

    pub const fn draw_start(self) -> usize {
        self.draw_start
    }

    pub const fn draw_count(self) -> usize {
        self.draw_count
    }

    pub const fn variant_count(self) -> usize {
        self.variant_count
    }

    /// Product of all later segments' full variant counts, including any
    /// hidden suffix. The last segment has a suffix count of one.
    pub const fn suffix_variant_count(self) -> usize {
        self.suffix_variant_count
    }
}

impl<'a> FactorizedPatternLayout<'a> {
    pub fn pattern_count(self) -> usize {
        self.shape.pattern_count()
    }

    pub fn full_sequence_len(self) -> usize {
        self.shape.full_sequence_len()
    }

    pub fn visible_sequence_len(self) -> usize {
        self.shape.visible_sequence_len()
    }

    pub fn segment_count(self) -> usize {
        self.shape.atoms().len()
    }

    pub fn segments(self) -> impl ExactSizeIterator<Item = FactorizedPatternSegment> + 'a {
        let mut draw_start = 0;
        let mut suffix_variant_count = self.pattern_count();
        self.shape.atoms().map(move |atom| {
            // The constructor checked the full product, nonzero divisors,
            // lengths, and uniqueness without allocating.
            suffix_variant_count /= atom.variant_count();
            let mut choices = [PieceKind::I; 7];
            choices[..atom.choices().len()].copy_from_slice(atom.choices());
            let segment = FactorizedPatternSegment {
                choices,
                choice_count: atom.choices().len() as u8,
                choice_mask: atom
                    .choices()
                    .iter()
                    .fold(0, |mask, &piece| mask | piece_bit(piece)),
                draw_start,
                draw_count: atom.draw_count(),
                variant_count: atom.variant_count(),
                suffix_variant_count,
            };
            draw_start += atom.draw_count();
            segment
        })
    }
}

impl MaterializedPatternUniverse {
    /// Certify the actual complete, unit-mass ranked storage without parsing
    /// source text or expanding any original queue. Consumers requiring every
    /// draw to be visible must additionally compare the full and visible lengths.
    pub fn complete_factorized_layout(&self) -> Option<FactorizedPatternLayout<'_>> {
        if !self.complete()
            || self.truncation_reason().is_some()
            || self.total_possible_pattern_count() != self.pattern_count() as u128
        {
            return None;
        }
        let UniformCompactPatternSource::FactorizedExpression(shape) =
            self.uniform_compact_source()?
        else {
            return None;
        };
        if shape.pattern_count() != self.pattern_count()
            || shape.visible_sequence_len() == 0
            || shape.visible_sequence_len() > shape.full_sequence_len()
            || shape.atoms().len() == 0
        {
            return None;
        }
        let mut pattern_count = 1_usize;
        let mut full_sequence_len = 0_usize;
        for atom in shape.atoms() {
            if atom.choices().is_empty()
                || atom.choices().len() > PieceKind::STANDARD_TETROMINOES.len()
                || atom.draw_count() == 0
                || atom.draw_count() > atom.choices().len()
            {
                return None;
            }
            let mut seen = 0_u8;
            for &piece in atom.choices() {
                let bit = piece_bit(piece);
                if seen & bit != 0 {
                    return None;
                }
                seen |= bit;
            }
            let variants = (0..atom.draw_count()).try_fold(1_usize, |count, used| {
                count.checked_mul(atom.choices().len() - used)
            })?;
            if variants != atom.variant_count() {
                return None;
            }
            pattern_count = pattern_count.checked_mul(variants)?;
            full_sequence_len = full_sequence_len.checked_add(atom.draw_count())?;
        }
        (pattern_count == shape.pattern_count() && full_sequence_len == shape.full_sequence_len())
            .then_some(FactorizedPatternLayout { shape })
    }
}

const fn piece_bit(piece: PieceKind) -> u8 {
    1 << match piece {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::probability::probability_value::ProbabilityValue;
    use clearra_coverage::universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    };

    use crate::{
        pattern_universe::PatternUniverseMaterializer,
        queue::queue_pattern_expression::QueuePatternExpression,
    };

    use super::*;

    fn source(expression: &str, visible: usize) -> MaterializedPatternUniverse {
        let expression = QueuePatternExpression::parse(expression, 0)
            .unwrap()
            .prefix(visible);
        let count = expression.pattern_count();
        assert!(expression.is_factorized());
        MaterializedPatternUniverse::from_factorized_queue_expression(
            PatternUniverseId::new(61),
            PatternWeightModelId::new(62),
            expression,
            ProbabilityValue::new(1.0 / count as f64).unwrap(),
            count as u128,
        )
        .unwrap()
    }

    fn permutations(choices: &[PieceKind], count: usize) -> Vec<Vec<PieceKind>> {
        fn visit(
            choices: &[PieceKind],
            count: usize,
            prefix: &mut Vec<PieceKind>,
            out: &mut Vec<Vec<PieceKind>>,
        ) {
            if prefix.len() == count {
                out.push(prefix.clone());
                return;
            }
            for &piece in choices {
                if !prefix.contains(&piece) {
                    prefix.push(piece);
                    visit(choices, count, prefix, out);
                    prefix.pop();
                }
            }
        }
        let mut out = Vec::new();
        visit(choices, count, &mut Vec::new(), &mut out);
        out
    }

    #[test]
    fn p1p7p7_layout_preserves_original_segment_and_ordinal_boundaries() {
        let universe = source("P1P7P7", 15);
        let layout = universe.complete_factorized_layout().unwrap();
        assert_eq!(layout.pattern_count(), 177_811_200);
        assert_eq!(layout.full_sequence_len(), 15);
        assert_eq!(layout.visible_sequence_len(), 15);
        assert_eq!(layout.segment_count(), 3);
        let segments = layout.segments().collect::<Vec<_>>();
        assert_eq!(
            segments
                .iter()
                .map(|s| (
                    s.draw_start(),
                    s.draw_count(),
                    s.variant_count(),
                    s.suffix_variant_count()
                ))
                .collect::<Vec<_>>(),
            [(0, 1, 7, 25_401_600), (1, 7, 5040, 5040), (8, 7, 5040, 1)]
        );
        assert!(segments
            .iter()
            .all(|s| s.choices() == PieceKind::STANDARD_TETROMINOES));
        assert!(segments.iter().all(|s| s.choice_mask() == 0x7f));
        let bags = permutations(&PieceKind::STANDARD_TETROMINOES, 7);
        for ordinal in [
            0,
            1,
            5039,
            5040,
            25_401_599,
            25_401_600,
            50_803_200,
            177_811_199,
        ] {
            let first = ordinal / 25_401_600;
            let tail = ordinal % 25_401_600;
            let mut expected = vec![PieceKind::STANDARD_TETROMINOES[first]];
            expected.extend_from_slice(&bags[tail / 5040]);
            expected.extend_from_slice(&bags[tail % 5040]);
            assert_eq!(
                universe.sequence_at(ordinal).as_ref(),
                expected,
                "ordinal {ordinal}"
            );
        }
    }

    #[test]
    fn small_segment_products_match_independent_full_permutation_enumeration() {
        for expression in ["P1P4", "P2P3", "[LOI]2P7"] {
            let universe = source(expression, usize::MAX);
            let layout = universe.complete_factorized_layout().unwrap();
            let mut expected = vec![Vec::new()];
            for segment in layout.segments() {
                let variants = permutations(segment.choices(), segment.draw_count());
                assert_eq!(variants.len(), segment.variant_count());
                let mut next = Vec::new();
                for prefix in &expected {
                    for suffix in &variants {
                        let mut sequence = prefix.clone();
                        sequence.extend_from_slice(suffix);
                        next.push(sequence);
                    }
                }
                expected = next;
            }
            assert_eq!(expected.len(), layout.pattern_count());
            for (ordinal, sequence) in expected.iter().enumerate() {
                assert_eq!(
                    universe.sequence_at(ordinal).as_ref(),
                    sequence,
                    "{expression}:{ordinal}"
                );
            }
        }
    }

    #[test]
    fn visible_prefix_retains_full_hidden_suffix_multiplicity() {
        let universe = source("P7", 3);
        let layout = universe.complete_factorized_layout().unwrap();
        assert_eq!(layout.pattern_count(), 5040);
        assert_eq!(layout.full_sequence_len(), 7);
        assert_eq!(layout.visible_sequence_len(), 3);
        let segment = layout.segments().next().unwrap();
        assert_eq!((segment.draw_count(), segment.variant_count()), (7, 5040));
        for ordinal in 0..24 {
            assert_eq!(universe.sequence_at(ordinal), universe.sequence_at(0));
        }
        assert_ne!(universe.sequence_at(24), universe.sequence_at(0));
    }

    #[test]
    fn layout_requires_actual_complete_uniform_factorized_storage() {
        let universe = source("P7", 7);
        let explicit = MaterializedPatternUniverse::from_sequences(
            universe.pattern_universe_id(),
            universe.pattern_weight_model_id(),
            (0..universe.pattern_count())
                .map(|id| universe.sequence_at(id).into_owned())
                .collect(),
            vec![ProbabilityValue::new(1.0 / 5040.0).unwrap(); 5040],
            5040,
            true,
            None,
        )
        .unwrap();
        assert!(explicit.complete_factorized_layout().is_none());
        assert!(PatternUniverseMaterializer::standard_7_bag(7, 0, 61)
            .unwrap()
            .complete_factorized_layout()
            .is_none());
        for (weight, total) in [(1.0 / 10080.0, 5040), (1.0 / 5040.0, 5041)] {
            let foreign = MaterializedPatternUniverse::from_factorized_queue_expression(
                universe.pattern_universe_id(),
                universe.pattern_weight_model_id(),
                QueuePatternExpression::parse("P7", 0).unwrap(),
                ProbabilityValue::new(weight).unwrap(),
                total,
            )
            .unwrap();
            assert!(foreign.complete_factorized_layout().is_none());
        }
    }
}
