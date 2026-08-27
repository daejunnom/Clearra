use clearra_app::{
    AppCommand, AppRequest, BuildProbabilityAppCommand, DamageAppCommand,
    FieldDocumentTransformAppCommand, FieldDocumentTransformKind, FumenAppCommand,
    OperationDocumentProblem, OperationSequenceAppCommand, ParityAppCommand, PcAppCommand,
    PcChanceIngressOrigin, PcFailedQueueIngressOrigin, PcMinimalsIngressOrigin,
    PcPathIngressOrigin, PcResultProjection, PcSaveIngressOrigin, PcScoreIngressOrigin,
    PcScoreMinimalsIngressOrigin, PcTilingIngressOrigin, PercentAppCommand,
    ProductCapabilityContract, RenAppCommand, RenderAppCommand, RequestStructuralProfiles,
    ResourceBudget, ScenarioAppCommand, SequenceDependenciesAppCommand, SetupAppCommand,
    SpinFinderAppCommand, SpinStructureAppCommand, SpinStructureProductMode, VerifyAppCommand,
    PC_SCORE_MAX_PATTERNS, PC_SCORE_MAX_PATTERN_BYTES, PC_SCORE_MAX_SOURCE_PIECES,
};
use clearra_core_domain::pc::pc_target::PcTarget;
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_forward_search::{ForwardSearchMode, ForwardSearchQuery};
use clearra_objectives::policy::{
    objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
};
use clearra_pc_graph::request::{
    GpuDeviceSelection, OpeningPcSearchQuery, PcCountPolicy, PcExecutionPolicy, PcHoldPolicy,
    PcQueueInput, PcScenarioQuery, PcSolutionProbabilityPolicy, RequestedSearchBackend,
    SupplyWindowSize, WorkerPolicy,
};
use clearra_problem::{
    BuildSolutionProbabilityPolicy, SetupCandidatePriority, SetupCycleResetBorrowPolicy,
    SetupLengthPreference, SetupPathDetail, SetupSearchMode, SetupSearchQuery,
};
use clearra_rules::profile::{builtin_rules::srs_plus, rule_profile::RuleProfile};
use clearra_spin_structure_search::SpinStructureQuery;
use clearra_supply::{
    queue::{queue_parser, queue_pattern_expression::QueuePatternExpression},
    QueueObservationPolicy,
};

use crate::{WebBuildProbabilityInput, WebBuildV2Input, WebPcScenarioInput, WebSetupScoreInput};
use crate::{WebCommandError, WebCommandErrorCode, WebVirtualFileHandle};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebCommandRequest {
    command_kind: String,
    lines: u8,
    backend: RequestedSearchBackend,
    rule: RuleProfile,
    gpu_device: GpuDeviceSelection,
    allow_backend_fallback: bool,
    queue: Option<String>,
    patterns: Option<String>,
    hold_enabled: bool,
    supply_window_size: Option<SupplyWindowSize>,
    count_policy: PcCountPolicy,
    objective: ObjectivePolicy,
    pc_result_projection: PcResultProjection,
    pc_failed_queue_origin: Option<PcFailedQueueIngressOrigin>,
    product_capability_contract: Option<ProductCapabilityContract>,
    queue_observation_policy: QueueObservationPolicy,
    scenario: Option<WebPcScenarioInput>,
    build_probability: Option<WebBuildProbabilityInput>,
    build_v2: Option<WebBuildV2Input>,
    setup_score: Option<WebSetupScoreInput>,
    forward_search: Option<ForwardSearchQuery>,
    spin_structure: Option<SpinStructureQuery>,
    spin_structure_product_mode: SpinStructureProductMode,
    percent_query: Option<PcScenarioQuery>,
    percent_failed_pattern_limit: usize,
    setup_remaining: Option<Vec<PieceKind>>,
    setup_queue_based_pieces: Option<Vec<PieceKind>>,
    setup_next_cycle_remaining_pieces: Option<Vec<PieceKind>>,
    setup_allow_post_cycle_borrow: bool,
    setup_candidate_priority: SetupCandidatePriority,
    setup_length_preference: SetupLengthPreference,
    setup_max_pieces: u8,
    setup_search_mode: SetupSearchMode,
    setup_path_detail: Option<SetupPathDetail>,
    max_patterns: Option<usize>,
    max_nodes: Option<usize>,
    max_frontier_states: Option<usize>,
    max_candidates: Option<usize>,
    max_memory_mib: Option<u64>,
    workers: Option<usize>,
    automatic_worker_limit: Option<usize>,
    worker_hardware_limit: usize,
    runtime_webgpu_available: bool,
    use_all_logical_processors: bool,
    cpu_warmup: bool,
    gpu_warmup: bool,
    tablebase_requested: bool,
    precompute_build_dependencies: bool,
    solution_probabilities: bool,
    verify_scope: Option<String>,
    operation_sequence: Option<OperationDocumentProblem>,
    sequence_dependencies: Option<OperationDocumentProblem>,
    parity: Option<ParityAppCommand>,
    fumen: Option<FumenAppCommand>,
    render: Option<RenderAppCommand>,
    field_document_transform: Option<FieldDocumentTransformAppCommand>,
    request_structural_profiles: RequestStructuralProfiles,
    virtual_files: Vec<WebVirtualFileHandle>,
}

impl WebCommandRequest {
    pub fn pc(lines: u8, backend: RequestedSearchBackend) -> Self {
        Self {
            command_kind: "pc".to_owned(),
            lines,
            backend,
            rule: srs_plus(),
            gpu_device: GpuDeviceSelection::Auto,
            allow_backend_fallback: matches!(backend, RequestedSearchBackend::Auto),
            queue: None,
            patterns: None,
            hold_enabled: true,
            supply_window_size: None,
            count_policy: PcCountPolicy::CountUnique,
            objective: ObjectivePolicy::unique(),
            pc_result_projection: PcResultProjection::Standard,
            pc_failed_queue_origin: None,
            product_capability_contract: None,
            queue_observation_policy: QueueObservationPolicy::default(),
            scenario: None,
            build_probability: None,
            build_v2: None,
            setup_score: None,
            forward_search: None,
            spin_structure: None,
            spin_structure_product_mode: SpinStructureProductMode::Search,
            percent_query: None,
            percent_failed_pattern_limit: 100,
            setup_remaining: None,
            setup_queue_based_pieces: None,
            setup_next_cycle_remaining_pieces: None,
            setup_allow_post_cycle_borrow: false,
            setup_candidate_priority: SetupCandidatePriority::default(),
            setup_length_preference: SetupLengthPreference::default(),
            setup_max_pieces: 9,
            setup_search_mode: SetupSearchMode::default(),
            setup_path_detail: None,
            max_patterns: None,
            max_nodes: None,
            max_frontier_states: None,
            max_candidates: None,
            max_memory_mib: None,
            workers: None,
            automatic_worker_limit: None,
            worker_hardware_limit: clearra_pc_graph::request::WorkerPolicy::hardware_worker_limit(),
            runtime_webgpu_available: true,
            use_all_logical_processors: false,
            cpu_warmup: false,
            gpu_warmup: false,
            tablebase_requested: false,
            precompute_build_dependencies: false,
            solution_probabilities: false,
            verify_scope: None,
            operation_sequence: None,
            sequence_dependencies: None,
            parity: None,
            fumen: None,
            render: None,
            field_document_transform: None,
            request_structural_profiles: RequestStructuralProfiles::STANDARD,
            virtual_files: Vec::new(),
        }
    }
}
impl WebCommandRequest {
    pub fn verify(scope: Option<String>) -> Self {
        Self {
            command_kind: "verify".to_owned(),
            lines: 0,
            backend: RequestedSearchBackend::Cpu,
            rule: srs_plus(),
            gpu_device: GpuDeviceSelection::Auto,
            allow_backend_fallback: false,
            queue: None,
            patterns: None,
            hold_enabled: true,
            supply_window_size: None,
            count_policy: PcCountPolicy::CountUnique,
            objective: ObjectivePolicy::unique(),
            pc_result_projection: PcResultProjection::Standard,
            pc_failed_queue_origin: None,
            product_capability_contract: None,
            queue_observation_policy: QueueObservationPolicy::default(),
            scenario: None,
            build_probability: None,
            build_v2: None,
            setup_score: None,
            forward_search: None,
            spin_structure: None,
            spin_structure_product_mode: SpinStructureProductMode::Search,
            percent_query: None,
            percent_failed_pattern_limit: 100,
            setup_remaining: None,
            setup_queue_based_pieces: None,
            setup_next_cycle_remaining_pieces: None,
            setup_allow_post_cycle_borrow: false,
            setup_candidate_priority: SetupCandidatePriority::default(),
            setup_length_preference: SetupLengthPreference::default(),
            setup_max_pieces: 9,
            setup_search_mode: SetupSearchMode::default(),
            setup_path_detail: None,
            max_patterns: None,
            max_nodes: None,
            max_frontier_states: None,
            max_candidates: None,
            max_memory_mib: None,
            workers: None,
            automatic_worker_limit: None,
            worker_hardware_limit: clearra_pc_graph::request::WorkerPolicy::hardware_worker_limit(),
            runtime_webgpu_available: true,
            use_all_logical_processors: false,
            cpu_warmup: false,
            gpu_warmup: false,
            tablebase_requested: false,
            precompute_build_dependencies: false,
            solution_probabilities: false,
            verify_scope: scope,
            operation_sequence: None,
            sequence_dependencies: None,
            parity: None,
            fumen: None,
            render: None,
            field_document_transform: None,
            request_structural_profiles: RequestStructuralProfiles::STANDARD,
            virtual_files: Vec::new(),
        }
    }
}
impl WebCommandRequest {
    pub fn operation_sequence(problem: OperationDocumentProblem) -> Self {
        let mut request = Self::pc(0, RequestedSearchBackend::Cpu);
        request.command_kind = "utility-sequence".to_owned();
        request.hold_enabled = false;
        request.operation_sequence = Some(problem);
        request
    }

