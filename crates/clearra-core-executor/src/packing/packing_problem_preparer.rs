use clearra_core_domain::objective::objective_kind::ObjectiveKind;
use clearra_core_ffi::{CPackingProblem, CPackingProblemBuilder, FfiProblemError};
use clearra_pc_graph::request::RequestedSearchBackend;
use clearra_problem::{SearchProblem, SearchProblemPreset};
use clearra_supply::{PackingMultisetFamily, PieceMultisetKey};

use crate::backend::{
    BackendSelectionError, PcBackendSelection, PcBackendSelectionContext, PcBackendSelector,
    SearchBackendCapabilityProvider,
};

pub(crate) struct PreparedPackingProblem {
    compact_problem: CPackingProblem,
    backend_selection: PcBackendSelection,
}

impl PreparedPackingProblem {
    pub(crate) fn compact_problem(&self) -> CPackingProblem {
        self.compact_problem
    }
}
impl PreparedPackingProblem {
    pub(crate) fn backend_selection(self) -> PcBackendSelection {
        self.backend_selection
    }
}

pub(crate) fn prepare_packing_problem_for_multiset_with_provider(
    problem: &SearchProblem,
    piece_multiset: PieceMultisetKey,
    provider: &impl SearchBackendCapabilityProvider,
) -> Result<PreparedPackingProblem, PackingProblemPrepareError> {
    prepare_packing_problem(problem, Some(piece_multiset), provider)
}

pub(crate) fn prepare_packing_problem_for_multiset_family_with_provider(
    problem: &SearchProblem,
    family: &PackingMultisetFamily,
    provider: &impl SearchBackendCapabilityProvider,
) -> Result<PreparedPackingProblem, PackingProblemPrepareError> {
    prepare_with_builder(
        problem,
        backend_context(problem).with_multiset_group_count(family.len()),
        provider,
        || CPackingProblemBuilder::from_search_problem_with_piece_multiset_family(problem, family),
    )
}

fn prepare_packing_problem(
    problem: &SearchProblem,
    piece_multiset: Option<PieceMultisetKey>,
    provider: &impl SearchBackendCapabilityProvider,
) -> Result<PreparedPackingProblem, PackingProblemPrepareError> {
    prepare_with_builder(
        problem,
        backend_context(problem),
        provider,
        || match piece_multiset {
            Some(piece_multiset) => {
                CPackingProblemBuilder::from_search_problem_with_piece_multiset(
                    problem,
                    piece_multiset,
                )
            }
            None => CPackingProblemBuilder::from_search_problem(problem),
        },
    )
}

fn prepare_with_builder(
    problem: &SearchProblem,
    context: PcBackendSelectionContext,
    provider: &impl SearchBackendCapabilityProvider,
    build_compact: impl FnOnce() -> Result<CPackingProblem, FfiProblemError>,
) -> Result<PreparedPackingProblem, PackingProblemPrepareError> {
    let (compact_problem, backend_selection) = if matches!(
        problem.backend_request().requested_backend(),
        RequestedSearchBackend::Gpu
    ) {
        std::thread::scope(|scope| {
            let selection = scope.spawn(|| {
                PcBackendSelector::select_with_context_and_provider(
                    problem.backend_request(),
                    context,
                    provider,
                )
            });
            let compact_problem = build_compact();
            let backend_selection = match selection.join() {
                Ok(selection) => selection,
                Err(panic) => std::panic::resume_unwind(panic),
            };
            (compact_problem, backend_selection)
        })
    } else {
        (
            build_compact(),
            PcBackendSelector::select_with_context_and_provider(
                problem.backend_request(),
                context,
                provider,
            ),
        )
    };

    let compact_problem = compact_problem.map_err(PackingProblemPrepareError::Ffi)?;
    let backend_selection = backend_selection.map_err(PackingProblemPrepareError::Backend)?;

    Ok(PreparedPackingProblem {
        compact_problem,
        backend_selection,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PackingProblemPrepareError {
    Ffi(FfiProblemError),
    Backend(BackendSelectionError),
}

pub(crate) fn backend_context(problem: &SearchProblem) -> PcBackendSelectionContext {
    match problem.preset() {
        SearchProblemPreset::OpeningPc => PcBackendSelectionContext::opening(
            matches!(
                problem.objective().kind(),
                ObjectiveKind::Unique | ObjectiveKind::Tiling
            ),
            problem.piece_window().max_pieces(),
        ),
        SearchProblemPreset::ScenarioPc => PcBackendSelectionContext::scenario(
            problem.count_policy(),
            problem.piece_window().max_pieces(),
        ),
        SearchProblemPreset::Setup | SearchProblemPreset::Build => {
            PcBackendSelectionContext::Unknown
        }
    }
}
