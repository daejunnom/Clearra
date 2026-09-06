// SRP rationale: this module has one behavior-level change reason: validating public product capability requests into query-bound application authority.

use std::{fmt, sync::Arc};

use clearra_host_contract::{AppCommandKind, QueryEnvelope};
use clearra_pc_graph::request::{OpeningPcSearchQuery, PcScenarioQuery};

use crate::{
    app_command::AppCommand,
    pc_chance_probability_result::{PcChanceIngressOrigin, PcChanceQuerySnapshot},
    pc_failed_queue_result::{PcFailedQueueIngressOrigin, PcFailedQueueQuerySnapshot},
    pc_minimum_cover_result::{PcMinimalsIngressOrigin, PcMinimumCoverQueryBinding},
    pc_path_result::{PcPathIngressOrigin, PcPathQueryBinding},
    pc_result_projection::{PcResultProjection, ValidatedPcResultProjection},
    pc_save_result::{PcSaveIngressOrigin, PcSaveQuerySnapshot},
    pc_score_minimum_cover_result::PcScoreMinimalsIngressOrigin,
    pc_score_summary_result::{PcScoreIngressOrigin, PcScoreQuerySnapshot},
    pc_tiling_family_result::{PcTilingIngressOrigin, PcTilingQueryBinding},
    render::AppResultKind,
};

/// Typed identity for a product capability whose request and result contracts
/// must be validated together.
///
/// This is deliberately not an open string. New product families must add a
/// typed variant and a fieldwise validator before Web or App code can attach
/// their identity to a request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductCapabilityContract {
    PcTiling,
    PcSaves,
    PcBestSave,
    PcMinimals,
    PcPath,
    PcChance,
    PcFailedQueue,
    PcScore,
    PcScoreFinder,
    PcScoreMinimals,
    PcAllSpinSolution,
    PcAllSpinPreservationChance,
    BuildCover,
    BuildSetup,
}

