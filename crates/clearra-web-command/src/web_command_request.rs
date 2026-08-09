use clearra_app::{
    AppCommand, AppRequest, BuildProbabilityAppCommand, DamageAppCommand, PcAppCommand,
    PercentAppCommand, ResourceBudget, ScenarioAppCommand, SetupAppCommand, SpinFinderAppCommand,
    SpinStructureAppCommand, VerifyAppCommand,
};
use clearra_core_domain::pc::pc_target::PcTarget;
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_forward_search::{ForwardSearchMode, ForwardSearchQuery};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    GpuDeviceSelection, OpeningPcSearchQuery, PcCountPolicy, PcExecutionPolicy, PcHoldPolicy,
    PcQueueInput, PcScenarioQuery, PcSolutionProbabilityPolicy, RequestedSearchBackend,
    SupplyWindowSize, WorkerPolicy,
};
use clearra_problem::{
    SetupCandidatePriority, SetupCycleResetBorrowPolicy, SetupLengthPreference, SetupPathDetail,
    SetupSearchMode, SetupSearchQuery,
};
use clearra_rules::profile::{builtin_rules::srs_plus, rule_profile::RuleProfile};
use clearra_spin_structure_search::SpinStructureQuery;
use clearra_supply::{
    queue::{queue_parser, queue_pattern_expression::QueuePatternExpression},
    QueueObservationPolicy,
};

use crate::{WebBuildProbabilityInput, WebPcScenarioInput};
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
    queue_observation_policy: QueueObservationPolicy,
    scenario: Option<WebPcScenarioInput>,
    build_probability: Option<WebBuildProbabilityInput>,
    forward_search: Option<ForwardSearchQuery>,
    spin_structure: Option<SpinStructureQuery>,
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
            queue_observation_policy: QueueObservationPolicy::default(),
            scenario: None,
            build_probability: None,
            forward_search: None,
            spin_structure: None,
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
            queue_observation_policy: QueueObservationPolicy::default(),
            scenario: None,
            build_probability: None,
            forward_search: None,
            spin_structure: None,
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
            virtual_files: Vec::new(),
        }
    }
}
impl WebCommandRequest {
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
        self
    }

    pub fn with_objective(mut self, objective: ObjectivePolicy) -> Self {
        self.objective = objective;
        if objective.score().requested() {
            self.count_policy = PcCountPolicy::CountAll;
        }
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

    pub fn to_app_request(&self) -> Result<AppRequest, WebCommandError> {
        if self.command_kind == "verify" {
            return Ok(match self.verify_scope.as_deref() {
                Some("kicks") => {
                    AppRequest::new(AppCommand::VerifyKicks(VerifyAppCommand::kicks()))
                }
                scope => AppRequest::new(AppCommand::Verify(VerifyAppCommand::with_scope(
                    scope.map(ToOwned::to_owned),
                ))),
            });
        }
        if matches!(self.command_kind.as_str(), "damage" | "spin-finder") {
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
            };
            return Ok(
                AppRequest::new(command).with_resource_budget(ResourceBudget::new(
                    u16::try_from(workers).unwrap_or(u16::MAX),
                    None,
                    None,
                )),
            );
        }
        if self.command_kind == "spin-structure" {
            let query = self.spin_structure.clone().ok_or_else(|| {
                WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    "spin-structure command is missing its typed query",
                )
            })?;
            let workers = self.resolved_worker_budget();
            return Ok(
                AppRequest::new(AppCommand::SpinStructure(SpinStructureAppCommand::new(
                    query,
                )))
                .with_resource_budget(ResourceBudget::new(
                    u16::try_from(workers).unwrap_or(u16::MAX),
                    None,
                    None,
                )),
            );
        }
        if self.command_kind == "percent" {
            let query = self.percent_query.clone().ok_or_else(|| {
                WebCommandError::new(
                    WebCommandErrorCode::MissingValue,
                    "percent requires a compiled scenario query",
                )
            })?;
            return Ok(AppRequest::new(AppCommand::Percent(
                PercentAppCommand::new(query)
                    .with_failed_pattern_limit(self.percent_failed_pattern_limit),
            )));
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
            return Ok(
                AppRequest::new(AppCommand::Setup(SetupAppCommand::new(query)))
                    .with_resource_budget(ResourceBudget::new(
                        u16::try_from(workers).unwrap_or(u16::MAX),
                        None,
                        None,
                    )),
            );
        }
        if !matches!(
            self.command_kind.as_str(),
            "pc" | "failed-queue" | "build-probability"
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
        if matches!(self.backend, RequestedSearchBackend::Auto) {
            policy = policy.with_allow_backend_fallback(true);
        }
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
                            .is_some_and(|input| input.hold_piece().is_none()))
            });
        let finite_standard_bag_len = standard_bag_pattern.and_then(|(leading_piece, length)| {
            (leading_piece.is_none() || leading_supply_piece.is_some()).then_some(length)
        });
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
                PcQueueInput::pattern_expression(expression)
            }
        } else if let Some(queue) = &self.queue {
            let fixed = queue_parser::parse_fixed_sequence(queue).map_err(|error| {
                WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("invalid web queue: {error:?}"),
                )
            })?;
            PcQueueInput::fixed_sequence(fixed)
        } else {
            PcQueueInput::standard_7_bag()
        };
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
            let query = input
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
                })?;
            return Ok(AppRequest::new(AppCommand::BuildProbability(
                BuildProbabilityAppCommand::new(query),
            )));
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
                AppCommand::Percent(
                    PercentAppCommand::failed_queue(query)
                        .with_failed_pattern_limit(self.percent_failed_pattern_limit),
                )
            } else {
                AppCommand::Scenario(ScenarioAppCommand::new(query))
            };
            return Ok(AppRequest::new(command));
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
            AppCommand::Percent(
                PercentAppCommand::failed_queue_opening(query)
                    .with_failed_pattern_limit(self.percent_failed_pattern_limit),
            )
        } else {
            AppCommand::Pc(PcAppCommand::new(query))
        };
        Ok(AppRequest::new(command))
    }
}
impl WebCommandRequest {
    pub fn command_kind(&self) -> &str {
        &self.command_kind
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