    pub fn sequence_dependencies(problem: OperationDocumentProblem) -> Self {
        let mut request = Self::pc(0, RequestedSearchBackend::Cpu);
        request.command_kind = "utility-sequence-dependencies".to_owned();
        request.hold_enabled = false;
        request.sequence_dependencies = Some(problem);
        request
    }

    pub fn parity(command: ParityAppCommand) -> Self {
        let mut request = Self::pc(0, RequestedSearchBackend::Cpu);
        request.command_kind = "utility-parity".to_owned();
        request.hold_enabled = false;
        request.parity = Some(command);
        request
    }

    pub fn fumen(command: FumenAppCommand) -> Self {
        let mut request = Self::pc(0, RequestedSearchBackend::Cpu);
        request.command_kind = "utility-fumen".to_owned();
        request.hold_enabled = false;
        request.fumen = Some(command);
        request
    }

    pub fn render(command: RenderAppCommand) -> Self {
        let mut request = Self::pc(0, RequestedSearchBackend::Cpu);
        request.command_kind = "utility-render".to_owned();
        request.hold_enabled = false;
        request.render = Some(command);
        request
    }

    pub fn field_document_transform(command: FieldDocumentTransformAppCommand) -> Self {
        let mut request = Self::pc(0, RequestedSearchBackend::Cpu);
        request.command_kind = match command.transform() {
            FieldDocumentTransformKind::ToGray => "utility-to-gray",
            FieldDocumentTransformKind::Mirror => "utility-mirror",
        }
        .to_owned();
        request.hold_enabled = false;
        request.field_document_transform = Some(command);
        request
    }

    pub fn percent(query: PcScenarioQuery, failed_pattern_limit: usize) -> Self {
        let mut request = Self::pc(0, RequestedSearchBackend::Cpu);
        request.command_kind = "percent".to_owned();
        request.allow_backend_fallback = false;
        request.percent_query = Some(query);
        request.percent_failed_pattern_limit = failed_pattern_limit;
        request
    }

    pub(crate) fn with_failed_queue_mode(mut self, failed_pattern_limit: usize) -> Self {
        self.command_kind = "failed-queue".to_owned();
        self.percent_failed_pattern_limit = failed_pattern_limit;
        self
    }

    pub fn setup(remaining: Vec<PieceKind>, allow_post_cycle_borrow: bool) -> Self {
        let mut request = Self::pc(0, RequestedSearchBackend::Cpu);
        request.command_kind = "setup".to_owned();
        request.allow_backend_fallback = false;
        request.setup_remaining = Some(remaining);
        request.setup_allow_post_cycle_borrow = allow_post_cycle_borrow;
        request
    }

    pub fn with_setup_candidate_priority(mut self, priority: SetupCandidatePriority) -> Self {
        self.setup_candidate_priority = priority;
        self
    }

    pub fn with_setup_length_preference(mut self, preference: SetupLengthPreference) -> Self {
        self.setup_length_preference = preference;
        self
    }

    pub fn with_setup_max_pieces(mut self, max_pieces: u8) -> Self {
        self.setup_max_pieces = max_pieces;
        self
    }

    pub fn with_setup_search_mode(mut self, mode: SetupSearchMode) -> Self {
        self.setup_search_mode = mode;
        self
    }

    pub fn with_setup_queue_based_pieces(mut self, pieces: Vec<PieceKind>) -> Self {
        self.setup_queue_based_pieces = Some(pieces);
        self.setup_search_mode = SetupSearchMode::QueueBased;
        self
    }

    pub fn with_setup_next_cycle_remaining_pieces(mut self, pieces: Vec<PieceKind>) -> Self {
        self.setup_next_cycle_remaining_pieces = Some(pieces);
        self
    }

    pub fn with_setup_path_detail(mut self, detail: SetupPathDetail) -> Self {
        self.setup_path_detail = Some(detail);
        self
    }
}
impl WebCommandRequest {
    pub fn build_probability(input: WebBuildProbabilityInput) -> Self {
        let mut request = Self::pc(0, RequestedSearchBackend::Cpu);
        request.command_kind = "build-probability".to_owned();
        request.scenario = None;
        request.build_probability = Some(input);
        request.allow_backend_fallback = false;
        request
    }

    pub fn build_v2(input: WebBuildV2Input) -> Self {
        let mut request = Self::pc(0, RequestedSearchBackend::Cpu);
        request.command_kind = "build-v2".to_owned();
        request.scenario = None;
        request.build_probability = None;
        request.build_v2 = Some(input);
        request.allow_backend_fallback = false;
        request.runtime_webgpu_available = false;
        request
    }

    pub fn setup_score(input: WebSetupScoreInput) -> Self {
        let mut request = Self::pc(0, RequestedSearchBackend::Cpu);
        request.command_kind = "setup-score".to_owned();
        request.scenario = None;
        request.hold_enabled = input.setup_hold_enabled();
        request.setup_score = Some(input);
        request.allow_backend_fallback = false;
        request.runtime_webgpu_available = false;
        request
    }

    pub fn with_finesse_score(mut self, score: clearra_problem::FinesseScoreRequest) -> Self {
        if let Some(input) = self.build_probability.take() {
            self.build_probability = Some(input.with_finesse_score(score));
        }
        self
    }
}
impl WebCommandRequest {
    pub fn forward(command_kind: &str, query: ForwardSearchQuery) -> Self {
        let mut request = Self::pc(0, RequestedSearchBackend::Cpu);
        request.command_kind = command_kind.to_owned();
        request.allow_backend_fallback = false;
        request.forward_search = Some(query);
        request
    }
}
impl WebCommandRequest {
    pub fn spin_structure(query: SpinStructureQuery) -> Self {
        let mut request = Self::pc(0, RequestedSearchBackend::Cpu);
        request.command_kind = "spin-structure".to_owned();
        request.allow_backend_fallback = false;
        request.spin_structure = Some(query);
        request
    }

    pub fn with_spin_structure_product_mode(
        mut self,
        product_mode: SpinStructureProductMode,
    ) -> Self {
        self.spin_structure_product_mode = product_mode;
        self
    }
}
impl WebCommandRequest {
    pub fn with_backend(mut self, backend: RequestedSearchBackend) -> Self {
        self.backend = backend;
        self
    }

    pub fn with_gpu_device(mut self, gpu_device: GpuDeviceSelection) -> Self {
        self.gpu_device = gpu_device;
        self
    }

    pub fn with_rule(mut self, rule: RuleProfile) -> Self {
        self.rule = rule;
        self
    }

    pub fn with_request_structural_profiles(mut self, profiles: RequestStructuralProfiles) -> Self {
        self.request_structural_profiles = profiles;
        self
    }