impl ProductCapabilityContract {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PcTiling => "pc.tiling",
            Self::PcSaves => "pc.saves",
            Self::PcBestSave => "pc.best-save",
            Self::PcMinimals => "pc.minimals",
            Self::PcPath => "pc.path",
            Self::PcChance => "pc.chance",
            Self::PcFailedQueue => "pc.failed-queue",
            Self::PcScore => "pc.score",
            Self::PcScoreFinder => "pc.score-finder",
            Self::PcScoreMinimals => "pc.score-minimals",
            Self::PcAllSpinSolution => "pc.allspin-sol",
            Self::PcAllSpinPreservationChance => "pc.allspin-pres-chance",
            Self::BuildCover => "build.cover",
            Self::BuildSetup => "build.setup",
        }
    }

    pub(crate) fn validate_request(
        self,
        command: &AppCommand,
        query: &QueryEnvelope,
    ) -> Result<ValidatedProductCapabilityContract, ProductCapabilityContractError> {
        let actual_query = command.query_envelope();
        if query != &actual_query {
            return Err(ProductCapabilityContractError::QueryEnvelopeMismatch {
                expected: actual_query,
                actual: query.clone(),
            });
        }

        if self == Self::PcFailedQueue {
            return self.validate_pc_failed_queue_request(command, actual_query);
        }

        let (expected_result_kind, expected_problem_preset, payload) = match command {
            AppCommand::Pc(command) => {
                let projection = command
                    .validated_result_projection()
                    .map_err(ProductCapabilityContractError::RequestContractRejected)?;
                let payload = if projection.projection().tiling_origin().is_some() {
                    ValidatedProductCapabilityPayload::PcTilingOpening {
                        query: command.query_arc(),
                        projection,
                    }
                } else if projection.projection().save_origin().is_some() {
                    ValidatedProductCapabilityPayload::PcSaveOpening {
                        query: command.query_arc(),
                        projection,
                    }
                } else if projection.projection().minimals_origin().is_some() {
                    ValidatedProductCapabilityPayload::PcMinimalsOpening {
                        query: command.query_arc(),
                        projection,
                    }
                } else if projection.projection().path_origin().is_some() {
                    ValidatedProductCapabilityPayload::PcPathOpening {
                        query: command.query_arc(),
                        projection,
                    }
                } else if projection.projection().chance_origin().is_some() {
                    ValidatedProductCapabilityPayload::PcChanceOpening {
                        query: command.query().clone(),
                        projection,
                    }
                } else if projection.projection().score_origin().is_some() {
                    ValidatedProductCapabilityPayload::PcScoreOpening {
                        query: command.query_arc(),
                        projection,
                    }
                } else if projection.projection().score_minimals_origin().is_some() {
                    ValidatedProductCapabilityPayload::PcScoreMinimalsOpening {
                        query: command.query_arc(),
                        projection,
                    }
                } else {
                    ValidatedProductCapabilityPayload::PcAllSpinOpening {
                        query: command.query().clone(),
                        projection,
                    }
                };
                (
                    AppResultKind::Pc,
                    ProductCapabilityProblemPreset::OpeningPc,
                    payload,
                )
            }
            AppCommand::Scenario(command) => {
                let projection = command
                    .validated_result_projection()
                    .map_err(ProductCapabilityContractError::RequestContractRejected)?;
                let payload = if projection.projection().tiling_origin().is_some() {
                    ValidatedProductCapabilityPayload::PcTilingScenario {
                        query: command.query_arc(),
                        projection,
                    }
                } else if projection.projection().save_origin().is_some() {
                    ValidatedProductCapabilityPayload::PcSaveScenario {
                        query: command.query_arc(),
                        projection,
                    }
                } else if projection.projection().minimals_origin().is_some() {
                    ValidatedProductCapabilityPayload::PcMinimalsScenario {
                        query: command.query_arc(),
                        projection,
                    }
                } else if projection.projection().path_origin().is_some() {
                    ValidatedProductCapabilityPayload::PcPathScenario {
                        query: command.query_arc(),
                        projection,
                    }
                } else if projection.projection().chance_origin().is_some() {
                    ValidatedProductCapabilityPayload::PcChanceScenario {
                        query: command.query().clone(),
                        projection,
                    }
                } else if projection.projection().score_origin().is_some() {
                    ValidatedProductCapabilityPayload::PcScoreScenario {
                        query: command.query_arc(),
                        projection,
                    }
                } else if projection.projection().score_minimals_origin().is_some() {
                    ValidatedProductCapabilityPayload::PcScoreMinimalsScenario {
                        query: command.query_arc(),
                        projection,
                    }
                } else {
                    ValidatedProductCapabilityPayload::PcAllSpinScenario {
                        query: command.query().clone(),
                        projection,
                    }
                };
                (
                    AppResultKind::Scenario,
                    ProductCapabilityProblemPreset::ScenarioPc,
                    payload,
                )
            }
            _ => {
                return Err(ProductCapabilityContractError::CommandFamilyMismatch {
                    contract: self,
                    actual: command.kind(),
                })
            }
        };

        let projection = payload.projection();
        if self == Self::PcScoreFinder
            && !matches!(
                &payload,
                ValidatedProductCapabilityPayload::PcScoreScenario { .. }
            )
        {
            return Err(ProductCapabilityContractError::RequestContractRejected(
                "pc.score-finder requires an explicit initial-field scenario",
            ));
        }
        if !self.accepts_projection(projection) {
            return Err(ProductCapabilityContractError::ProjectionMismatch {
                contract: self,
                actual: projection,
            });
        }

        Ok(ValidatedProductCapabilityContract {
            contract: self,
            command_kind: command.kind(),
            query: actual_query,
            expected_result_kind,
            expected_problem_preset,
            payload,
        })
    }

    pub(crate) fn required_for_command(command: &AppCommand) -> Option<Self> {
        if let AppCommand::Percent(command) = command {
            return command
                .pc_failed_queue_origin()
                .map(|_| Self::PcFailedQueue);
        }
        let projection = match command {
            AppCommand::Pc(command) => command.result_projection(),
            AppCommand::Scenario(command) => command.result_projection(),
            _ => return None,
        };
        match projection {
            PcResultProjection::Standard => None,
            PcResultProjection::TilingFamilyV1(_) => Some(Self::PcTiling),
            PcResultProjection::SaveGroupsV2(_) => Some(Self::PcSaves),
            PcResultProjection::BestSaveV2(_) => Some(Self::PcBestSave),
            PcResultProjection::MinimumCoverV2(_) => Some(Self::PcMinimals),
            PcResultProjection::PathFamilyV2(_) => Some(Self::PcPath),
            PcResultProjection::ChanceProbabilityV2(_) => Some(Self::PcChance),
            PcResultProjection::ScoreSummaryV2(origin) => Some(if origin.is_score_finder() {
                Self::PcScoreFinder
            } else {
                Self::PcScore
            }),
            PcResultProjection::ScorePortfolioV2(_) => Some(Self::PcScoreMinimals),
            PcResultProjection::AllSpinSolution(_) => Some(Self::PcAllSpinSolution),
            PcResultProjection::AllSpinPreservationChance(_) => {
                Some(Self::PcAllSpinPreservationChance)
            }
        }
    }

    const fn accepts_projection(self, projection: PcResultProjection) -> bool {
        if let PcResultProjection::ScoreSummaryV2(origin) = projection {
            return match self {
                Self::PcScore => !origin.is_score_finder(),
                Self::PcScoreFinder => origin.is_score_finder(),
                _ => false,
            };
        }
        matches!(
            (self, projection),
            (Self::PcTiling, PcResultProjection::TilingFamilyV1(_))
                | (Self::PcSaves, PcResultProjection::SaveGroupsV2(_))
                | (Self::PcBestSave, PcResultProjection::BestSaveV2(_))
                | (Self::PcMinimals, PcResultProjection::MinimumCoverV2(_))
                | (Self::PcPath, PcResultProjection::PathFamilyV2(_))
                | (Self::PcChance, PcResultProjection::ChanceProbabilityV2(_))
                | (
                    Self::PcScoreMinimals,
                    PcResultProjection::ScorePortfolioV2(_)
                )
                | (
                    Self::PcAllSpinSolution,
                    PcResultProjection::AllSpinSolution(_)
                )
                | (
                    Self::PcAllSpinPreservationChance,
                    PcResultProjection::AllSpinPreservationChance(_)
                )
        )
    }

    fn validate_pc_failed_queue_request(
        self,
        command: &AppCommand,
        actual_query: QueryEnvelope,
    ) -> Result<ValidatedProductCapabilityContract, ProductCapabilityContractError> {
        let AppCommand::Percent(command) = command else {
            return Err(ProductCapabilityContractError::CommandFamilyMismatch {
                contract: self,
                actual: command.kind(),
            });
        };
        let origin = command.pc_failed_queue_origin().ok_or(
            ProductCapabilityContractError::RequestContractRejected(
                "pc.failed-queue requires a typed failed-queue command",
            ),
        )?;
        let (expected_problem_preset, payload) = if let Some(query) = command.opening_query() {
            (
                ProductCapabilityProblemPreset::OpeningPc,
                ValidatedProductCapabilityPayload::PcFailedQueueOpening {
                    query: query.clone(),
                    origin,
                    failed_pattern_limit: command.failed_pattern_limit(),
                },
            )
        } else if let Some(query) = command.query() {
            (
                ProductCapabilityProblemPreset::ScenarioPc,
                ValidatedProductCapabilityPayload::PcFailedQueueScenario {
                    query: query.clone(),
                    origin,
                    failed_pattern_limit: command.failed_pattern_limit(),
                },
            )
        } else {
            return Err(ProductCapabilityContractError::RequestContractRejected(
                "pc.failed-queue query is unavailable",
            ));
        };
        Ok(ValidatedProductCapabilityContract {
            contract: self,
            command_kind: AppCommandKind::Percent,
            query: actual_query,
            expected_result_kind: AppResultKind::Percent,
            expected_problem_preset,
            payload,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProductCapabilityProblemPreset {
    OpeningPc,
    ScenarioPc,
}

impl ProductCapabilityProblemPreset {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::OpeningPc => "opening-pc",
            Self::ScenarioPc => "scenario-pc",
        }
    }

    pub(crate) const fn initial_field_supplied(self) -> bool {
        matches!(self, Self::ScenarioPc)
    }
}

/// Opaque proof that the immutable App command, query envelope, objective,
/// preset and result projection agree with a typed product capability.
///
/// Constructors are crate-private. Execution paths move this proof into their
/// exactly-once response finalization point.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ValidatedProductCapabilityContract {
    contract: ProductCapabilityContract,
    command_kind: AppCommandKind,
    query: QueryEnvelope,
    expected_result_kind: AppResultKind,
    expected_problem_preset: ProductCapabilityProblemPreset,
    payload: ValidatedProductCapabilityPayload,
}

