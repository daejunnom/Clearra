use clearra_supply::piece_source::{PieceSource, PieceSourceKind};

pub const C_PIECE_SOURCE_FIXED_QUEUE: u32 = 1;
pub const C_PIECE_SOURCE_BAG_UNIVERSE: u32 = 2;
pub const C_PIECE_SOURCE_OBSERVED_WINDOW: u32 = 3;
pub const C_PIECE_SOURCE_MATERIALIZED_PATTERN_UNIVERSE: u32 = 4;

pub const C_SUPPLY_TRUNCATION_NONE: u16 = 0;
pub const C_SUPPLY_TRUNCATION_OBSERVED_WINDOW_BUDGET_EXCEEDED: u16 = 1;
pub const C_SUPPLY_TRUNCATION_MATERIALIZED_PATTERN_BUDGET_EXCEEDED: u16 = 2;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CPieceSourceDescriptor {
    pub piece_source_id: u64,
    pub source_kind: u32,
    pub provenance_id: u32,
    pub pattern_universe_id: u64,
    pub pattern_weight_model_id: u64,
    pub materialized_pattern_count: u32,
    pub fixed_sequence_len: u16,
    pub piece_set_profile_id: u8,
    pub complete: u8,
    pub truncation_reason: u16,
    pub exact_bag_automaton_supported: u8,
    pub reserved: [u8; 5],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PieceSourceDescriptorError {
    FixedSequenceTooLong { value: usize },
    MaterializedPatternCountTooLarge { value: usize },
}

pub struct PieceSourceDescriptorCompiler;

impl PieceSourceDescriptorCompiler {
    pub fn compile(
        source: &PieceSource,
    ) -> Result<CPieceSourceDescriptor, PieceSourceDescriptorError> {
        let fixed_sequence_len = fixed_sequence_len(source)?;
        let materialized_pattern_count = source
            .materialized_universe()
            .map_or(0, |universe| universe.pattern_count());
        let materialized_pattern_count =
            u32::try_from(materialized_pattern_count).map_err(|_| {
                PieceSourceDescriptorError::MaterializedPatternCountTooLarge {
                    value: materialized_pattern_count,
                }
            })?;

        Ok(CPieceSourceDescriptor {
            piece_source_id: source.id().get(),
            source_kind: source_kind(source.kind()),
            provenance_id: provenance_fingerprint(source.provenance().supply_provenance_id()),
            pattern_universe_id: source.pattern_universe_id().map_or(0, |id| id.get()),
            pattern_weight_model_id: source.pattern_weight_model_id().map_or(0, |id| id.get()),
            materialized_pattern_count,
            fixed_sequence_len,
            piece_set_profile_id: source.piece_set_id().get(),
            complete: u8::from(source.complete()),
            truncation_reason: source
                .truncation_reason()
                .map_or(C_SUPPLY_TRUNCATION_NONE, |reason| reason.as_u16()),
            exact_bag_automaton_supported: u8::from(exact_bag_automaton_supported(source)),
            reserved: [0; 5],
        })
    }
}

fn exact_bag_automaton_supported(source: &PieceSource) -> bool {
    source.kind() == PieceSourceKind::BagUniverse
        && source.bag_universe_descriptor().is_some_and(|bag| {
            bag.pattern() == clearra_core_domain::piece::piece_kind::PieceKind::STANDARD_TETROMINOES
        })
}

pub(super) fn provenance_fingerprint(value: u64) -> u32 {
    let folded = (value ^ (value >> 32)) as u32;
    folded.max(1)
}

fn source_kind(kind: PieceSourceKind) -> u32 {
    match kind {
        PieceSourceKind::FixedQueue => C_PIECE_SOURCE_FIXED_QUEUE,
        PieceSourceKind::BagUniverse => C_PIECE_SOURCE_BAG_UNIVERSE,
        PieceSourceKind::ObservedWindow => C_PIECE_SOURCE_OBSERVED_WINDOW,
        PieceSourceKind::MaterializedPatternUniverse => {
            C_PIECE_SOURCE_MATERIALIZED_PATTERN_UNIVERSE
        }
    }
}

fn fixed_sequence_len(source: &PieceSource) -> Result<u16, PieceSourceDescriptorError> {
    let len = source.fixed_sequence().map_or(0, |sequence| sequence.len());
    u16::try_from(len).map_err(|_| PieceSourceDescriptorError::FixedSequenceTooLong { value: len })
}

#[cfg(test)]
#[path = "piece_source_descriptor_tests.rs"]
mod tests;