    pub fn with_allow_backend_fallback(mut self, allow_backend_fallback: bool) -> Self {
        self.allow_backend_fallback = allow_backend_fallback;
        self
    }
}
impl WebCommandRequest {
    pub fn with_scenario(mut self, scenario: WebPcScenarioInput) -> Self {
        self.scenario = Some(scenario);
        self
    }

    pub fn with_build_probability(mut self, input: WebBuildProbabilityInput) -> Self {
        self.build_probability = Some(input);
        self
    }

    pub fn with_max_patterns(mut self, max_patterns: usize) -> Self {
        self.max_patterns = Some(max_patterns);
        self
    }

    pub fn with_max_nodes(mut self, max_nodes: usize) -> Self {
        self.max_nodes = Some(max_nodes);
        self
    }

    pub fn with_max_frontier_states(mut self, max_frontier_states: usize) -> Self {
        self.max_frontier_states = Some(max_frontier_states);
        self
    }

    pub fn with_max_candidates(mut self, max_candidates: usize) -> Self {
        self.max_candidates = Some(max_candidates);
        self
    }

    pub fn with_max_memory_mib(mut self, max_memory_mib: u64) -> Self {
        self.max_memory_mib = Some(max_memory_mib);
        self
    }

    pub fn with_workers(mut self, workers: usize) -> Self {
        self.workers = Some(workers);
        self.automatic_worker_limit = None;
        self
    }

    pub fn with_automatic_worker_limit(mut self, workers: usize) -> Self {
        self.workers = None;
        self.automatic_worker_limit = Some(workers.max(1));
        self
    }

    pub fn with_worker_hardware_limit(mut self, workers: usize) -> Self {
        self.worker_hardware_limit = workers.max(1);
        self
    }

    pub fn with_runtime_webgpu_available(mut self, available: bool) -> Self {
        self.runtime_webgpu_available = available;
        self
    }

    pub fn with_use_all_logical_processors(mut self, value: bool) -> Self {
        self.use_all_logical_processors = value;
        self
    }

    pub fn with_cpu_warmup(mut self, value: bool) -> Self {
        self.cpu_warmup = value;
        self
    }

    pub fn with_gpu_warmup(mut self, value: bool) -> Self {
        self.gpu_warmup = value;
        self
    }

    pub fn with_tablebase_requested(mut self, value: bool) -> Self {
        self.tablebase_requested = value;
        self
    }

    pub fn with_precompute_build_dependencies(mut self, value: bool) -> Self {
        self.precompute_build_dependencies = value;
        self
    }

    pub fn with_solution_probabilities(mut self, value: bool) -> Self {
        self.solution_probabilities = value;
        self
    }
}
impl WebCommandRequest {
    pub fn with_queue(mut self, queue: impl Into<String>) -> Self {
        self.queue = Some(queue.into());
        self
    }

    pub fn with_patterns(mut self, patterns: impl Into<String>) -> Self {
        self.patterns = Some(patterns.into());
        self
    }

    pub fn with_hold_enabled(mut self, hold_enabled: bool) -> Self {
        self.hold_enabled = hold_enabled;
        self
    }

    pub fn with_source_piece_count(mut self, source_piece_count: usize) -> Self {
        self.supply_window_size = Some(SupplyWindowSize::new(source_piece_count));
        self
    }

    pub fn with_count_policy(mut self, count_policy: PcCountPolicy) -> Self {
        self.count_policy = count_policy;
        if let Some(scenario) = self.scenario.take() {
            self.scenario = Some(scenario.with_count_policy(count_policy));
        }
        self
    }

    pub fn with_objective(mut self, objective: ObjectivePolicy) -> Self {
        self.objective = objective;
        if objective.score().requested() {
            self.count_policy = PcCountPolicy::CountAll;
        }
        self
    }

    pub fn with_pc_result_projection(mut self, projection: PcResultProjection) -> Self {
        self.pc_result_projection = projection;
        self
    }

    /// Atomically binds a public All-Spin mode to its typed capability
    /// identity. Supplying only `with_pc_result_projection` remains possible
    /// for compatibility, but lowering rejects every non-standard projection
    /// that was not bound through this method.
    pub fn with_pc_allspin_product_capability(
        mut self,
        contract: ProductCapabilityContract,
        profile: SpinProfileSelection,
    ) -> Self {
        self.pc_result_projection = match contract {
            ProductCapabilityContract::PcAllSpinSolution => {
                PcResultProjection::AllSpinSolution(profile)
            }
            ProductCapabilityContract::PcAllSpinPreservationChance => {
                PcResultProjection::AllSpinPreservationChance(profile)
            }
            ProductCapabilityContract::PcChance => PcResultProjection::Standard,
            ProductCapabilityContract::PcMinimals => PcResultProjection::Standard,
            ProductCapabilityContract::PcFailedQueue => PcResultProjection::Standard,
            ProductCapabilityContract::PcScore => PcResultProjection::Standard,
            ProductCapabilityContract::PcScoreFinder => PcResultProjection::Standard,
            ProductCapabilityContract::PcPath => PcResultProjection::Standard,
            ProductCapabilityContract::PcScoreMinimals => PcResultProjection::Standard,
            ProductCapabilityContract::PcTiling => PcResultProjection::Standard,
            ProductCapabilityContract::PcSaves | ProductCapabilityContract::PcBestSave => {
                PcResultProjection::Standard
            }
            ProductCapabilityContract::BuildCover | ProductCapabilityContract::BuildSetup => {
                PcResultProjection::Standard
            }
        };
        self.product_capability_contract = Some(contract);
        self
    }

    pub fn with_pc_chance_product_capability(mut self, origin: PcChanceIngressOrigin) -> Self {
        self.pc_result_projection = PcResultProjection::ChanceProbabilityV2(origin);
        self.product_capability_contract = Some(ProductCapabilityContract::PcChance);
        self
    }

    pub fn with_pc_minimals_product_capability(mut self, origin: PcMinimalsIngressOrigin) -> Self {
        self.pc_result_projection = PcResultProjection::MinimumCoverV2(origin);
        self.product_capability_contract = Some(ProductCapabilityContract::PcMinimals);
        self
    }

    pub fn with_pc_path_product_capability(mut self, origin: PcPathIngressOrigin) -> Self {
        self.pc_result_projection = PcResultProjection::PathFamilyV2(origin);
        self.product_capability_contract = Some(ProductCapabilityContract::PcPath);
        self
    }

    pub fn with_pc_score_product_capability(mut self, origin: PcScoreIngressOrigin) -> Self {
        self.pc_result_projection = PcResultProjection::ScoreSummaryV2(origin);
        self.product_capability_contract = Some(if origin.is_score_finder() {
            ProductCapabilityContract::PcScoreFinder
        } else {
            ProductCapabilityContract::PcScore
        });
        self
    }

    pub fn with_pc_score_minimals_product_capability(
        mut self,
        origin: PcScoreMinimalsIngressOrigin,
    ) -> Self {
        self.pc_result_projection = PcResultProjection::ScorePortfolioV2(origin);
        self.product_capability_contract = Some(ProductCapabilityContract::PcScoreMinimals);
        self
    }

    pub fn with_pc_tiling_product_capability(mut self, origin: PcTilingIngressOrigin) -> Self {
        self.pc_result_projection = PcResultProjection::TilingFamilyV1(origin);
        self.product_capability_contract = Some(ProductCapabilityContract::PcTiling);
        self
    }

    pub fn with_pc_save_product_capability(mut self, origin: PcSaveIngressOrigin) -> Self {
        let (projection, contract) = match origin.mode() {
            clearra_app::PcSaveResultMode::SaveGroups => (
                PcResultProjection::SaveGroupsV2(origin),
                ProductCapabilityContract::PcSaves,
            ),
            clearra_app::PcSaveResultMode::BestSave => (
                PcResultProjection::BestSaveV2(origin),
                ProductCapabilityContract::PcBestSave,
            ),
        };
        self.pc_result_projection = projection;
        self.product_capability_contract = Some(contract);
        self
    }