/// Closed typed payload carried by the common proof. Each product family adds
/// its own validator-owned variant instead of borrowing PC proof fields or
/// falling back to string metadata.
// The `Pc` prefix mirrors the published product-capability identifiers and
// keeps each closed proof variant searchable at this cross-product boundary.
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Debug, Eq, PartialEq)]
enum ValidatedProductCapabilityPayload {
    PcTilingOpening {
        query: Arc<OpeningPcSearchQuery>,
        projection: ValidatedPcResultProjection,
    },
    PcTilingScenario {
        query: Arc<PcScenarioQuery>,
        projection: ValidatedPcResultProjection,
    },
    PcSaveOpening {
        query: Arc<OpeningPcSearchQuery>,
        projection: ValidatedPcResultProjection,
    },
    PcSaveScenario {
        query: Arc<PcScenarioQuery>,
        projection: ValidatedPcResultProjection,
    },
    PcMinimalsOpening {
        query: Arc<OpeningPcSearchQuery>,
        projection: ValidatedPcResultProjection,
    },
    PcMinimalsScenario {
        query: Arc<PcScenarioQuery>,
        projection: ValidatedPcResultProjection,
    },
    PcPathOpening {
        query: Arc<OpeningPcSearchQuery>,
        projection: ValidatedPcResultProjection,
    },
    PcPathScenario {
        query: Arc<PcScenarioQuery>,
        projection: ValidatedPcResultProjection,
    },
    PcFailedQueueOpening {
        query: OpeningPcSearchQuery,
        origin: PcFailedQueueIngressOrigin,
        failed_pattern_limit: usize,
    },
    PcFailedQueueScenario {
        query: PcScenarioQuery,
        origin: PcFailedQueueIngressOrigin,
        failed_pattern_limit: usize,
    },
    PcChanceOpening {
        query: OpeningPcSearchQuery,
        projection: ValidatedPcResultProjection,
    },
    PcChanceScenario {
        query: PcScenarioQuery,
        projection: ValidatedPcResultProjection,
    },
    PcScoreOpening {
        query: Arc<OpeningPcSearchQuery>,
        projection: ValidatedPcResultProjection,
    },
    PcScoreScenario {
        query: Arc<PcScenarioQuery>,
        projection: ValidatedPcResultProjection,
    },
    PcScoreMinimalsOpening {
        query: Arc<OpeningPcSearchQuery>,
        projection: ValidatedPcResultProjection,
    },
    PcScoreMinimalsScenario {
        query: Arc<PcScenarioQuery>,
        projection: ValidatedPcResultProjection,
    },
    PcAllSpinOpening {
        query: OpeningPcSearchQuery,
        projection: ValidatedPcResultProjection,
    },
    PcAllSpinScenario {
        query: PcScenarioQuery,
        projection: ValidatedPcResultProjection,
    },
}

