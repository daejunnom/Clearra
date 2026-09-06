use std::fmt;

use clearra_i18n::LanguageId;

use crate::{
    app_command::AppCommand,
    io::AppFilePolicy,
    product_capability_contract::{
        ProductCapabilityContract, ProductCapabilityContractError,
        ValidatedProductCapabilityContract,
    },
    request_profile_selection::{
        RequestProfileSelection, RequestProfileSelectionError, RequestStructuralProfiles,
    },
};
use clearra_host_contract::{
    BackendPolicy, DiagnosticsPolicy, LocalePolicy, OutputPolicy, QueryEnvelope, ResourceBudget,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppOutputPolicy {
    contract: OutputPolicy,
}

impl AppOutputPolicy {
    pub fn new(include_render_model: bool) -> Self {
        Self {
            contract: OutputPolicy::new("text", include_render_model),
        }
    }
}
impl AppOutputPolicy {
    pub fn include_render_model(&self) -> bool {
        self.contract.include_render_model()
    }
}
impl AppOutputPolicy {
    pub fn contract(&self) -> &OutputPolicy {
        &self.contract
    }

    pub(crate) fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        self.contract.checked_retained_capacity_bytes()
    }
}

impl Default for AppOutputPolicy {
    fn default() -> Self {
        Self::new(true)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AppRequest {
    command: AppCommand,
    query: QueryEnvelope,
    backend_policy: BackendPolicy,
    output_policy: AppOutputPolicy,
    diagnostics_policy: DiagnosticsPolicy,
    locale_policy: LocalePolicy,
    resource_budget: ResourceBudget,
    language: Option<LanguageId>,
    file_policy: Option<AppFilePolicy>,
    product_capability_contract: Option<ValidatedProductCapabilityContract>,
    request_profiles: RequestProfileSelection,
}

pub(crate) type AppExecutionParts = (
    AppCommand,
    AppOutputPolicy,
    ResourceBudget,
    Option<LanguageId>,
    Option<AppFilePolicy>,
    Option<ValidatedProductCapabilityContract>,
);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AppExecutionPartsRejection {
    command_kind: clearra_host_contract::AppCommandKind,
    output_policy: AppOutputPolicy,
    error: AppRequestBindingError,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AppRequestBindingError {
    ProductCapability(ProductCapabilityContractError),
    RequestProfiles(RequestProfileSelectionError),
}

impl fmt::Display for AppRequestBindingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProductCapability(error) => {
                write!(formatter, "product capability request rejected: {error}")
            }
            Self::RequestProfiles(error) => {
                write!(formatter, "request profile selection rejected: {error}")
            }
        }
    }
}

impl AppExecutionPartsRejection {
    pub(crate) fn into_parts(
        self,
    ) -> (
        clearra_host_contract::AppCommandKind,
        AppOutputPolicy,
        AppRequestBindingError,
    ) {
        (self.command_kind, self.output_policy, self.error)
    }
}

impl AppRequest {
    pub fn new(command: AppCommand) -> Self {
        let query = command.query_envelope();
        let backend_policy = command.backend_policy();
        let request_profiles = RequestProfileSelection::for_command(&command);
        Self {
            command,
            query,
            backend_policy,
            output_policy: AppOutputPolicy::default(),
            diagnostics_policy: DiagnosticsPolicy::default(),
            locale_policy: LocalePolicy::default(),
            resource_budget: ResourceBudget::default(),
            language: None,
            file_policy: None,
            product_capability_contract: None,
            request_profiles,
        }
    }
}
impl AppRequest {
    pub fn with_request_structural_profiles(
        mut self,
        structural: RequestStructuralProfiles,
    ) -> Result<Self, RequestProfileSelectionError> {
        let profiles = self.request_profiles.with_structural_profiles(structural);
        profiles.validate_for_command(&self.command)?;
        self.request_profiles = profiles;
        Ok(self)
    }

    pub fn request_profiles(&self) -> RequestProfileSelection {
        self.request_profiles
    }

    pub(crate) fn validate_request_profile_binding(
        &self,
    ) -> Result<(), RequestProfileSelectionError> {
        self.request_profiles.validate_for_command(&self.command)
    }
}
impl AppRequest {
    pub fn with_product_capability_contract(
        mut self,
        contract: ProductCapabilityContract,
    ) -> Result<Self, ProductCapabilityContractError> {
        let validated = contract.validate_request(&self.command, &self.query)?;
        self.product_capability_contract = Some(validated);
        Ok(self)
    }
}
impl AppRequest {
    pub fn with_output_policy(mut self, output_policy: AppOutputPolicy) -> Self {
        self.output_policy = output_policy;
        self
    }
}
impl AppRequest {
    pub fn with_language(mut self, language: LanguageId) -> Self {
        self.language = Some(language);
        self
    }
}
impl AppRequest {
    pub fn with_file_policy(mut self, file_policy: AppFilePolicy) -> Self {
        self.file_policy = Some(file_policy);
        self
    }
}
impl AppRequest {
    pub fn with_backend_policy(mut self, backend_policy: BackendPolicy) -> Self {
        self.backend_policy = backend_policy;
        self
    }
}
impl AppRequest {
    pub fn with_diagnostics_policy(mut self, diagnostics_policy: DiagnosticsPolicy) -> Self {
        self.diagnostics_policy = diagnostics_policy;
        self
    }
}
impl AppRequest {
    pub fn with_locale_policy(mut self, locale_policy: LocalePolicy) -> Self {
        self.locale_policy = locale_policy;
        self
    }
}
impl AppRequest {
    pub fn with_resource_budget(mut self, resource_budget: ResourceBudget) -> Self {
        self.resource_budget = resource_budget;
        self
    }