    pub fn with_pc_failed_queue_product_capability(
        mut self,
        origin: PcFailedQueueIngressOrigin,
    ) -> Self {
        self.command_kind = "failed-queue".to_owned();
        self.pc_result_projection = PcResultProjection::Standard;
        self.pc_failed_queue_origin = Some(origin);
        self.product_capability_contract = Some(ProductCapabilityContract::PcFailedQueue);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_product_capability_contract_for_test(
        mut self,
        contract: ProductCapabilityContract,
    ) -> Self {
        self.product_capability_contract = Some(contract);
        self
    }

    pub fn with_queue_observation_policy(mut self, policy: QueueObservationPolicy) -> Self {
        self.queue_observation_policy = policy;
        self
    }
}
impl WebCommandRequest {
    pub fn with_virtual_file(mut self, file: WebVirtualFileHandle) -> Self {
        self.virtual_files.push(file);
        self
    }
}
impl WebCommandRequest {
    fn resolved_worker_budget(&self) -> usize {
        let hardware_limit = self.worker_hardware_limit.max(1);
        let workers = match self.workers {
            Some(workers) => WorkerPolicy::clamp_requested_for_hardware(
                workers,
                self.use_all_logical_processors,
                hardware_limit,
            ),
            None => WorkerPolicy::Auto
                .effective_for_hardware_limit(self.use_all_logical_processors, hardware_limit),
        };
        self.automatic_worker_limit
            .map_or(workers, |limit| workers.min(limit.max(1)))
            .max(1)
    }

    fn validate_typed_pc_state(&self) -> Result<(), WebCommandError> {
        let invalid =
            |reason: &'static str| WebCommandError::new(WebCommandErrorCode::InvalidValue, reason);
        match (
            self.pc_failed_queue_origin,
            self.pc_result_projection,
            self.product_capability_contract,
        ) {
            (
                Some(_),
                PcResultProjection::Standard,
                Some(ProductCapabilityContract::PcFailedQueue),
            ) if self.command_kind == "failed-queue" => return Ok(()),
            (Some(_), _, _) => {
                return Err(invalid(
                    "typed failed-queue origin requires the failed-queue command and matching product capability contract",
                ))
            }
            (None, _, Some(ProductCapabilityContract::PcFailedQueue)) => {
                return Err(invalid(
                    "pc.failed-queue product capability requires a closed failed-queue origin",
                ))
            }
            (None, _, _) => {}
        }
        match (self.pc_result_projection, self.product_capability_contract) {
            (PcResultProjection::Standard, None) => return Ok(()),
            (PcResultProjection::Standard, Some(_)) => {
                return Err(invalid(
                    "standard PC projection cannot carry a product capability contract",
                ))
            }
            (PcResultProjection::ChanceProbabilityV2(_), None)
            | (PcResultProjection::MinimumCoverV2(_), None)
            | (PcResultProjection::PathFamilyV2(_), None)
            | (PcResultProjection::ScoreSummaryV2(_), None)
            | (PcResultProjection::ScorePortfolioV2(_), None)
            | (PcResultProjection::TilingFamilyV1(_), None)
            | (PcResultProjection::SaveGroupsV2(_), None)
            | (PcResultProjection::BestSaveV2(_), None)
            | (PcResultProjection::AllSpinSolution(_), None)
            | (PcResultProjection::AllSpinPreservationChance(_), None) => {
                return Err(invalid(
                    "typed PC projection requires its matching product capability contract",
                ))
            }
            (PcResultProjection::TilingFamilyV1(_), Some(ProductCapabilityContract::PcTiling))
            | (PcResultProjection::PathFamilyV2(_), Some(ProductCapabilityContract::PcPath))
            | (PcResultProjection::SaveGroupsV2(_), Some(ProductCapabilityContract::PcSaves))
            | (PcResultProjection::BestSaveV2(_), Some(ProductCapabilityContract::PcBestSave))
            | (
                PcResultProjection::MinimumCoverV2(_),
                Some(ProductCapabilityContract::PcMinimals),
            )
            | (
                PcResultProjection::ChanceProbabilityV2(_),
                Some(ProductCapabilityContract::PcChance),
            )
            | (
                PcResultProjection::ScoreSummaryV2(
                    PcScoreIngressOrigin::CanonicalPcScore
                    | PcScoreIngressOrigin::CompatibilityScore,
                ),
                Some(ProductCapabilityContract::PcScore),
            )
            | (
                PcResultProjection::ScoreSummaryV2(PcScoreIngressOrigin::CanonicalPcScoreFinder),
                Some(ProductCapabilityContract::PcScoreFinder),
            )
            | (
                PcResultProjection::ScorePortfolioV2(_),
                Some(ProductCapabilityContract::PcScoreMinimals),
            )
            | (
                PcResultProjection::AllSpinSolution(_),
                Some(ProductCapabilityContract::PcAllSpinSolution),
            )
            | (
                PcResultProjection::AllSpinPreservationChance(_),
                Some(ProductCapabilityContract::PcAllSpinPreservationChance),
            ) => {}
            _ => {
                return Err(invalid(
                    "typed PC projection and product capability contract do not match",
                ))
            }
        }
        if self.command_kind != "pc" {
            return Err(invalid(
                "typed PC projection is only valid for a PC search request",
            ));
        }
        if self
            .scenario
            .as_ref()
            .is_some_and(|scenario| u16::from(self.lines) != scenario.visible_height())
        {
            return Err(invalid(
                "typed PC scenario lines must equal the initial-field height",
            ));
        }
        if self
            .scenario
            .as_ref()
            .is_some_and(|scenario| self.hold_enabled != scenario.allow_hold())
        {
            return Err(invalid(
                "typed PC scenario hold policy must have one consistent value",
            ));
        }
        match self.pc_result_projection {
            PcResultProjection::Standard => unreachable!("standard projection returned above"),
            PcResultProjection::PathFamilyV2(_) => {
                if self.objective != ObjectivePolicy::all()
                    || self.count_policy != PcCountPolicy::CountAll
                {
                    return Err(invalid("pc path requires objective all and count all"));
                }
                if self.solution_probabilities
                    || self.tablebase_requested
                    || self.precompute_build_dependencies
                {
                    return Err(invalid(
                        "pc path does not accept probability, tablebase, or build-dependency semantics",
                    ));
                }
                if self
                    .scenario
                    .as_ref()
                    .is_some_and(WebPcScenarioInput::has_allowed_colored_solution_identities)
                {
                    return Err(invalid(
                        "pc path does not accept supplied colored solution identities",
                    ));
                }
            }
            PcResultProjection::TilingFamilyV1(_) => {
                if self.objective != ObjectivePolicy::tiling() {
                    return Err(invalid(
                        "pc tiling requires the geometry-only tiling objective without constraints",
                    ));
                }
                if self.solution_probabilities {
                    return Err(invalid(
                        "pc tiling does not accept per-solution probability calculation",
                    ));
                }
                if self.tablebase_requested || self.precompute_build_dependencies {
                    return Err(invalid(
                        "pc tiling does not accept tablebase or build-dependency semantics",
                    ));
                }
                if self
                    .scenario
                    .as_ref()
                    .is_some_and(WebPcScenarioInput::has_allowed_colored_solution_identities)
                {
                    return Err(invalid(
                        "pc tiling does not accept supplied colored solution identities",
                    ));
                }
            }
            PcResultProjection::SaveGroupsV2(_) | PcResultProjection::BestSaveV2(_) => {
                if self.max_memory_mib.is_some() {
                    return Err(invalid(
                        "pc saves does not support an explicit memory cap until terminal-supply proof memory is accounted",
                    ));
                }
                if self.queue.is_some() {
                    return Err(invalid(
                        "pc saves requires fixed bag-boundary provenance and does not accept an exact queue",
                    ));
                }
                if self.objective != ObjectivePolicy::all() {
                    return Err(invalid(
                        "pc saves requires the all non-scoring objective without constraints",
                    ));
                }
                if self.solution_probabilities {
                    return Err(invalid(
                        "pc saves computes exact group probabilities and does not accept per-solution probabilities",
                    ));
                }
                if self.queue_observation_policy != QueueObservationPolicy::FullQueueOracle {
                    return Err(invalid("pc saves requires full-queue oracle knowledge"));
                }
                if self.tablebase_requested || self.precompute_build_dependencies {
                    return Err(invalid(
                        "pc saves does not accept tablebase or build-dependency semantics",
                    ));
                }
                if self
                    .scenario
                    .as_ref()
                    .is_some_and(WebPcScenarioInput::has_allowed_colored_solution_identities)
                {
                    return Err(invalid(
                        "pc saves does not accept supplied colored solution identities",
                    ));
                }
            }
            PcResultProjection::ChanceProbabilityV2(_) => {
                if self.max_memory_mib.is_some() {
                    return Err(invalid(
                        "pc chance does not support an explicit memory cap until transient proof memory is accounted",
                    ));
                }
                if self.objective != ObjectivePolicy::unique() {
                    return Err(invalid(
                        "pc chance requires the unique non-scoring objective without constraints",
                    ));
                }
                if self.solution_probabilities {
                    return Err(invalid(
                        "pc chance does not accept per-solution probability calculation",
                    ));
                }
                if self
                    .scenario
                    .as_ref()
                    .is_some_and(WebPcScenarioInput::has_allowed_colored_solution_identities)
                {
                    return Err(invalid(
                        "pc chance does not accept supplied colored solution identities",
                    ));
                }
            }
            PcResultProjection::MinimumCoverV2(_) => {
                if self.max_memory_mib.is_some() {
                    return Err(invalid(
                        "pc minimals does not support an explicit memory cap until exact replay scratch is accounted",
                    ));
                }
                if self.objective.kind() != ObjectivePolicy::minimum_cover().kind()
                    || self.objective.score().requested()
                {
                    return Err(invalid(
                        "pc minimals requires the non-scoring minimum-cover objective",
                    ));
                }
                if self.queue_observation_policy != QueueObservationPolicy::FullQueueOracle {
                    return Err(invalid("pc minimals requires full-queue oracle knowledge"));
                }
                if self.tablebase_requested || self.precompute_build_dependencies {
                    return Err(invalid(
                        "pc minimals does not accept tablebase or build-dependency semantics",
                    ));
                }
                if self
                    .scenario
                    .as_ref()
                    .is_some_and(WebPcScenarioInput::has_allowed_colored_solution_identities)
                {
                    return Err(invalid(
                        "pc minimals does not accept supplied colored solution identities",
                    ));
                }
            }
            PcResultProjection::ScoreSummaryV2(_) | PcResultProjection::ScorePortfolioV2(_) => {
                if self.backend != RequestedSearchBackend::Cpu
                    || self.workers != Some(1)
                    || self.automatic_worker_limit.is_some()
                    || self.use_all_logical_processors
                    || self.allow_backend_fallback
                    || self.gpu_device != GpuDeviceSelection::Auto
                    || self.cpu_warmup
                    || self.gpu_warmup
                    || self.tablebase_requested
                    || self.precompute_build_dependencies
                {
                    return Err(invalid(
                        "pc score requires the fixed Wasm CPU single-session execution policy",
                    ));
                }
                if self.max_memory_mib.is_some() {
                    return Err(invalid(
                        "pc score does not support an explicit memory cap until transient proof memory is accounted",
                    ));
                }
                if self.max_patterns != Some(PC_SCORE_MAX_PATTERNS)
                    || self.max_nodes.is_some()
                    || self.max_frontier_states.is_some()
                    || self.max_candidates.is_some()
                {
                    return Err(invalid(
                        "pc score requires its fixed product execution limits",
                    ));
                }
                if self.patterns.as_ref().is_some_and(|patterns| {
                    patterns.len() > PC_SCORE_MAX_PATTERN_BYTES || patterns.contains(';')
                }) {
                    return Err(invalid(
                        "pc score requires one bounded factorized queue expression",
                    ));
                }
                if self
                    .queue
                    .as_ref()
                    .is_some_and(|queue| queue.len() > PC_SCORE_MAX_SOURCE_PIECES)
                    || self
                        .supply_window_size
                        .is_some_and(|window| window.source_pieces() > PC_SCORE_MAX_SOURCE_PIECES)
                {
                    return Err(invalid("pc score accepts at most 16 source pieces"));
                }
                let score = self.objective.score();
                let expected_objective = if matches!(
                    self.pc_result_projection,
                    PcResultProjection::ScorePortfolioV2(_)
                ) {
                    ObjectivePolicy::minimum_cover().with_score_policy(score)
                } else {
                    ObjectivePolicy::all().with_score_policy(score)
                };
                if self.objective != expected_objective || !score.requested() {
                    return Err(invalid(
                        if matches!(
                            self.pc_result_projection,
                            PcResultProjection::ScorePortfolioV2(_)
                        ) {
                            "pc score-minimals requires the score-aware minimum-cover objective"
                        } else {
                            "pc score requires the all score-summary objective without constraints"
                        },
                    ));
                }
                if self.solution_probabilities {
                    return Err(invalid(
                        "pc score does not accept per-solution probability calculation",
                    ));
                }
                if self
                    .scenario
                    .as_ref()
                    .is_some_and(WebPcScenarioInput::has_allowed_colored_solution_identities)
                {
                    return Err(invalid(
                        "pc score does not accept supplied colored solution identities",
                    ));
                }
            }
            PcResultProjection::AllSpinSolution(_) => {
                if self.queue.is_none() || self.patterns.is_some() {
                    return Err(invalid(
                        "pc All-Spin exact solution projection requires exactly one fixed queue",
                    ));
                }
            }
            PcResultProjection::AllSpinPreservationChance(_) => {
                if self.patterns.is_none() || self.queue.is_some() {
                    return Err(invalid(
                        "pc All-Spin preservation chance projection requires exactly one queue pattern",
                    ));
                }
            }
        }
        // These request fields would otherwise be ignored or canonicalized
        // during lowering, erasing evidence that a typed caller supplied an
        // option that the selected product contract forbids.
        if self.pc_result_projection.spin_profile().is_some() && self.supply_window_size.is_some() {
            return Err(invalid(
                "pc All-Spin does not accept a source-piece window override",
            ));
        }
        if self.build_probability.is_some() || self.build_v2.is_some() {
            return Err(invalid(
                "typed PC does not accept a build base or target field",
            ));
        }
        let expected_count_policy = if self.pc_result_projection.score_origin().is_some()
            || self.pc_result_projection.score_minimals_origin().is_some()
            || self.pc_result_projection.save_origin().is_some()
            || self.pc_result_projection.path_origin().is_some()
        {
            PcCountPolicy::CountAll
        } else {
            PcCountPolicy::CountUnique
        };
        if self.count_policy != expected_count_policy {
            return Err(invalid(
                if expected_count_policy == PcCountPolicy::CountAll {
                    "this typed PC result requires all solution counting"
                } else {
                    "typed PC requires unique solution counting"
                },
            ));
        }
        if self
            .scenario
            .as_ref()
            .is_some_and(|scenario| scenario.count_policy() != expected_count_policy)
        {
            return Err(invalid(
                if expected_count_policy == PcCountPolicy::CountAll {
                    "this typed PC scenario result requires all solution counting"
                } else {
                    "typed PC scenario requires unique solution counting"
                },
            ));
        }
        if !self.virtual_files.is_empty() {
            return Err(invalid(
                "typed PC does not accept FILE or virtual-file input",
            ));
        }
        Ok(())
    }