/// Allocation-free view of the immutable score query carried by the validated
/// product proof. Final response validation only needs to compare that query
/// with the authority-owned snapshot; rebuilding an owned snapshot here would
/// clone cardinality-dependent queue storage after execution has completed.
pub(crate) enum PcScoreQueryBinding<'a> {
    Opening(&'a OpeningPcSearchQuery),
    Scenario(&'a PcScenarioQuery),
}

/// Allocation-free request view used to compare a validated save product
/// proof with the authority-owned execution snapshot.
pub(crate) enum PcSaveQueryBinding<'a> {
    Opening(&'a OpeningPcSearchQuery),
    Scenario(&'a PcScenarioQuery),
}

impl PcSaveQueryBinding<'_> {
    pub(crate) fn matches_snapshot(&self, snapshot: &PcSaveQuerySnapshot) -> bool {
        match (self, snapshot) {
            (Self::Opening(expected), PcSaveQuerySnapshot::Opening(actual)) => {
                *expected == actual.as_ref()
            }
            (Self::Scenario(expected), PcSaveQuerySnapshot::Scenario(actual)) => {
                *expected == actual.as_ref()
            }
            (Self::Opening(_), PcSaveQuerySnapshot::Scenario(_))
            | (Self::Scenario(_), PcSaveQuerySnapshot::Opening(_)) => false,
        }
    }
}

impl PcScoreQueryBinding<'_> {
    pub(crate) fn matches_snapshot(&self, snapshot: &PcScoreQuerySnapshot) -> bool {
        match (self, snapshot) {
            (Self::Opening(expected), PcScoreQuerySnapshot::Opening(actual)) => {
                *expected == actual.as_ref()
            }
            (Self::Scenario(expected), PcScoreQuerySnapshot::Scenario(actual)) => {
                *expected == actual.as_ref()
            }
            (Self::Opening(_), PcScoreQuerySnapshot::Scenario(_))
            | (Self::Scenario(_), PcScoreQuerySnapshot::Opening(_)) => false,
        }
    }
}

impl ValidatedProductCapabilityPayload {
    fn projection(&self) -> PcResultProjection {
        match self {
            Self::PcTilingOpening { projection, .. }
            | Self::PcTilingScenario { projection, .. }
            | Self::PcSaveOpening { projection, .. }
            | Self::PcSaveScenario { projection, .. }
            | Self::PcMinimalsOpening { projection, .. }
            | Self::PcMinimalsScenario { projection, .. }
            | Self::PcPathOpening { projection, .. }
            | Self::PcPathScenario { projection, .. }
            | Self::PcChanceOpening { projection, .. }
            | Self::PcChanceScenario { projection, .. }
            | Self::PcScoreOpening { projection, .. }
            | Self::PcScoreScenario { projection, .. }
            | Self::PcScoreMinimalsOpening { projection, .. }
            | Self::PcScoreMinimalsScenario { projection, .. }
            | Self::PcAllSpinOpening { projection, .. }
            | Self::PcAllSpinScenario { projection, .. } => projection.projection(),
            Self::PcFailedQueueOpening { .. } | Self::PcFailedQueueScenario { .. } => {
                unreachable!("failed-queue payload has no PC result projection")
            }
        }
    }

    fn pc_save_binding(&self) -> Option<(PcSaveQueryBinding<'_>, PcSaveIngressOrigin)> {
        match self {
            Self::PcSaveOpening { query, projection } => Some((
                PcSaveQueryBinding::Opening(query.as_ref()),
                projection
                    .projection()
                    .save_origin()
                    .expect("pc save opening payload carries save projection"),
            )),
            Self::PcSaveScenario { query, projection } => Some((
                PcSaveQueryBinding::Scenario(query.as_ref()),
                projection
                    .projection()
                    .save_origin()
                    .expect("pc save scenario payload carries save projection"),
            )),
            _ => None,
        }
    }