    /// Host result-construction policy; never rewrites search memory or query semantics.
    pub fn with_product_retention_budget(
        mut self,
        budget: Option<crate::ProductRetentionBudget>,
    ) -> Self {
        if let AppCommand::BuildProbability(command) = &mut self.command {
            command.set_product_retention_budget(budget);
        }
        self
    }
}
impl AppRequest {
    pub fn command(&self) -> &AppCommand {
        &self.command
    }
}
impl AppRequest {
    pub fn command_kind(&self) -> clearra_host_contract::AppCommandKind {
        self.command.kind()
    }
}
impl AppRequest {
    pub fn query(&self) -> &QueryEnvelope {
        &self.query
    }
}
impl AppRequest {
    pub fn backend_policy(&self) -> &BackendPolicy {
        &self.backend_policy
    }
}
impl AppRequest {
    pub fn diagnostics_policy(&self) -> DiagnosticsPolicy {
        self.diagnostics_policy
    }
}
impl AppRequest {
    pub fn locale_policy(&self) -> &LocalePolicy {
        &self.locale_policy
    }
}
impl AppRequest {
    pub fn resource_budget(&self) -> ResourceBudget {
        self.resource_budget
    }
}
impl AppRequest {
    pub fn into_command(self) -> AppCommand {
        self.command
    }
}
impl AppRequest {
    pub fn into_parts(
        self,
    ) -> (
        AppCommand,
        AppOutputPolicy,
        Option<LanguageId>,
        Option<AppFilePolicy>,
    ) {
        (
            self.command,
            self.output_policy,
            self.language,
            self.file_policy,
        )
    }
}
impl AppRequest {
    pub(crate) fn validate_product_capability_binding(
        &self,
    ) -> Result<(), ProductCapabilityContractError> {
        let required_contract = ProductCapabilityContract::required_for_command(&self.command);
        match (required_contract, self.product_capability_contract.as_ref()) {
            (None, None) => Ok(()),
            (Some(required), None) => {
                Err(ProductCapabilityContractError::RequiredContractMissing { required })
            }
            (None, Some(actual)) => Err(ProductCapabilityContractError::UnexpectedContract {
                actual: actual.contract(),
            }),
            (Some(required), Some(actual)) if required != actual.contract() => {
                Err(ProductCapabilityContractError::RequiredContractMismatch {
                    required,
                    actual: actual.contract(),
                })
            }
            (Some(required), Some(actual)) => required
                .validate_request(&self.command, &self.query)
                .and_then(|fresh| {
                    if &fresh == actual {
                        Ok(())
                    } else {
                        Err(ProductCapabilityContractError::StaleRequestContract {
                            contract: required,
                        })
                    }
                }),
        }
    }

    pub(crate) fn into_execution_parts(
        self,
    ) -> Result<AppExecutionParts, AppExecutionPartsRejection> {
        if let Err(error) = self.validate_request_profile_binding() {
            return Err(AppExecutionPartsRejection {
                command_kind: self.command.kind(),
                output_policy: self.output_policy,
                error: AppRequestBindingError::RequestProfiles(error),
            });
        }
        if let Err(error) = self.validate_product_capability_binding() {
            return Err(AppExecutionPartsRejection {
                command_kind: self.command.kind(),
                output_policy: self.output_policy,
                error: AppRequestBindingError::ProductCapability(error),
            });
        }

        Ok((
            self.command,
            self.output_policy,
            self.resource_budget,
            self.language,
            self.file_policy,
            self.product_capability_contract,
        ))
    }
}
impl AppRequest {
    pub fn product_capability_contract(&self) -> Option<ProductCapabilityContract> {
        self.product_capability_contract
            .as_ref()
            .map(ValidatedProductCapabilityContract::contract)
    }