    fn validate_build_v2_state(&self) -> Result<(), WebCommandError> {
        if self.command_kind != "build-v2" {
            return Ok(());
        }
        let input = self.build_v2.as_ref().ok_or_else(|| {
            WebCommandError::new(
                WebCommandErrorCode::MissingValue,
                "Build v2 requires a nominal typed input",
            )
        })?;
        let invalid = |message| WebCommandError::new(WebCommandErrorCode::InvalidValue, message);
        if self.backend != RequestedSearchBackend::Cpu
            || self.allow_backend_fallback
            || self.gpu_device != GpuDeviceSelection::Auto
            || self.gpu_warmup
        {
            return Err(invalid(
                "Build v2 is CPU-only and does not accept GPU or backend-fallback state",
            ));
        }
        if self.max_memory_mib.is_some() {
            return Err(invalid(
                "Build v2 does not accept max-memory-mib until governed request and response memory authority exists",
            ));
        }
        if self.tablebase_requested || self.precompute_build_dependencies {
            return Err(invalid(
                "Build v2 does not accept tablebase or dependency-DAG overrides",
            ));
        }
        if self.solution_probabilities {
            return Err(invalid(
                "Build v2 solution-probability evidence is fixed by its capability contract",
            ));
        }
        if self.scenario.is_some()
            || self.build_probability.is_some()
            || self.product_capability_contract.is_some()
            || self.supply_window_size.is_some()
            || !self.virtual_files.is_empty()
        {
            return Err(invalid(
                "Build v2 cannot combine nominal input with legacy PC/Build or file state",
            ));
        }
        if self.queue_observation_policy != QueueObservationPolicy::default()
            || self.objective != ObjectivePolicy::unique()
            || self.count_policy != PcCountPolicy::CountUnique
        {
            return Err(invalid(
                "Build v2 options must remain owned by the nominal Build input",
            ));
        }
        if self.hold_enabled != input.allow_hold() {
            return Err(invalid(
                "Build v2 hold policy disagrees with its nominal query input",
            ));
        }
        if self.queue.is_some() == self.patterns.is_some() {
            return Err(invalid(
                "Build v2 requires exactly one of --queue or --patterns",
            ));
        }
        Ok(())
    }