    fn pc_tiling_binding(&self) -> Option<(PcTilingQueryBinding<'_>, PcTilingIngressOrigin)> {
        match self {
            Self::PcTilingOpening { query, projection } => Some((
                PcTilingQueryBinding::Opening(query.as_ref()),
                projection
                    .projection()
                    .tiling_origin()
                    .expect("pc tiling opening payload carries tiling projection"),
            )),
            Self::PcTilingScenario { query, projection } => Some((
                PcTilingQueryBinding::Scenario(query.as_ref()),
                projection
                    .projection()
                    .tiling_origin()
                    .expect("pc tiling scenario payload carries tiling projection"),
            )),
            _ => None,
        }
    }

    fn pc_minimum_cover_binding(
        &self,
    ) -> Option<(PcMinimumCoverQueryBinding<'_>, PcMinimalsIngressOrigin)> {
        match self {
            Self::PcMinimalsOpening { query, projection } => Some((
                PcMinimumCoverQueryBinding::Opening(query),
                projection
                    .projection()
                    .minimals_origin()
                    .expect("pc minimals opening payload carries minimum-cover projection"),
            )),
            Self::PcMinimalsScenario { query, projection } => Some((
                PcMinimumCoverQueryBinding::Scenario(query),
                projection
                    .projection()
                    .minimals_origin()
                    .expect("pc minimals scenario payload carries minimum-cover projection"),
            )),
            _ => None,
        }
    }

    fn pc_path_binding(&self) -> Option<(PcPathQueryBinding<'_>, PcPathIngressOrigin)> {
        match self {
            Self::PcPathOpening { query, projection } => Some((
                PcPathQueryBinding::Opening(query),
                projection
                    .projection()
                    .path_origin()
                    .expect("pc path opening payload carries path projection"),
            )),
            Self::PcPathScenario { query, projection } => Some((
                PcPathQueryBinding::Scenario(query),
                projection
                    .projection()
                    .path_origin()
                    .expect("pc path scenario payload carries path projection"),
            )),
            _ => None,
        }
    }

    fn pc_chance_binding(&self) -> Option<(PcChanceQuerySnapshot, PcChanceIngressOrigin)> {
        match self {
            Self::PcChanceOpening { query, projection } => Some((
                PcChanceQuerySnapshot::Opening(query.clone()),
                projection
                    .projection()
                    .chance_origin()
                    .expect("pc chance opening payload carries chance projection"),
            )),
            Self::PcChanceScenario { query, projection } => Some((
                PcChanceQuerySnapshot::Scenario(query.clone()),
                projection
                    .projection()
                    .chance_origin()
                    .expect("pc chance scenario payload carries chance projection"),
            )),
            Self::PcScoreOpening { .. }
            | Self::PcScoreScenario { .. }
            | Self::PcScoreMinimalsOpening { .. }
            | Self::PcScoreMinimalsScenario { .. }
            | Self::PcSaveOpening { .. }
            | Self::PcSaveScenario { .. }
            | Self::PcMinimalsOpening { .. }
            | Self::PcMinimalsScenario { .. }
            | Self::PcPathOpening { .. }
            | Self::PcPathScenario { .. }
            | Self::PcTilingOpening { .. }
            | Self::PcTilingScenario { .. }
            | Self::PcAllSpinOpening { .. }
            | Self::PcAllSpinScenario { .. }
            | Self::PcFailedQueueOpening { .. }
            | Self::PcFailedQueueScenario { .. } => None,
        }
    }

    fn pc_score_binding(&self) -> Option<(PcScoreQueryBinding<'_>, PcScoreIngressOrigin)> {
        match self {
            Self::PcScoreOpening { query, projection } => Some((
                PcScoreQueryBinding::Opening(query.as_ref()),
                projection
                    .projection()
                    .score_origin()
                    .expect("pc score opening payload carries score projection"),
            )),
            Self::PcScoreScenario { query, projection } => Some((
                PcScoreQueryBinding::Scenario(query.as_ref()),
                projection
                    .projection()
                    .score_origin()
                    .expect("pc score scenario payload carries score projection"),
            )),
            Self::PcChanceOpening { .. }
            | Self::PcChanceScenario { .. }
            | Self::PcScoreMinimalsOpening { .. }
            | Self::PcScoreMinimalsScenario { .. }
            | Self::PcSaveOpening { .. }
            | Self::PcSaveScenario { .. }
            | Self::PcMinimalsOpening { .. }
            | Self::PcMinimalsScenario { .. }
            | Self::PcPathOpening { .. }
            | Self::PcPathScenario { .. }
            | Self::PcTilingOpening { .. }
            | Self::PcTilingScenario { .. }
            | Self::PcAllSpinOpening { .. }
            | Self::PcAllSpinScenario { .. }
            | Self::PcFailedQueueOpening { .. }
            | Self::PcFailedQueueScenario { .. } => None,
        }
    }

