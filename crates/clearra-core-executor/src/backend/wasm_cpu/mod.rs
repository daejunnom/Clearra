mod build_probability;
mod build_probability_distributed;
mod buildup;
mod catalog;
mod coverage_product;
mod distributed;
mod exact_collections;
mod extended_board;
mod extended_build_probability;
mod extended_buildup;
mod extended_geometry;
mod extended_geometry_component;
mod extended_geometry_dense;
mod extended_geometry_domain;
mod extended_inverse_catalog;
mod extended_reachability;
mod geometry;
mod geometry_apdp;
mod geometry_component;
mod geometry_domain;
mod geometry_family;
#[cfg(all(test, feature = "parallel"))]
mod geometry_parallel_tests;
mod geometry_projection;
mod geometry_separator;
mod kick_profiles;
#[cfg(feature = "parallel")]
mod parallel_coverage;
#[cfg(feature = "parallel")]
mod parallel_search;
#[cfg(feature = "parallel")]
mod parallel_worker;
mod piece_order_language;
mod reachability;
mod realization_feasibility;
mod result;
mod standard_bag_coverage;
#[cfg(feature = "webgpu-search")]
mod webgpu_distributed;
#[cfg(feature = "webgpu-search")]
mod webgpu_search;

pub(crate) use build_probability::{BuildProbabilityAdvance, WasmBuildProbabilitySession};
pub use build_probability_distributed::{
    WasmBuildProbabilityCandidateProducer, WasmBuildProbabilityDistributedResultMerger,
    WasmBuildProbabilityDistributedVerifier,
};
pub use distributed::{
    WasmCandidatePacket, WasmCandidateProducerAdvance, WasmCpuCandidateProducer,
    WasmDistributedBackendExecution, WasmDistributedGeometrySummary, WasmDistributedProgress,
    WasmDistributedResultMerger, WasmDistributedVerifier,
};
pub(crate) use result::{ExactSearchAdvance, WasmExactSearchSession};
#[cfg(feature = "webgpu-search")]
pub use webgpu_distributed::WasmWebGpuCandidateProducer;
#[cfg(feature = "webgpu-search")]
pub(crate) use webgpu_search::WasmWebGpuSearchSession;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WasmExactSearchError {
    InvalidProblem(&'static str),
    Cancelled,
}

const MAX_BOARD64_PIECES: usize = 15;

const fn piece_index(piece: clearra_core_domain::piece::piece_kind::PieceKind) -> usize {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    match piece {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    }
}

fn mix_digest(mut hash: u64, value: u64) -> u64 {
    if hash == 0 {
        hash = 0xcbf2_9ce4_8422_2325;
    }
    for byte in value.to_le_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