    fn validate_setup_score_state(&self) -> Result<(), WebCommandError> {
        if self.command_kind != "setup-score" {
            return Ok(());
        }
        let input = self.setup_score.as_ref().ok_or_else(|| {
            WebCommandError::new(
                WebCommandErrorCode::MissingValue,
                "Setup score requires a nominal typed input",
            )
        })?;
        let invalid = |message| WebCommandError::new(WebCommandErrorCode::InvalidValue, message);
        if self.backend != RequestedSearchBackend::Cpu
            || self.allow_backend_fallback
            || self.gpu_device != GpuDeviceSelection::Auto
            || self.runtime_webgpu_available
            || self.gpu_warmup
        {
            return Err(invalid(
                "Setup score is CPU-only and does not accept GPU or backend-fallback state",
            ));
        }
        if self.max_memory_mib.is_some() {
            return Err(invalid(
                "Setup score does not accept max-memory-mib without a governed multi-phase memory authority",
            ));
        }
        if self.max_nodes.is_some()
            || self.max_frontier_states.is_some()
            || self.max_candidates.is_some()
            || self.tablebase_requested
            || self.precompute_build_dependencies
            || self.cpu_warmup
            || self.solution_probabilities
        {
            return Err(invalid(
                "Setup score does not accept legacy search-limit, warmup, tablebase, dependency, or solution-probability overrides",
            ));
        }
        if self.scenario.is_some()
            || self.build_probability.is_some()
            || self.build_v2.is_some()
            || self.forward_search.is_some()
            || self.spin_structure.is_some()
            || self.percent_query.is_some()
            || self.setup_remaining.is_some()
            || self.queue.is_some()
            || self.patterns.is_some()
            || self.supply_window_size.is_some()
            || self.product_capability_contract.is_some()
            || !self.virtual_files.is_empty()
        {
            return Err(invalid(
                "Setup score nominal input cannot combine with another command, legacy queue, product wrapper, or file state",
            ));
        }
        if self.queue_observation_policy != QueueObservationPolicy::default()
            || self.objective != ObjectivePolicy::unique()
            || self.count_policy != PcCountPolicy::CountUnique
        {
            return Err(invalid(
                "Setup score semantics must remain owned by its nominal input",
            ));
        }
        if self.hold_enabled != input.setup_hold_enabled() {
            return Err(invalid(
                "Setup score hold policy disagrees with its nominal input",
            ));
        }
        Ok(())
    }

    fn attach_product_capability_contract(
        &self,
        request: AppRequest,
    ) -> Result<AppRequest, WebCommandError> {
        let Some(contract) = self.product_capability_contract else {
            return Ok(request);
        };
        request
            .with_product_capability_contract(contract)
            .map_err(|error| {
                WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("invalid typed product capability contract: {error}"),
                )
            })
    }