    fn pc_score_minimals_binding(
        &self,
    ) -> Option<(PcScoreQueryBinding<'_>, PcScoreMinimalsIngressOrigin)> {
        match self {
            Self::PcScoreMinimalsOpening { query, projection } => Some((
                PcScoreQueryBinding::Opening(query.as_ref()),
                projection
                    .projection()
                    .score_minimals_origin()
                    .expect("pc score-minimals opening payload carries its distinct projection"),
            )),
            Self::PcScoreMinimalsScenario { query, projection } => Some((
                PcScoreQueryBinding::Scenario(query.as_ref()),
                projection
                    .projection()
                    .score_minimals_origin()
                    .expect("pc score-minimals scenario payload carries its distinct projection"),
            )),
            _ => None,
        }
    }

    fn pc_failed_queue_binding(
        &self,
    ) -> Option<(
        PcFailedQueueQuerySnapshot,
        PcFailedQueueIngressOrigin,
        usize,
    )> {
        match self {
            Self::PcFailedQueueOpening {
                query,
                origin,
                failed_pattern_limit,
            } => Some((
                PcFailedQueueQuerySnapshot::Opening(query.clone()),
                *origin,
                *failed_pattern_limit,
            )),
            Self::PcFailedQueueScenario {
                query,
                origin,
                failed_pattern_limit,
            } => Some((
                PcFailedQueueQuerySnapshot::Scenario(query.clone()),
                *origin,
                *failed_pattern_limit,
            )),
            _ => None,
        }
    }
}

impl ValidatedProductCapabilityContract {
    pub(crate) fn checked_score_minimum_cover_retained_capacity_bytes(&self) -> Option<u128> {
        if self.contract != ProductCapabilityContract::PcScoreMinimals {
            return None;
        }
        let query = match &self.payload {
            ValidatedProductCapabilityPayload::PcScoreMinimalsOpening { query, .. } => {
                PcScoreQuerySnapshot::Opening(Arc::clone(query))
            }
            ValidatedProductCapabilityPayload::PcScoreMinimalsScenario { query, .. } => {
                PcScoreQuerySnapshot::Scenario(Arc::clone(query))
            }
            _ => return None,
        };
        query
            .checked_pointee_retained_bytes()?
            .checked_add((2 * core::mem::size_of::<usize>()) as u128)
    }

    /// Heap pointees retained by the closed pc.minimals proof. Other product
    /// payloads deliberately return None instead of silently omitting owners.
    pub(crate) fn checked_minimum_cover_retained_capacity_bytes(&self) -> Option<u128> {
        if self.contract != ProductCapabilityContract::PcMinimals {
            return None;
        }
        match &self.payload {
            ValidatedProductCapabilityPayload::PcMinimalsOpening { query, .. } => {
                crate::pc_minimum_cover_result::checked_minimum_opening_query_retained_bytes(query)
            }
            ValidatedProductCapabilityPayload::PcMinimalsScenario { query, .. } => {
                crate::pc_minimum_cover_result::checked_minimum_scenario_query_retained_bytes(query)
            }
            _ => None,
        }
    }

    pub(crate) const fn contract(&self) -> ProductCapabilityContract {
        self.contract
    }

    #[cfg(test)]
    pub(crate) fn pc_score_query_snapshot_for_test(&self) -> Option<PcScoreQuerySnapshot> {
        match &self.payload {
            ValidatedProductCapabilityPayload::PcScoreOpening { query, .. } => {
                Some(PcScoreQuerySnapshot::Opening(Arc::clone(query)))
            }
            ValidatedProductCapabilityPayload::PcScoreScenario { query, .. } => {
                Some(PcScoreQuerySnapshot::Scenario(Arc::clone(query)))
            }
            ValidatedProductCapabilityPayload::PcScoreMinimalsOpening { query, .. } => {
                Some(PcScoreQuerySnapshot::Opening(Arc::clone(query)))
            }
            ValidatedProductCapabilityPayload::PcScoreMinimalsScenario { query, .. } => {
                Some(PcScoreQuerySnapshot::Scenario(Arc::clone(query)))
            }
            _ => None,
        }
    }

    pub(crate) const fn command_kind(&self) -> AppCommandKind {
        self.command_kind
    }

    pub(crate) fn query(&self) -> &QueryEnvelope {
        &self.query
    }

    pub(crate) const fn expected_result_kind(&self) -> AppResultKind {
        self.expected_result_kind
    }

    pub(crate) const fn expected_problem_preset(&self) -> ProductCapabilityProblemPreset {
        self.expected_problem_preset
    }