    #[cfg(test)]
    pub(crate) fn with_query_envelope_for_test(mut self, query: QueryEnvelope) -> Self {
        self.query = query;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_command_for_product_capability_test(mut self, command: AppCommand) -> Self {
        self.command = command;
        self
    }
}
impl AppRequest {
    pub fn output_policy(&self) -> &AppOutputPolicy {
        &self.output_policy
    }
}
impl AppRequest {
    pub fn language(&self) -> Option<LanguageId> {
        self.language
    }
}
impl AppRequest {
    pub fn file_policy(&self) -> Option<&AppFilePolicy> {
        self.file_policy.as_ref()
    }
}

impl AppRequest {
    /// Returns the complete heap payload currently owned by a typed
    /// Build-probability request, measured fieldwise from allocator capacity.
    ///
    /// The command query, backend string, output-format string, and optional
    /// locale string are counted exactly once. `QueryEnvelope` is a unit tag;
    /// diagnostics, resource, language, and file policies are inline. A
    /// mismatched command/envelope or any product-capability proof fails closed:
    /// Build has no such proof, and PC-family proofs may contain shared `Arc`
    /// pointees whose backing allocations are owned and measured elsewhere.
    pub fn checked_build_probability_retained_capacity_bytes(&self) -> Option<u128> {
        let AppCommand::BuildProbability(command) = &self.command else {
            return None;
        };
        if self.query != QueryEnvelope::BuildProbability
            || self.product_capability_contract.is_some()
        {
            return None;
        }

        checked_build_probability_retained_capacity_sum([
            command.query().checked_retained_capacity_bytes()?,
            self.backend_policy.checked_retained_capacity_bytes()?,
            self.output_policy.checked_retained_capacity_bytes()?,
            self.locale_policy.checked_retained_capacity_bytes()?,
        ])
    }
}

fn checked_build_probability_retained_capacity_sum(components: [u128; 4]) -> Option<u128> {
    components
        .into_iter()
        .try_fold(0_u128, |total, bytes| total.checked_add(bytes))
}

#[cfg(test)]
mod retained_capacity_tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_host_contract::{BackendPolicy, LocalePolicy, QueryEnvelope};
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::{
        BuildProbabilityField, BuildProbabilityQuery, FinessePlacement, FinesseScoreRequest,
    };
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::{checked_build_probability_retained_capacity_sum, AppRequest};
    use crate::{
        app_command::AppCommand,
        commands::{BuildProbabilityAppCommand, RulesAppCommand},
    };

    fn build_query() -> BuildProbabilityQuery {
        let mut pieces = Vec::with_capacity(43);
        pieces.push(PieceKind::O);
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(pieces)),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let field =
            BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0x0c03, 0, 0, 0])
                .expect("one-piece field");
        let mut placements = Vec::with_capacity(47);
        placements.push(FinessePlacement::new(
            PieceKind::O,
            RotationState::Zero,
            4,
            0,
        ));
        BuildProbabilityQuery::new(core, field).with_finesse_score(
            FinesseScoreRequest::new(placements).expect("one placement is valid"),
        )
    }

    #[test]
    fn build_request_counts_owned_heap_once_and_unit_envelope_adds_nothing() {
        let query = build_query();
        let query_capacity = query
            .checked_retained_capacity_bytes()
            .expect("query capacity fits u128");
        let mut backend = String::with_capacity(53);
        backend.push_str("cpu");
        let backend_capacity = backend.capacity() as u128;
        let mut locale = String::with_capacity(59);
        locale.push_str("ko-KR");
        let locale_capacity = locale.capacity() as u128;
        let request = AppRequest::new(AppCommand::BuildProbability(
            BuildProbabilityAppCommand::new(query),
        ))
        .with_backend_policy(BackendPolicy::new(backend, false))
        .with_locale_policy(LocalePolicy::new(Some(locale)));
        assert_eq!(request.query(), &QueryEnvelope::BuildProbability);
        let output_capacity = request
            .output_policy
            .checked_retained_capacity_bytes()
            .expect("output policy capacity fits u128");
        let expected = checked_build_probability_retained_capacity_sum([
            query_capacity,
            backend_capacity,
            output_capacity,
            locale_capacity,
        ])
        .expect("request capacity fits u128");
        let actual = request
            .checked_build_probability_retained_capacity_bytes()
            .expect("typed Build request is measurable");
        let admitted = |limit| actual <= limit;

        assert_eq!(actual, expected);
        assert!(actual > 0);
        assert!(admitted(actual));
        assert!(!admitted(actual - 1));
    }

    #[test]
    fn build_request_measurement_fails_closed_for_other_command_or_envelope() {
        let other = AppRequest::new(AppCommand::Rules(RulesAppCommand::new("list")));
        assert_eq!(
            other.checked_build_probability_retained_capacity_bytes(),
            None
        );

        let mismatched = AppRequest::new(AppCommand::BuildProbability(
            BuildProbabilityAppCommand::new(build_query()),
        ))
        .with_query_envelope_for_test(QueryEnvelope::Damage);
        assert_eq!(
            mismatched.checked_build_probability_retained_capacity_bytes(),
            None
        );
    }

    #[test]
    fn build_request_capacity_sum_fails_closed_on_overflow() {
        assert_eq!(
            checked_build_probability_retained_capacity_sum([u128::MAX, 0, 0, 0]),
            Some(u128::MAX)
        );
        assert_eq!(
            checked_build_probability_retained_capacity_sum([u128::MAX, 1, 0, 0]),
            None
        );
    }
}