    pub fn to_app_request(&self) -> Result<AppRequest, WebCommandError> {
        self.to_app_request_without_structural_profiles()?
            .with_request_structural_profiles(self.request_structural_profiles)
            .map_err(|error| {
                WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("invalid request profile selection: {error}"),
                )
            })
    }

    fn to_app_request_without_structural_profiles(&self) -> Result<AppRequest, WebCommandError> {
        if self.command_kind == "utility-sequence" {
            let problem = self.operation_sequence.clone().ok_or_else(|| {
                WebCommandError::new(
                    WebCommandErrorCode::MissingValue,
                    "sequence requires a typed operation document",
                )
            })?;
            return Ok(AppRequest::new(AppCommand::UtilitySequence(
                OperationSequenceAppCommand::new(problem),
            )));
        }
        if self.command_kind == "utility-sequence-dependencies" {
            let problem = self.sequence_dependencies.clone().ok_or_else(|| {
                WebCommandError::new(
                    WebCommandErrorCode::MissingValue,
                    "sequence-dependencies requires a typed operation document",
                )
            })?;
            return Ok(AppRequest::new(AppCommand::UtilitySequenceDependencies(
                SequenceDependenciesAppCommand::new(problem),
            )));
        }
        if self.command_kind == "utility-parity" {
            let command = self.parity.clone().ok_or_else(|| {
                WebCommandError::new(
                    WebCommandErrorCode::MissingValue,
                    "parity requires a typed field document",
                )
            })?;
            return Ok(AppRequest::new(AppCommand::UtilityParity(command)));
        }
        if self.command_kind == "utility-fumen" {
            let command = self.fumen.clone().ok_or_else(|| {
                WebCommandError::new(
                    WebCommandErrorCode::MissingValue,
                    "fumen requires a typed transform request",
                )
            })?;
            return Ok(AppRequest::new(AppCommand::UtilityFumen(command)));
        }
        if self.command_kind == "utility-render" {
            let command = self.render.clone().ok_or_else(|| {
                WebCommandError::new(
                    WebCommandErrorCode::MissingValue,
                    "render requires a typed field document",
                )
            })?;
            return Ok(AppRequest::new(AppCommand::UtilityRender(command)));
        }
        if matches!(
            self.command_kind.as_str(),
            "utility-to-gray" | "utility-mirror"
        ) {
            let command = self.field_document_transform.clone().ok_or_else(|| {
                WebCommandError::new(
                    WebCommandErrorCode::MissingValue,
                    "field-document transform requires a typed field document",
                )
            })?;
            let app_command = match command.transform() {
                FieldDocumentTransformKind::ToGray => AppCommand::UtilityToGray(command),
                FieldDocumentTransformKind::Mirror => AppCommand::UtilityMirror(command),
            };
            return Ok(AppRequest::new(app_command));
        }
        self.validate_typed_pc_state()?;
        self.validate_build_v2_state()?;
        self.validate_setup_score_state()?;
        if self.command_kind == "verify" {
            let request = match self.verify_scope.as_deref() {
                Some("kicks") => {
                    AppRequest::new(AppCommand::VerifyKicks(VerifyAppCommand::kicks()))
                }
                scope => AppRequest::new(AppCommand::Verify(VerifyAppCommand::with_scope(
                    scope.map(ToOwned::to_owned),
                ))),
            };
            return self.attach_product_capability_contract(request);
        }
        if matches!(self.command_kind.as_str(), "damage" | "spin-finder" | "ren") {
            let query = self.forward_search.clone().ok_or_else(|| {
                WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    "forward-search command is missing its typed query",
                )
            })?;
            let workers = self.resolved_worker_budget();
            let command = match query.mode() {
                ForwardSearchMode::MaximumDamage | ForwardSearchMode::DamageAtLeast(_) => {
                    AppCommand::Damage(DamageAppCommand::new(query))
                }
                ForwardSearchMode::SpinFinder(_) => {
                    AppCommand::SpinFinder(SpinFinderAppCommand::new(query))
                }
                ForwardSearchMode::MaximumRen => AppCommand::Ren(RenAppCommand::new(query)),
            };
            let request = AppRequest::new(command).with_resource_budget(ResourceBudget::new(
                u16::try_from(workers).unwrap_or(u16::MAX),
                None,
                None,
            ));
            return self.attach_product_capability_contract(request);
        }
        if self.command_kind == "spin-structure" {
            let query = self.spin_structure.clone().ok_or_else(|| {
                WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    "spin-structure command is missing its typed query",
                )
            })?;
            let workers = self.resolved_worker_budget();
            let command = match self.spin_structure_product_mode {
                SpinStructureProductMode::Search => SpinStructureAppCommand::new(query),
                SpinStructureProductMode::Cover { max_patterns } => {
                    SpinStructureAppCommand::cover(query, max_patterns)
                }
                SpinStructureProductMode::Guaranteed {
                    final_piece,
                    max_patterns,
                    dependency_report,
                } => SpinStructureAppCommand::guaranteed(
                    query,
                    final_piece,
                    max_patterns,
                    dependency_report,
                ),
            };
            let request = AppRequest::new(AppCommand::SpinStructure(command)).with_resource_budget(
                ResourceBudget::new(u16::try_from(workers).unwrap_or(u16::MAX), None, None),
            );
            return self.attach_product_capability_contract(request);
        }
        if self.command_kind == "percent" {
            let query = self.percent_query.clone().ok_or_else(|| {
                WebCommandError::new(
                    WebCommandErrorCode::MissingValue,
                    "percent requires a compiled scenario query",
                )
            })?;
            let request = AppRequest::new(AppCommand::Percent(
                PercentAppCommand::new(query)
                    .with_failed_pattern_limit(self.percent_failed_pattern_limit),
            ));
            return self.attach_product_capability_contract(request);
        }
        if self.command_kind == "setup" {
            let remaining = self.setup_remaining.clone().ok_or_else(|| {
                WebCommandError::new(
                    WebCommandErrorCode::MissingValue,
                    "setup requires remaining pieces",
                )
            })?;
            let borrow_policy = if self.setup_allow_post_cycle_borrow {
                SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse
            } else {
                SetupCycleResetBorrowPolicy::ForbidPostCyclePieceUse
            };
            let mut query = SetupSearchQuery::default()
                .with_rule(self.rule)
                .with_remaining_pieces(remaining)
                .with_queue_observation_policy(self.queue_observation_policy)
                .with_tablebase_requested(self.tablebase_requested);
            match self.setup_search_mode {
                SetupSearchMode::ShapeOracle => {
                    if self.setup_queue_based_pieces.is_some() {
                        return Err(WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            "shape-oracle setup search does not accept observed QB pieces",
                        ));
                    }
                }
                SetupSearchMode::QueueBased => {
                    let pieces = self.setup_queue_based_pieces.clone().ok_or_else(|| {
                        WebCommandError::new(
                            WebCommandErrorCode::MissingValue,
                            "queue-based setup search requires observed next-bag pieces",
                        )
                    })?;
                    query = query.with_queue_based_pieces(pieces);
                }
            }
            if let Some(pieces) = self.setup_next_cycle_remaining_pieces.clone() {
                query = query.with_next_cycle_remaining_pieces(pieces);
            }
            let mut query = query
                .with_cycle_reset_borrow_policy(borrow_policy)
                .with_candidate_priority(self.setup_candidate_priority)
                .with_length_preference(self.setup_length_preference)
                .with_max_setup_pieces(self.setup_max_pieces);
            if let Some(detail) = self.setup_path_detail.clone() {
                query = query.with_path_detail(detail);
            }
            let workers = self.resolved_worker_budget();
            let request = AppRequest::new(AppCommand::Setup(SetupAppCommand::new(query)))
                .with_resource_budget(ResourceBudget::new(
                    u16::try_from(workers).unwrap_or(u16::MAX),
                    None,
                    None,
                ));
            return self.attach_product_capability_contract(request);
        }
        if !matches!(
            self.command_kind.as_str(),
            "pc" | "failed-queue" | "build-probability" | "build-v2" | "setup-score"
        ) {
            return Err(WebCommandError::new(
                WebCommandErrorCode::UnsupportedCommand,
                format!("unsupported web command '{}'", self.command_kind),
            ));
        }
        let mut policy = PcExecutionPolicy::mvp_default()
            .with_worker_hardware_limit(self.worker_hardware_limit)
            .with_runtime_webgpu_available(self.runtime_webgpu_available)
            .with_requested_backend(self.backend)
            .with_gpu_device(self.gpu_device.clone())
            .with_allow_backend_fallback(self.allow_backend_fallback)
            .with_use_all_logical_processors(self.use_all_logical_processors)
            .with_cpu_warmup(self.cpu_warmup)
            .with_gpu_warmup(self.gpu_warmup)
            .with_tablebase_requested(self.tablebase_requested)
            .with_precompute_build_dependencies(self.precompute_build_dependencies);
        if let Some(max_patterns) = self.max_patterns {
            policy = policy.with_max_patterns(max_patterns);
        }
        if let Some(max_nodes) = self.max_nodes {
            policy = policy.with_max_nodes(max_nodes);
        }
        if let Some(max_frontier_states) = self.max_frontier_states {
            policy = policy.with_max_frontier_states(max_frontier_states);
        }
        if let Some(max_candidates) = self.max_candidates {
            policy = policy.with_max_candidates(max_candidates);
        }
        if let Some(max_memory_mib) = self.max_memory_mib {
            policy = policy.with_max_memory_mib(Some(max_memory_mib));
        }
        if let Some(workers) = self.workers {
            policy = policy.with_workers(workers);
        } else if let Some(workers) = self.automatic_worker_limit {
            policy = policy.with_automatic_worker_limit(workers);
        }

        if self.command_kind == "setup-score" {
            let input = self.setup_score.as_ref().ok_or_else(|| {
                WebCommandError::new(
                    WebCommandErrorCode::MissingValue,
                    "Setup score command is missing its nominal typed input",
                )
            })?;
            let command = input.to_app_command(policy, self.rule)?;
            let workers = self.resolved_worker_budget();
            return Ok(
                AppRequest::new(AppCommand::SetupScore(command)).with_resource_budget(
                    ResourceBudget::new(u16::try_from(workers).unwrap_or(u16::MAX), None, None),
                ),
            );
        }

        let standard_bag_pattern = self
            .patterns
            .as_deref()
            .and_then(QueuePatternExpression::standard_7_bag_with_optional_leading_piece);
        let leading_supply_piece = standard_bag_pattern
            .and_then(|(leading_piece, _)| leading_piece)
            .filter(|_| {
                self.hold_enabled
                    && (self
                        .scenario
                        .as_ref()
                        .is_some_and(|scenario| scenario.hold_piece().is_none())
                        || self
                            .build_probability
                            .as_ref()
                            .is_some_and(|input| input.hold_piece().is_none())
                        || self
                            .build_v2
                            .as_ref()
                            .is_some_and(|input| input.hold_piece().is_none()))
            });
        let finite_standard_bag_len = standard_bag_pattern.and_then(|(leading_piece, length)| {
            (leading_piece.is_none() || leading_supply_piece.is_some()).then_some(length)
        });
        let score_summary_requested = self.pc_result_projection.score_origin().is_some();
        if score_summary_requested
            && finite_standard_bag_len.is_some_and(|length| length > PC_SCORE_MAX_SOURCE_PIECES)
        {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!("pc score accepts at most {PC_SCORE_MAX_SOURCE_PIECES} source pieces"),
            ));
        }
        let queue = if let Some(patterns) = &self.patterns {
            if finite_standard_bag_len.is_some() {
                PcQueueInput::standard_7_bag()
            } else {
                let expression = QueuePatternExpression::parse(patterns, policy.max_patterns())
                    .map_err(|error| {
                        WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            format!("invalid web queue pattern: {error}"),
                        )
                    })?;
                if score_summary_requested && expression.sequence_len() > PC_SCORE_MAX_SOURCE_PIECES
                {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!(
                            "pc score accepts at most {PC_SCORE_MAX_SOURCE_PIECES} source pieces"
                        ),
                    ));
                }
                PcQueueInput::pattern_expression(expression)
            }
        } else if let Some(queue) = &self.queue {
            let fixed = queue_parser::parse_fixed_sequence(queue).map_err(|error| {
                WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("invalid web queue: {error:?}"),
                )
            })?;
            if score_summary_requested && fixed.len() > PC_SCORE_MAX_SOURCE_PIECES {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("pc score accepts at most {PC_SCORE_MAX_SOURCE_PIECES} source pieces"),
                ));
            }
            PcQueueInput::fixed_sequence(fixed)
        } else {
            PcQueueInput::standard_7_bag()
        };
        if self.command_kind == "build-v2" {
            let input = self.build_v2.as_ref().ok_or_else(|| {
                WebCommandError::new(
                    WebCommandErrorCode::MissingValue,
                    "Build v2 command is missing its nominal typed input",
                )
            })?;
            let input = leading_supply_piece.map_or_else(
                || input.clone(),
                |piece| input.clone().with_leading_hold_piece(piece),
            );
            let command =
                input.to_app_command(queue, policy, finite_standard_bag_len, self.rule)?;
            let workers = self.resolved_worker_budget();
            return Ok(
                AppRequest::new(AppCommand::BuildV2(command)).with_resource_budget(
                    ResourceBudget::new(u16::try_from(workers).unwrap_or(u16::MAX), None, None),
                ),
            );
        }
        if self.command_kind == "build-probability" {
            let input = self.build_probability.as_ref().ok_or_else(|| {
                WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    "build-probability command is missing its field input",
                )
            })?;
            let input = leading_supply_piece.map_or_else(
                || input.clone(),
                |piece| input.clone().with_leading_hold_piece(piece),
            );
            let mut query = input
                .to_query(
                    queue,
                    policy,
                    finite_standard_bag_len,
                    self.rule,
                    self.objective,
                )
                .map_err(|error| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid build-probability field: {error:?}"),
                    )
                })?
                .with_queue_observation_policy(self.queue_observation_policy);
            if query.aggregation().is_tiling_only()
                && self.queue_observation_policy == QueueObservationPolicy::VisibleSeven
            {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    "visible-7 queue knowledge is unavailable with tiling-only Build semantics",
                ));
            }
            if self.solution_probabilities {
                if query.aggregation().is_tiling_only() {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "solution-probabilities is unavailable with tiling aggregation",
                    ));
                }
                if query.finesse_score().is_some() {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "solution-probabilities is unavailable with finesse score",
                    ));
                }
                query =
                    query.with_solution_probability_policy(BuildSolutionProbabilityPolicy::Include);
            }
            let query_memory_mib = query.core_query().execution_policy().max_memory_mib();
            let request_memory_mib = query_memory_mib
                .map(|memory_mib| {
                    u32::try_from(memory_mib).map_err(|_| {
                        WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            "build-probability max-memory-mib exceeds the App request authority range",
                        )
                    })
                })
                .transpose()?;
            let workers = self.resolved_worker_budget();
            let resource_budget = ResourceBudget::new(
                u16::try_from(workers).unwrap_or(u16::MAX),
                None,
                request_memory_mib.map(u64::from),
            );
            if resource_budget.max_memory_mib().map(u64::from) != query_memory_mib
                || resource_budget.memory_mib() != query_memory_mib
            {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    "build-probability memory authorities disagree",
                ));
            }
            let request = AppRequest::new(AppCommand::BuildProbability(
                BuildProbabilityAppCommand::new(query),
            ))
            .with_resource_budget(resource_budget);
            return self.attach_product_capability_contract(request);
        }
        if let Some(scenario) = &self.scenario {
            let scenario = leading_supply_piece.map_or_else(
                || scenario.clone(),
                |piece| scenario.clone().with_hold_piece(Some(piece)),
            );
            let mut query = scenario
                .to_query(
                    queue,
                    policy,
                    finite_standard_bag_len,
                    self.rule,
                    self.objective,
                )
                .with_queue_observation_policy(self.queue_observation_policy);
            if self.solution_probabilities {
                query =
                    query.with_solution_probability_policy(PcSolutionProbabilityPolicy::Include);
            }
            let command = if self.command_kind == "failed-queue" {
                AppCommand::Percent(match self.pc_failed_queue_origin {
                    Some(origin) => PercentAppCommand::pc_failed_queue(query, origin)
                        .with_failed_pattern_limit(self.percent_failed_pattern_limit),
                    None => PercentAppCommand::failed_queue(query)
                        .with_failed_pattern_limit(self.percent_failed_pattern_limit),
                })
            } else {
                let command = ScenarioAppCommand::new(query)
                    .with_result_projection(self.pc_result_projection);
                command.validate_result_projection().map_err(|reason| {
                    WebCommandError::new(WebCommandErrorCode::InvalidValue, reason)
                })?;
                AppCommand::Scenario(command)
            };
            return self.attach_product_capability_contract(AppRequest::new(command));
        }

        let target = PcTarget::new(self.lines).map_err(|error| {
            WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!("invalid pc line target: {error:?}"),
            )
        })?;

        let mut query = OpeningPcSearchQuery::new(target)
            .with_execution_policy(policy)
            .with_queue(queue)
            .with_rule(self.rule)
            .with_objective(self.objective)
            .with_queue_observation_policy(self.queue_observation_policy)
            .with_hold_policy(if self.hold_enabled {
                PcHoldPolicy::default()
            } else {
                PcHoldPolicy::Disabled
            });
        if self.supply_window_size.is_none() {
            if let Some(length) = finite_standard_bag_len {
                let required_pieces = usize::from(self.lines) * 10 / 4;
                query = query.with_supply_window_size(SupplyWindowSize::new(
                    length.min(required_pieces.saturating_add(usize::from(self.hold_enabled))),
                ));
            }
        }
        if let Some(supply_window_size) = self.supply_window_size {
            query = query.with_supply_window_size(supply_window_size);
        }
        if self.solution_probabilities {
            query = query.with_solution_probability_policy(PcSolutionProbabilityPolicy::Include);
        }

        let command = if self.command_kind == "failed-queue" {
            AppCommand::Percent(match self.pc_failed_queue_origin {
                Some(origin) => PercentAppCommand::pc_failed_queue_opening(query, origin)
                    .with_failed_pattern_limit(self.percent_failed_pattern_limit),
                None => PercentAppCommand::failed_queue_opening(query)
                    .with_failed_pattern_limit(self.percent_failed_pattern_limit),
            })
        } else {
            let command =
                PcAppCommand::new(query).with_result_projection(self.pc_result_projection);
            command.validate_result_projection().map_err(|reason| {
                WebCommandError::new(WebCommandErrorCode::InvalidValue, reason)
            })?;
            AppCommand::Pc(command)
        };
        self.attach_product_capability_contract(AppRequest::new(command))
    }
}
impl WebCommandRequest {
    pub fn command_kind(&self) -> &str {
        &self.command_kind
    }