    pub(crate) fn pc_allspin_projection(&self) -> Option<PcResultProjection> {
        if matches!(
            &self.payload,
            ValidatedProductCapabilityPayload::PcFailedQueueOpening { .. }
                | ValidatedProductCapabilityPayload::PcFailedQueueScenario { .. }
        ) {
            return None;
        }
        let projection = self.payload.projection();
        if projection.chance_origin().is_some()
            || projection.minimals_origin().is_some()
            || projection.path_origin().is_some()
            || projection.score_origin().is_some()
            || projection.score_minimals_origin().is_some()
            || projection.tiling_origin().is_some()
            || projection.save_origin().is_some()
        {
            None
        } else {
            Some(projection)
        }
    }

    pub(crate) fn pc_tiling_binding(
        &self,
    ) -> Option<(PcTilingQueryBinding<'_>, PcTilingIngressOrigin)> {
        self.payload.pc_tiling_binding()
    }

    pub(crate) fn pc_save_binding(&self) -> Option<(PcSaveQueryBinding<'_>, PcSaveIngressOrigin)> {
        self.payload.pc_save_binding()
    }

    pub(crate) fn pc_minimum_cover_binding(
        &self,
    ) -> Option<(PcMinimumCoverQueryBinding<'_>, PcMinimalsIngressOrigin)> {
        self.payload.pc_minimum_cover_binding()
    }

    pub(crate) fn pc_path_binding(&self) -> Option<(PcPathQueryBinding<'_>, PcPathIngressOrigin)> {
        self.payload.pc_path_binding()
    }

    pub(crate) fn pc_chance_binding(
        &self,
    ) -> Option<(PcChanceQuerySnapshot, PcChanceIngressOrigin)> {
        self.payload.pc_chance_binding()
    }

    pub(crate) fn pc_score_binding(
        &self,
    ) -> Option<(PcScoreQueryBinding<'_>, PcScoreIngressOrigin)> {
        self.payload.pc_score_binding()
    }

    pub(crate) fn pc_score_minimals_binding(
        &self,
    ) -> Option<(PcScoreQueryBinding<'_>, PcScoreMinimalsIngressOrigin)> {
        self.payload.pc_score_minimals_binding()
    }

    pub(crate) fn pc_failed_queue_binding(
        &self,
    ) -> Option<(
        PcFailedQueueQuerySnapshot,
        PcFailedQueueIngressOrigin,
        usize,
    )> {
        self.payload.pc_failed_queue_binding()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProductCapabilityContractError {
    RequiredContractMissing {
        required: ProductCapabilityContract,
    },
    RequiredContractMismatch {
        required: ProductCapabilityContract,
        actual: ProductCapabilityContract,
    },
    UnexpectedContract {
        actual: ProductCapabilityContract,
    },
    StaleRequestContract {
        contract: ProductCapabilityContract,
    },
    QueryEnvelopeMismatch {
        expected: QueryEnvelope,
        actual: QueryEnvelope,
    },
    CommandFamilyMismatch {
        contract: ProductCapabilityContract,
        actual: AppCommandKind,
    },
    ProjectionMismatch {
        contract: ProductCapabilityContract,
        actual: PcResultProjection,
    },
    RequestContractRejected(&'static str),
    ResponseStatusNotSuccessful,
    ResponseAlreadyWrapped,
    ResponseCommandMismatch,
    ResponseResultMissing,
    ResponseResultKindMismatch,
    ResponseRenderModelMissing,
    ResponseRenderKindMismatch,
    ResponseRenderFamilyMismatch,
    ResponseResultContractMismatch,
    ResponseResultProjectionIncomplete,
    ResponseProblemPresetMismatch,
    ResponseTilingEvidenceMissing,
    ResponseTilingEvidenceMismatch,
    ResponseSaveEvidenceMissing,
    ResponseSaveEvidenceMismatch,
    ResponseMinimumCoverEvidenceMismatch(&'static str),
    ResponsePathEvidenceMismatch(&'static str),
    ResponseChanceEvidenceMissing,
    ResponseChanceEvidenceMismatch,
    ResponseFailedQueueEvidenceMissing,
    ResponseFailedQueueEvidenceMismatch,
    ResponseScoreEvidenceMissing,
    ResponseScoreEvidenceMismatch,
    ResponseScorePortfolioEvidenceMissing,
    ResponseScorePortfolioEvidenceMismatch,
    ResourceProbabilityIncomplete,
    SolverNotExecuted,
    ExecutionUnavailable,
    AvailabilityReasonPresent,
    ResultIncomplete,
    ResultTruncated,
    TruncationReasonPresent,
}

impl fmt::Display for ProductCapabilityContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RequiredContractMissing { required } => write!(
                formatter,
                "{} result projection requires its matching product capability contract",
                required.as_str()
            ),
            Self::RequiredContractMismatch { required, actual } => write!(
                formatter,
                "{} result projection cannot use {} product capability proof",
                required.as_str(),
                actual.as_str()
            ),
            Self::UnexpectedContract { actual } => write!(
                formatter,
                "standard result projection cannot inherit {} product capability proof",
                actual.as_str()
            ),
            Self::StaleRequestContract { contract } => write!(
                formatter,
                "{} product capability proof is stale for the current command",
                contract.as_str()
            ),
            Self::QueryEnvelopeMismatch { expected, actual } => write!(
                formatter,
                "query envelope mismatch: expected {}, got {}",
                expected.as_str(),
                actual.as_str()
            ),
            Self::CommandFamilyMismatch { contract, actual } => write!(
                formatter,
                "{} does not accept AppCommandKind::{actual:?}",
                contract.as_str()
            ),
            Self::ProjectionMismatch { contract, actual } => write!(
                formatter,
                "{} does not accept result projection {actual:?}",
                contract.as_str()
            ),
            Self::RequestContractRejected(reason) => {
                write!(formatter, "request contract rejected: {reason}")
            }
            Self::ResponseStatusNotSuccessful => formatter.write_str("response is not successful"),
            Self::ResponseAlreadyWrapped => {
                formatter.write_str("response already has a product capability wrapper")
            }
            Self::ResponseCommandMismatch => formatter.write_str("response command kind mismatch"),
            Self::ResponseResultMissing => formatter.write_str("response result is missing"),
            Self::ResponseResultKindMismatch => {
                formatter.write_str("response result kind mismatch")
            }
            Self::ResponseRenderModelMissing => {
                formatter.write_str("response render model is missing")
            }
            Self::ResponseRenderKindMismatch => {
                formatter.write_str("response render kind mismatch")
            }
            Self::ResponseRenderFamilyMismatch => {
                formatter.write_str("response render family mismatch")
            }
            Self::ResponseResultContractMismatch => {
                formatter.write_str("target result contract mismatch")
            }
            Self::ResponseResultProjectionIncomplete => {
                formatter.write_str("target result projection is incomplete")
            }
            Self::ResponseProblemPresetMismatch => {
                formatter.write_str("target result problem preset mismatch")
            }
            Self::ResponseTilingEvidenceMissing => {
                formatter.write_str("pc tiling execution evidence is missing")
            }
            Self::ResponseTilingEvidenceMismatch => formatter
                .write_str("pc tiling execution evidence does not match the request or result"),
            Self::ResponseSaveEvidenceMissing => {
                formatter.write_str("pc save execution evidence is missing")
            }
            Self::ResponseSaveEvidenceMismatch => formatter
                .write_str("pc save execution evidence does not match the request or result"),
            Self::ResponseMinimumCoverEvidenceMismatch(reason) => write!(
                formatter,
                "pc minimum-cover execution evidence does not match the request or result: {reason}"
            ),
            Self::ResponsePathEvidenceMismatch(reason) => write!(
                formatter,
                "pc path execution evidence does not match the request or result: {reason}"
            ),
            Self::ResponseChanceEvidenceMissing => {
                formatter.write_str("pc chance execution evidence is missing")
            }
            Self::ResponseChanceEvidenceMismatch => formatter
                .write_str("pc chance execution evidence does not match the request or result"),
            Self::ResponseFailedQueueEvidenceMissing => {
                formatter.write_str("pc failed-queue execution evidence is missing")
            }
            Self::ResponseFailedQueueEvidenceMismatch => formatter.write_str(
                "pc failed-queue execution evidence does not match the request or result",
            ),
            Self::ResponseScoreEvidenceMissing => {
                formatter.write_str("pc score execution evidence is missing")
            }
            Self::ResponseScoreEvidenceMismatch => formatter
                .write_str("pc score execution evidence does not match the request or result"),
            Self::ResponseScorePortfolioEvidenceMissing => {
                formatter.write_str("pc score-minimals execution evidence is missing")
            }
            Self::ResponseScorePortfolioEvidenceMismatch => formatter.write_str(
                "pc score-minimals execution evidence does not match the request or result",
            ),
            Self::ResourceProbabilityIncomplete => {
                formatter.write_str("resource probability result is incomplete")
            }
            Self::SolverNotExecuted => formatter.write_str("solver was not executed"),
            Self::ExecutionUnavailable => formatter.write_str("execution is not available"),
            Self::AvailabilityReasonPresent => {
                formatter.write_str("available execution carries a failure reason")
            }
            Self::ResultIncomplete => formatter.write_str("result is not complete"),
            Self::ResultTruncated => formatter.write_str("result is truncated"),
            Self::TruncationReasonPresent => {
                formatter.write_str("non-truncated result carries a truncation reason")
            }
        }
    }
}

impl std::error::Error for ProductCapabilityContractError {}