    pub const fn pc_result_projection(&self) -> PcResultProjection {
        self.pc_result_projection
    }

    pub const fn product_capability_contract(&self) -> Option<ProductCapabilityContract> {
        self.product_capability_contract
    }

    pub const fn pc_failed_queue_origin(&self) -> Option<PcFailedQueueIngressOrigin> {
        self.pc_failed_queue_origin
    }

    pub const fn build_v2_input(&self) -> Option<&WebBuildV2Input> {
        self.build_v2.as_ref()
    }

    pub const fn request_structural_profiles(&self) -> RequestStructuralProfiles {
        self.request_structural_profiles
    }
}
impl WebCommandRequest {
    pub const fn lines(&self) -> u8 {
        self.lines
    }
}
impl WebCommandRequest {
    pub const fn backend(&self) -> RequestedSearchBackend {
        self.backend
    }
}
impl WebCommandRequest {
    pub fn gpu_device(&self) -> &GpuDeviceSelection {
        &self.gpu_device
    }
}
impl WebCommandRequest {
    pub const fn allow_backend_fallback(&self) -> bool {
        self.allow_backend_fallback
    }
}
impl WebCommandRequest {
    pub fn queue(&self) -> Option<&str> {
        self.queue.as_deref()
    }

    pub fn patterns(&self) -> Option<&str> {
        self.patterns.as_deref()
    }
}
impl WebCommandRequest {
    pub fn virtual_files(&self) -> &[WebVirtualFileHandle] {
        &self.virtual_files
    }
}
impl WebCommandRequest {
    pub fn scenario(&self) -> Option<&WebPcScenarioInput> {
        self.scenario.as_ref()
    }

    pub fn build_probability_input(&self) -> Option<&WebBuildProbabilityInput> {
        self.build_probability.as_ref()
    }

    pub fn forward_search_query(&self) -> Option<&ForwardSearchQuery> {
        self.forward_search.as_ref()
    }

    pub fn spin_structure_query(&self) -> Option<&SpinStructureQuery> {
        self.spin_structure.as_ref()
    }
}
impl WebCommandRequest {
    pub fn backend_requested(&self) -> &'static str {
        if matches!(
            self.command_kind.as_str(),
            "pc" | "failed-queue" | "build-probability" | "setup"
        ) {
            self.backend.as_str()
        } else {
            "cpu"
        }
    }

    pub fn requests_webgpu(&self) -> bool {
        matches!(
            self.command_kind.as_str(),
            "pc" | "failed-queue" | "build-probability"
        ) && matches!(
            self.backend,
            RequestedSearchBackend::Auto
                | RequestedSearchBackend::Gpu
                | RequestedSearchBackend::Hybrid
        )
    }
}
