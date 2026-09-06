// This module is the Discord ingress and presentation projection for product
// capabilities. Executable search and utility semantics are owned by the CLI
// command authority below; Discord-owned meta and settings handlers remain
// explicit. Keep this independent from the JSON contract fixture: contract
// tests compare both projections field by field so either side detects drift.

export const PRODUCT_CAPABILITY_REGISTRY_VERSION =
  "clearra.product-capability-registry.v1";
export const CLI_COMMAND_LOWERING_AUTHORITY = "clearra.cli-command.v1";
export const CLI_COMPATIBILITY_LOWERING_AUTHORITY =
  "clearra.cli-command.compatibility.v1";

const CURRENT_CAPABILITY_ROWS = [
  { id: "meta.help", kind: "meta", status: "active", path: ["help"], ingress: { slash: true, text: true }, problemContractId: "help-query.v1", inputSchemaId: "help-query.v1", modalSchemaId: null, resultContractId: "help-document.v1", algorithmFamily: "utility", timeoutClass: "utility_bounded", effectClasses: ["representation_only"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: [], loweringAuthority: "discord.help-registry-query.v1", engine: null },
  { id: "settings.channel", kind: "configuration", status: "active", path: ["channel-settings"], ingress: { slash: true, text: false }, problemContractId: "discord-channel-settings.v1", inputSchemaId: "channel-settings.v1", modalSchemaId: null, resultContractId: "settings-ack.v1", algorithmFamily: "utility", timeoutClass: "utility_bounded", effectClasses: ["configuration", "external_state"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: [], loweringAuthority: "discord.channel-settings-handler.v1", engine: null },
  { id: "settings.server", kind: "configuration", status: "active", path: ["server-settings"], ingress: { slash: true, text: false }, problemContractId: "discord-server-settings.v1", inputSchemaId: "server-settings.v1", modalSchemaId: null, resultContractId: "settings-ack.v1", algorithmFamily: "utility", timeoutClass: "utility_bounded", effectClasses: ["configuration", "external_state"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: [], loweringAuthority: "discord.server-settings-handler.v1", engine: null },
  { id: "pc.path", kind: "search", status: "active", path: ["pc", "path"], ingress: { slash: true, text: true }, problemContractId: "pc-clear-to-empty.v2", inputSchemaId: "pc-pattern.v2", modalSchemaId: "pc-pattern.v2", resultContractId: "pc-path-family.v2", algorithmFamily: "pc_inverse_lock_clear", timeoutClass: "pc_reverse", effectClasses: ["search_space", "supply_semantics", "reachability_semantics", "result_materialization"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["pc-path-family.v2"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "pc-path-v2", argvPrefix: ["pc", "path"], fixedSemantics: { objective: "all", solutionIdentity: "all", discordSelection: "smallest-canonical-candidate-id" } } },
  { id: "pc.chance", kind: "search", status: "active", path: ["pc", "chance"], ingress: { slash: true, text: true }, problemContractId: "pc-clear-to-empty.v2", inputSchemaId: "pc-pattern.v2", modalSchemaId: "pc-pattern.v2", resultContractId: "pc-probability.v2", algorithmFamily: "pc_inverse_lock_clear", timeoutClass: "pc_reverse", effectClasses: ["search_space", "supply_semantics", "probability_semantics"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["pc-probability.v2"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "pc-chance-v2", argvPrefix: ["pc", "chance"], fixedSemantics: { solutionIdentity: "unique", queueKnowledge: "full-oracle" } } },
  { id: "pc.minimals", kind: "search", status: "active", path: ["pc", "minimals"], ingress: { slash: true, text: true }, problemContractId: "pc-clear-to-empty.v2", inputSchemaId: "pc-pattern.v2", modalSchemaId: "pc-pattern.v2", resultContractId: "pc-minimum-cover.v2", algorithmFamily: "pc_inverse_lock_clear", timeoutClass: "pc_reverse", effectClasses: ["search_space", "probability_semantics", "objective_selection", "result_materialization"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["pc-minimum-cover.v2"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "pc-v2", argvPrefix: ["pc", "minimals"], pcObjective: "minimum-cover" } },
  { id: "pc.score", kind: "search", status: "active", path: ["pc", "score"], ingress: { slash: true, text: true }, problemContractId: "pc-clear-to-empty.v2", inputSchemaId: "pc-score.v2", modalSchemaId: "pc-score.v2", resultContractId: "pc-score-summary.v2", algorithmFamily: "pc_inverse_lock_clear", timeoutClass: "pc_reverse", effectClasses: ["search_space", "reachability_semantics", "score_semantics", "objective_selection"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["pc-score-summary.v2"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "pc-score-v2", argvPrefix: ["pc", "score"] } },
  { id: "pc.saves", kind: "search", status: "active", path: ["pc", "saves"], ingress: { slash: true, text: true }, problemContractId: "pc-save-fixed-bag-boundary.v2", inputSchemaId: "pc-save-pattern.v2", modalSchemaId: "pc-save-pattern.v2", resultContractId: "pc-save-groups.v2", algorithmFamily: "pc_inverse_lock_clear", timeoutClass: "pc_reverse", effectClasses: ["search_space", "supply_semantics", "probability_semantics", "result_materialization"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["pc-save-groups.v2"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "pc-save-v2", argvPrefix: ["pc", "saves"], fixedSemantics: { bagBoundary: "fixed", groupIdentity: "terminal-hold-plus-active-bag-remainder", probabilityBasis: "whole-universe-unconditional" } } },
  { id: "pc.best-save", kind: "search", status: "active", path: ["pc", "best-save"], ingress: { slash: true, text: true }, problemContractId: "pc-save-fixed-bag-boundary.v2", inputSchemaId: "pc-save-pattern.v2", modalSchemaId: "pc-save-pattern.v2", resultContractId: "pc-best-save.v2", algorithmFamily: "pc_inverse_lock_clear", timeoutClass: "pc_reverse", effectClasses: ["search_space", "supply_semantics", "probability_semantics", "objective_selection", "result_materialization"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["pc-best-save.v2"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "pc-save-v2", argvPrefix: ["pc", "best-save"], fixedSemantics: { bagBoundary: "fixed", schema: "clearra-save-v1", probabilityBasis: "whole-universe-unconditional", discordTieSelection: "smallest-canonical-candidate-id" } } },
  { id: "pc.score-minimals", kind: "search", status: "active", path: ["pc", "score-minimals"], ingress: { slash: true, text: true }, problemContractId: "pc-clear-to-empty.v2", inputSchemaId: "pc-score.v2", modalSchemaId: "pc-score.v2", resultContractId: "pc-score-portfolio.v2", algorithmFamily: "pc_inverse_lock_clear", timeoutClass: "pc_reverse", effectClasses: ["search_space", "score_semantics", "probability_semantics", "objective_selection"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["pc-score-portfolio.v2"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "pc-score-v2", argvPrefix: ["pc", "score-minimals"], fixedSemantics: { scoreEquality: "score-only", attackRole: "informational-only", discordTieSelection: "smallest-canonical-candidate-id" } } },
  { id: "pc.tiling", kind: "search", status: "active", path: ["pc", "tiling"], ingress: { slash: true, text: true }, problemContractId: "pc-clear-to-empty.v2", inputSchemaId: "pc-pattern.v2", modalSchemaId: "pc-pattern.v2", resultContractId: "pc-tiling-family.v1", algorithmFamily: "pc_inverse_lock_clear", timeoutClass: "pc_reverse", effectClasses: ["search_space", "supply_semantics", "result_materialization"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["pc-tiling-family.v1"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "pc-tiling-v2", argvPrefix: ["pc", "tiling"] } },
  { id: "pc.failed-queue", kind: "search", status: "active", path: ["pc", "failed-queue"], ingress: { slash: true, text: true }, problemContractId: "pc-clear-to-empty.v2", inputSchemaId: "pc-pattern.v2", modalSchemaId: "pc-pattern.v2", resultContractId: "pc-failed-queue.v2", algorithmFamily: "pc_inverse_lock_clear", timeoutClass: "pc_reverse", effectClasses: ["search_space", "probability_semantics", "result_materialization"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["pc-failed-queue.v2"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "pc-failed-v2", argvPrefix: ["pc", "failed-queue"] } },
  { id: "pc.score-finder", kind: "search", status: "active", path: ["pc", "score-finder"], ingress: { slash: true, text: true }, problemContractId: "pc-fixed-queue-score.v2", inputSchemaId: "pc-fixed-score.v2", modalSchemaId: "pc-fixed-score.v2", resultContractId: "pc-fixed-score-witness.v2", algorithmFamily: "pc_inverse_lock_clear", timeoutClass: "pc_reverse", effectClasses: ["search_space", "supply_semantics", "score_semantics", "objective_selection"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["pc-fixed-score-witness.v2"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "pc-score-finder-v2", argvPrefix: ["pc", "score-finder"], fixedSemantics: { scoreProfile: "jstris-ultra", spinProfile: "t-spins", scoreEquality: "score-only", attackRole: "informational-only", discordTieSelection: "smallest-canonical-candidate-id" } } },
  { id: "pc.allspin-sol", kind: "search", status: "active", path: ["pc", "allspin-sol"], ingress: { slash: true, text: true }, problemContractId: "pc-b2b-preservation.v1", inputSchemaId: "pc-allspin-exact-queue.v1", modalSchemaId: "pc-allspin-exact-queue.v1", resultContractId: "pc-b2b-preserving-witness.v1", algorithmFamily: "pc_inverse_lock_clear", timeoutClass: "pc_reverse", effectClasses: ["search_space", "supply_semantics", "result_materialization"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["pc", "pc-scenario"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "pc-allspin-exact-v1", argvPrefix: ["pc", "allspin-sol"] } },
  { id: "pc.allspin-pres-chance", kind: "search", status: "active", path: ["pc", "allspin-pres-chance"], ingress: { slash: true, text: true }, problemContractId: "pc-b2b-preservation.v1", inputSchemaId: "pc-allspin-pattern.v1", modalSchemaId: "pc-allspin-pattern.v1", resultContractId: "pc-b2b-preservation-probability.v1", algorithmFamily: "pc_inverse_lock_clear", timeoutClass: "pc_reverse", effectClasses: ["search_space", "supply_semantics", "result_materialization"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["pc", "pc-scenario"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "pc-allspin-pattern-v1", argvPrefix: ["pc", "allspin-pres-chance"] } },
  { id: "build.cover", kind: "search", status: "active", path: ["build", "cover"], ingress: { slash: true, text: true }, problemContractId: "build-base-target", inputSchemaId: "build-base-target", modalSchemaId: "build-base-target", resultContractId: "build-probability", algorithmFamily: "build_inverse_lock_clear", timeoutClass: "build_long", effectClasses: ["search_space", "reachability_semantics", "score_semantics", "probability_semantics", "objective_selection", "result_materialization"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["build-probability", "build-path-family.v1", "build-field-average-score.v1", "build-fixed-score-witness.v1", "build-coverage-portfolio.v2", "build-probability-score-minimum.v1"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "build-cover", argvPrefix: ["build-probability"], fixedSemantics: { resultAggregationCompatibility: "non-all-requires-buildability", scoreEquality: "score-only", attackRole: "informational-only", discordTieSelection: "smallest-canonical-candidate-id" } } },
  { id: "build.probability", kind: "search", status: "active", path: ["build", "probability"], ingress: { slash: true, text: true }, problemContractId: "build-base-target-probability.v1", inputSchemaId: "build-base-target-probability.v1", modalSchemaId: "build-base-target-probability.v1", resultContractId: "build-probability", algorithmFamily: "build_inverse_lock_clear", timeoutClass: "build_long", effectClasses: ["search_space", "reachability_semantics", "score_semantics", "probability_semantics", "objective_selection", "result_materialization"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["build-probability", "build-path-family.v1", "build-field-average-score.v1", "build-fixed-score-witness.v1", "build-coverage-portfolio.v2", "build-probability-score-minimum.v1"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "build-cover", argvPrefix: ["build-probability"], fixedSemantics: { resultAggregationCompatibility: "non-all-requires-buildability", scoreEquality: "score-only", attackRole: "informational-only", discordTieSelection: "smallest-canonical-candidate-id" } } },
  { id: "build.finesse-score", kind: "search", status: "active", path: ["build", "finesse-score"], ingress: { slash: true, text: true }, problemContractId: "fixed-placement-finesse-score.v2", inputSchemaId: "finesse-score-document.v2", modalSchemaId: "finesse-score-document.v2", resultContractId: "finesse-input-score.v2", algorithmFamily: "fixed_placement_finesse", timeoutClass: "build_long", effectClasses: ["search_space", "reachability_semantics", "result_materialization"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["build-probability"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "finesse-score", argvPrefix: ["finesse", "score"] } },
  { id: "setup.joint", kind: "search", status: "active", path: ["setup", "joint"], ingress: { slash: true, text: true }, problemContractId: "setup-ranking.v2", inputSchemaId: "setup-ranking.v2", modalSchemaId: "setup-ranking.v2", resultContractId: "setup-joint-ranking.v2", algorithmFamily: "setup_inverse_plus_bidirectional_coverage", timeoutClass: "setup_long", effectClasses: ["search_space", "supply_semantics", "probability_semantics", "objective_selection"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["setup-joint-ranking.v2"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "setup-v2", argvPrefix: ["setup", "joint"], setupPriority: "all", fixedSemantics: { discordFamilyLimit: 24 } } },
  { id: "setup.build", kind: "search", status: "active", path: ["setup", "build"], ingress: { slash: true, text: true }, problemContractId: "setup-ranking.v2", inputSchemaId: "setup-ranking.v2", modalSchemaId: "setup-ranking.v2", resultContractId: "setup-build-ranking.v2", algorithmFamily: "setup_inverse_plus_bidirectional_coverage", timeoutClass: "setup_long", effectClasses: ["search_space", "supply_semantics", "probability_semantics", "objective_selection"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["setup-build-ranking.v2"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "setup-v2", argvPrefix: ["setup", "build"], setupPriority: "build", fixedSemantics: { discordFamilyLimit: 24 } } },
  { id: "setup.pc", kind: "search", status: "active", path: ["setup", "pc"], ingress: { slash: true, text: true }, problemContractId: "setup-ranking.v2", inputSchemaId: "setup-ranking.v2", modalSchemaId: "setup-ranking.v2", resultContractId: "setup-pc-ranking.v2", algorithmFamily: "setup_inverse_plus_bidirectional_coverage", timeoutClass: "setup_long", effectClasses: ["search_space", "supply_semantics", "probability_semantics", "objective_selection"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["setup-pc-ranking.v2"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "setup-v2", argvPrefix: ["setup", "pc"], setupPriority: "pc", fixedSemantics: { discordFamilyLimit: 24 } } },
  { id: "setup.score", kind: "search", status: "active", path: ["setup", "score"], ingress: { slash: true, text: true }, problemContractId: "setup-document-score.v1", inputSchemaId: "setup-score-document.v1", modalSchemaId: "setup-score-document.v1", resultContractId: "setup-score-ranking.v1", algorithmFamily: "setup_inverse_plus_bidirectional_coverage", timeoutClass: "setup_long", effectClasses: ["supply_semantics", "probability_semantics", "score_semantics", "objective_selection"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["setup-score-ranking.v1"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "setup-score-v1", argvPrefix: ["setup", "score"], fixedSemantics: { backend: "cpu", backendFallback: false, discordFamilyLimit: 24 } } },
  { id: "forward.spin", kind: "search", status: "active", path: ["forward", "spin"], ingress: { slash: true, text: true }, problemContractId: "ordered-forward-spin-search", inputSchemaId: "forward-spin", modalSchemaId: "forward-spin", resultContractId: "forward-spin", algorithmFamily: "forward_state_expansion", timeoutClass: "forward_long", effectClasses: ["search_space", "reachability_semantics", "result_materialization"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["spin-finder"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "spin", argvPrefix: ["spin-finder"] } },
  { id: "forward.damage", kind: "search", status: "active", path: ["forward", "damage"], ingress: { slash: true, text: true }, problemContractId: "ordered-forward-damage-search", inputSchemaId: "forward-damage-exact-queue", modalSchemaId: "forward-damage-exact-queue", resultContractId: "forward-damage", algorithmFamily: "forward_state_expansion", timeoutClass: "forward_long", effectClasses: ["search_space", "reachability_semantics", "result_materialization"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["damage"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "fixed-next", argvPrefix: ["damage"] } },
  { id: "forward.ren", kind: "search", status: "active", path: ["forward", "ren"], ingress: { slash: true, text: true }, problemContractId: "ordered-forward-ren-search", inputSchemaId: "forward-ren-exact-queue", modalSchemaId: "forward-ren-exact-queue", resultContractId: "forward-ren", algorithmFamily: "forward_state_expansion", timeoutClass: "forward_long", effectClasses: ["search_space", "reachability_semantics", "result_materialization"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["ren"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "forward-ren-v1", argvPrefix: ["ren"] } },
  { id: "spin-structure.search", kind: "search", status: "active", path: ["spin-structure", "search"], ingress: { slash: true, text: true }, problemContractId: "unordered-spin-structure.v2", inputSchemaId: "spin-structure-inventory.v2", modalSchemaId: "spin-structure-inventory.v2", resultContractId: "spin-structure-family.v2", algorithmFamily: "structural_exact", timeoutClass: "structure_long", effectClasses: ["search_space", "reachability_semantics", "score_semantics", "objective_selection", "result_materialization"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["spin-structure-family.v2"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "spin-structure-v2", argvPrefix: ["spin-structure", "search"], fixedSemantics: { discordFamilyLimit: 24 } } },
  { id: "spin-structure.cover", kind: "search", status: "active", path: ["spin-structure", "cover"], ingress: { slash: true, text: true }, problemContractId: "unordered-spin-structure-coverage.v1", inputSchemaId: "spin-structure-cover.v1", modalSchemaId: "spin-structure-cover.v1", resultContractId: "spin-structure-coverage.v1", algorithmFamily: "structural_exact", timeoutClass: "structure_long", effectClasses: ["search_space", "reachability_semantics", "score_semantics", "probability_semantics", "objective_selection"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["spin-structure-coverage.v1"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "spin-structure-cover-v1", argvPrefix: ["spin-structure", "cover"], fixedSemantics: { objective: "min-cover", discordPortfolioSelection: "first-canonical-portfolio", discordFamilyLimit: 24 } } },
  { id: "spin-structure.guaranteed", kind: "search", status: "active", path: ["spin-structure", "guaranteed"], ingress: { slash: true, text: true }, problemContractId: "unordered-guaranteed-spin-structure.v1", inputSchemaId: "spin-structure-guaranteed.v1", modalSchemaId: "spin-structure-guaranteed.v1", resultContractId: "spin-structure-guaranteed.v1", algorithmFamily: "structural_exact", timeoutClass: "structure_long", effectClasses: ["search_space", "reachability_semantics", "score_semantics", "objective_selection", "result_materialization"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["spin-structure-guaranteed.v1"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "spin-structure-guaranteed-v1", argvPrefix: ["spin-structure", "guaranteed"], fixedSemantics: { discordFamilyLimit: 24 } } },
  { id: "utility.sequence", kind: "utility", status: "active", path: ["utility", "sequence"], ingress: { slash: true, text: true }, problemContractId: "operation-sequence.v1", inputSchemaId: "operation-document.v1", modalSchemaId: "utility-operation-document.v1", resultContractId: "operation-sequence.v1", algorithmFamily: "utility", timeoutClass: "utility_bounded", effectClasses: ["representation_only", "result_materialization"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["sequence"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "operation-document-v1", argvPrefix: ["utility", "sequence"] } },
  { id: "utility.sequence-dependencies", kind: "utility", status: "active", path: ["utility", "sequence-dependencies"], ingress: { slash: true, text: true }, problemContractId: "operation-dependencies.v1", inputSchemaId: "operation-document.v1", modalSchemaId: "utility-operation-document.v1", resultContractId: "operation-dependency-report.v1", algorithmFamily: "utility", timeoutClass: "utility_bounded", effectClasses: ["representation_only", "result_materialization"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["sequence-dependencies"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "operation-document-v1", argvPrefix: ["utility", "sequence-dependencies"] } },
  { id: "utility.parity", kind: "utility", status: "active", path: ["utility", "parity"], ingress: { slash: true, text: true }, problemContractId: "field-parity.v1", inputSchemaId: "field-document.v1", modalSchemaId: "utility-field.v1", resultContractId: "parity-report.v1", algorithmFamily: "utility", timeoutClass: "utility_bounded", effectClasses: ["representation_only"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["parity-report.v1"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "field-document-v1", argvPrefix: ["utility", "parity"] } },
  { id: "utility.fumen", kind: "utility", status: "active", path: ["utility", "fumen"], ingress: { slash: true, text: true }, problemContractId: "fumen-transform.v1", inputSchemaId: "fumen-transform.v1", modalSchemaId: "utility-fumen.v1", resultContractId: "fumen-document.v1", algorithmFamily: "utility", timeoutClass: "utility_bounded", effectClasses: ["representation_only", "artifact_encoding"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["field-document.v1", "field-document-set.v1"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "fumen-transform-v1", argvPrefix: ["utility", "fumen"] } },
  { id: "utility.render", kind: "utility", status: "active", path: ["utility", "render"], ingress: { slash: true, text: true }, problemContractId: "field-document-render.v1", inputSchemaId: "field-document.v1", modalSchemaId: "utility-field.v1", resultContractId: "render-artifact.v1", algorithmFamily: "utility", timeoutClass: "utility_bounded", effectClasses: ["representation_only", "artifact_encoding"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["render-artifact.v1"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "render-document-v1", argvPrefix: ["utility", "render"] } },
  { id: "utility.to-gray", kind: "utility", status: "active", path: ["utility", "to-gray"], ingress: { slash: true, text: true }, problemContractId: "field-color-normalization.v1", inputSchemaId: "field-document.v1", modalSchemaId: "utility-field.v1", resultContractId: "field-document.v1", algorithmFamily: "utility", timeoutClass: "utility_bounded", effectClasses: ["representation_only", "artifact_encoding"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["field-document.v1"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "field-document-v1", argvPrefix: ["utility", "to-gray"] } },
  { id: "utility.mirror", kind: "utility", status: "active", path: ["utility", "mirror"], ingress: { slash: true, text: true }, problemContractId: "field-mirror.v1", inputSchemaId: "field-document.v1", modalSchemaId: "utility-field.v1", resultContractId: "field-document.v1", algorithmFamily: "utility", timeoutClass: "utility_bounded", effectClasses: ["representation_only", "artifact_encoding"], helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["field-document.v1"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "field-document-v1", argvPrefix: ["utility", "mirror"] } },
  { id: "diagnostic.verify", kind: "search", status: "hidden", path: ["verify"], ingress: { slash: false, text: true }, problemContractId: "verification-report", inputSchemaId: "verify-scope", modalSchemaId: null, resultContractId: "verification-report", algorithmFamily: "verification", timeoutClass: "diagnostic", effectClasses: ["diagnostic"], helpPolicy: "hidden", i18nPolicy: "hidden", resultAllowlist: ["verify", "verify-kicks"], loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY, engine: { input: "verify", argvPrefix: ["sfinder", "verify"] } },
];

const PLANNED_CAPABILITY_ROWS = [
  ["build.setup", ["build", "setup"], "build.setup", "build_inverse_lock_clear", "build_long"],
  ["build.congruent", ["build", "congruent"], "build.congruent", "build_inverse_lock_clear", "build_long"],
  ["build.congruent-cover", ["build", "congruent-cover"], "build.congruent-cover", "build_inverse_lock_clear", "build_long"],
  ["build.setup-cover", ["build", "setup-cover"], "build.setup-cover", "build_inverse_lock_clear", "build_long"],
  ["build.setup-cover-percent", ["build", "setup-cover-percent"], "build.setup-cover-percent", "build_inverse_lock_clear", "build_long"],
  ["build.setup-cover-score", ["build", "setup-cover-score"], "build.setup-cover-score", "build_inverse_lock_clear", "build_long"],
  ["build.evaluate.cover-percent", ["build", "cover-percent"], "build.evaluate.cover-percent", "build_inverse_lock_clear", "build_long"],
  ["build.evaluate.cover", ["build", "evaluate-cover"], "build.evaluate.cover", "build_inverse_lock_clear", "build_long"],
  ["build.evaluate.minimals", ["build", "evaluate-minimals"], "build.evaluate.minimals", "build_inverse_lock_clear", "build_long"],
  ["build.evaluate.score", ["build", "evaluate-score"], "build.evaluate.score", "build_inverse_lock_clear", "build_long"],
  ["build.evaluate.b2b-cover", ["build", "evaluate-b2b-cover"], "build.evaluate.b2b-cover", "build_inverse_lock_clear", "build_long"],
].map(([id, path, problemContractId, algorithmFamily, timeoutClass]) => ({
  id,
  kind: "search",
  status: "planned",
  path,
  ingress: { slash: false, text: false },
  problemContractId,
  inputSchemaId: null,
  modalSchemaId: null,
  resultContractId: null,
  algorithmFamily,
  timeoutClass,
  effectClasses: ["search_space", "result_materialization"],
  helpPolicy: "internal",
  i18nPolicy: "internal",
  resultAllowlist: [],
  loweringAuthority: "none",
  engine: null,
}));

// The complete canonical Build v2 grammar is product-active only after the
// independent native/WASM/GUI/CLI evidence has been merged into the release
// authority. Legacy flat routes retain their separate v1 authority below.
const BUILD_V2_DISCORD_SURFACE_ROWS = Object.freeze(new Map([
  ["build.cover", {
    id: "build.cover", kind: "search", status: "active", path: ["build", "cover"],
    ingress: { slash: true, text: true }, problemContractId: "build-base-target-search.v2",
    inputSchemaId: "build-base-target.v2", modalSchemaId: "build-base-target.v2",
    resultContractId: "build-coverage-portfolio.v2", algorithmFamily: "build_inverse_lock_clear",
    timeoutClass: "build_long",
    effectClasses: ["search_space", "supply_semantics", "reachability_semantics", "probability_semantics"],
    helpPolicy: "public", i18nPolicy: "en-ko", resultAllowlist: ["build-coverage-portfolio.v2"],
    loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY,
    engine: { input: "build-v2-cover", argvPrefix: ["build", "cover"] },
    discordSurfaceStatus: "ready", productActivationReady: true,
  }],
  ["build.setup", buildV2SurfaceRow(
    "build.setup", "setup", "build-colored-target.v2", "build-colored-target.v2",
    "build-target-family.v2", "build-v2-target",
    ["search_space", "supply_semantics", "reachability_semantics", "result_materialization"],
  )],
  ["build.congruent", buildV2SurfaceRow(
    "build.congruent", "congruent", "build-colored-congruence.v1", "build-colored-target.v2",
    "build-congruence-family.v1", "build-v2-target",
    ["search_space", "supply_semantics", "result_materialization"],
  )],
  ["build.congruent-cover", buildV2SurfaceRow(
    "build.congruent-cover", "congruent-cover", "build-colored-congruence-coverage.v1",
    "build-colored-target.v2", "build-congruence-coverage.v1", "build-v2-target",
    ["search_space", "supply_semantics", "probability_semantics", "result_materialization"],
  )],
  ["build.setup-cover", buildV2SurfaceRow(
    "build.setup-cover", "setup-cover", "build-setup-cover.v1", "build-colored-target.v2",
    "build-setup-cover.v1", "build-v2-target",
    ["search_space", "supply_semantics", "probability_semantics", "result_materialization"],
  )],
  ["build.setup-cover-percent", buildV2SurfaceRow(
    "build.setup-cover-percent", "setup-cover-percent", "build-setup-cover.v1",
    "build-colored-target.v2", "build-setup-cover-probability.v1", "build-v2-target",
    ["search_space", "probability_semantics"],
  )],
  ["build.setup-cover-score", buildV2SurfaceRow(
    "build.setup-cover-score", "setup-cover-score", "build-setup-cover-score.v1",
    "build-colored-target-score.v1", "build-setup-cover-score.v1", "build-v2-target",
    ["search_space", "probability_semantics", "score_semantics", "objective_selection"],
    scoreOnlyBuildV2Semantics(),
  )],
  ["build.evaluate.cover-percent", buildV2SurfaceRow(
    "build.evaluate.cover-percent", "evaluate-cover-percent", "supplied-solution-build-evaluation.v1",
    "build-solution-document.v1", "build-supplied-probability.v1", "build-v2-supplied",
    ["probability_semantics"],
  )],
  ["build.evaluate.cover", buildV2SurfaceRow(
    "build.evaluate.cover", "evaluate-cover", "supplied-solution-build-evaluation.v1",
    "build-solution-document.v1", "build-supplied-coverage.v1", "build-v2-supplied",
    ["supply_semantics", "reachability_semantics", "probability_semantics"],
  )],
  ["build.evaluate.minimals", buildV2SurfaceRow(
    "build.evaluate.minimals", "evaluate-minimals", "supplied-solution-build-evaluation.v1",
    "build-solution-document.v1", "build-supplied-minimum-cover.v1", "build-v2-supplied",
    ["probability_semantics", "objective_selection", "result_materialization"],
  )],
  ["build.evaluate.score", buildV2SurfaceRow(
    "build.evaluate.score", "evaluate-score", "supplied-solution-build-score.v1",
    "build-solution-score-document.v1", "build-supplied-score.v1", "build-v2-supplied",
    ["reachability_semantics", "score_semantics", "objective_selection"],
    scoreOnlyBuildV2Semantics(),
  )],
  ["build.evaluate.b2b-cover", buildV2SurfaceRow(
    "build.evaluate.b2b-cover", "evaluate-b2b-cover", "supplied-solution-b2b-coverage.v1",
    "build-solution-document.v1", "build-supplied-b2b-coverage.v1", "build-v2-supplied",
    ["reachability_semantics", "score_semantics", "probability_semantics", "objective_selection"],
  )],
]));

function buildV2SurfaceRow(
  id,
  subcommand,
  problemContractId,
  inputSchemaId,
  resultContractId,
  input,
  effectClasses,
  fixedSemantics = undefined,
) {
  return {
    id,
    kind: "search",
    status: "active",
    path: ["build", subcommand],
    ingress: { slash: true, text: true },
    problemContractId,
    inputSchemaId,
    modalSchemaId: inputSchemaId,
    resultContractId,
    algorithmFamily: "build_inverse_lock_clear",
    timeoutClass: "build_long",
    effectClasses,
    helpPolicy: "public",
    i18nPolicy: "en-ko",
    resultAllowlist: [resultContractId],
    loweringAuthority: CLI_COMMAND_LOWERING_AUTHORITY,
    engine: {
      input,
      argvPrefix: id.startsWith("build.evaluate.")
        ? ["build", "evaluate", subcommand.slice("evaluate-".length)]
        : ["build", subcommand],
      ...(fixedSemantics ? { fixedSemantics } : {}),
    },
    discordSurfaceStatus: "ready",
    productActivationReady: true,
  };
}

function scoreOnlyBuildV2Semantics() {
  return {
    scoreEquality: "score-only",
    attackRole: "informational-only",
    discordTieSelection: "smallest-canonical-candidate-id",
  };
}

const LEGACY_ROUTE_ROWS = [
  { id: "pc.path/slash/path", capability_id: "pc.path", surface: "discord-slash", path: ["path"], classification: "equivalence", input: "pc", input_schema_id: "pc-pattern", modal_schema_id: "pc-pattern", argv_prefix: ["sfinder", "path"], public_result_kind: "path", remove_in: "v0.10.0" },
  { id: "pc.path/text/path", capability_id: "pc.path", surface: "discord-text", name: "path", classification: "equivalence", input: "pc", input_schema_id: "pc-pattern", modal_schema_id: "pc-pattern", argv_prefix: ["sfinder", "path"], public_result_kind: "path", lifetime: "long-term" },
  { id: "pc.minimals/slash/minimals", capability_id: "pc.minimals", surface: "discord-slash", path: ["minimals"], classification: "equivalence", input: "pc-v2", input_schema_id: "pc-pattern.v2", modal_schema_id: "pc-pattern.v2", argv_prefix: ["pc", "minimals"], public_result_kind: "minimals", remove_in: "v0.10.0" },
  { id: "pc.minimals/text/minimals", capability_id: "pc.minimals", surface: "discord-text", name: "minimals", classification: "equivalence", input: "pc-v2", input_schema_id: "pc-pattern.v2", modal_schema_id: "pc-pattern.v2", argv_prefix: ["pc", "minimals"], public_result_kind: "minimals", lifetime: "long-term" },
  { id: "pc.score-minimals/slash/score-minimals", capability_id: "pc.score-minimals", surface: "discord-slash", path: ["score-minimals"], classification: "equivalence", input: "pc-score-v2", input_schema_id: "pc-score.v2", modal_schema_id: "pc-score.v2", argv_prefix: ["pc", "score-minimals"], public_result_kind: "score-minimals", remove_in: "v0.10.0" },
  { id: "pc.score-minimals/text/score-minimals", capability_id: "pc.score-minimals", surface: "discord-text", name: "score-minimals", classification: "equivalence", input: "pc-score-v2", input_schema_id: "pc-score.v2", modal_schema_id: "pc-score.v2", argv_prefix: ["pc", "score-minimals"], public_result_kind: "score-minimals", lifetime: "long-term" },
  { id: "pc.score-finder/slash/score-finder", capability_id: "pc.score-finder", surface: "discord-slash", path: ["score-finder"], classification: "equivalence", input: "score-fixed-next", input_schema_id: "pc-fixed-score", modal_schema_id: "pc-fixed-score", argv_prefix: ["sfinder", "score-finder"], public_result_kind: "score-finder", remove_in: "v0.10.0" },
  { id: "pc.score-finder/text/score-finder", capability_id: "pc.score-finder", surface: "discord-text", name: "score-finder", classification: "equivalence", input: "score-fixed-next", input_schema_id: "pc-fixed-score", modal_schema_id: "pc-fixed-score", argv_prefix: ["sfinder", "score-finder"], public_result_kind: "score-finder", lifetime: "long-term" },
  { id: "pc.allspin-sol/slash/allspin-sol-finder", capability_id: "pc.allspin-sol", surface: "discord-slash", path: ["allspin-sol-finder"], classification: "equivalence", input: "pc-allspin-exact-v1", input_schema_id: "pc-allspin-exact-queue.v1", modal_schema_id: "pc-allspin-exact-queue.v1", argv_prefix: ["pc", "allspin-sol"], public_result_kind: "allspin-sol-finder", remove_in: "v0.10.0" },
  { id: "pc.allspin-sol/text/allspin-sol-finder", capability_id: "pc.allspin-sol", surface: "discord-text", name: "allspin-sol-finder", classification: "equivalence", input: "pc-allspin-exact-v1", input_schema_id: "pc-allspin-exact-queue.v1", modal_schema_id: "pc-allspin-exact-queue.v1", argv_prefix: ["pc", "allspin-sol"], public_result_kind: "allspin-sol-finder", lifetime: "long-term" },
  { id: "pc.allspin-pres-chance/slash/allspin-pres-chance", capability_id: "pc.allspin-pres-chance", surface: "discord-slash", path: ["allspin-pres-chance"], classification: "equivalence", input: "pc-allspin-pattern-v1", input_schema_id: "pc-allspin-pattern.v1", modal_schema_id: "pc-allspin-pattern.v1", argv_prefix: ["pc", "allspin-pres-chance"], public_result_kind: "allspin-pres-chance", remove_in: "v0.10.0" },
  { id: "pc.allspin-pres-chance/text/allspin-pres-chance", capability_id: "pc.allspin-pres-chance", surface: "discord-text", name: "allspin-pres-chance", classification: "equivalence", input: "pc-allspin-pattern-v1", input_schema_id: "pc-allspin-pattern.v1", modal_schema_id: "pc-allspin-pattern.v1", argv_prefix: ["pc", "allspin-pres-chance"], public_result_kind: "allspin-pres-chance", lifetime: "long-term" },
  { id: "build.cover/slash/cover", capability_id: "build.cover", surface: "discord-slash", path: ["cover"], classification: "equivalence", input: "cover", input_schema_id: "build-base-target", modal_schema_id: "build-base-target", argv_prefix: ["build-probability"], public_result_kind: "cover", remove_in: "v0.10.0" },
  { id: "build.cover/text/cover", capability_id: "build.cover", surface: "discord-text", name: "cover", classification: "equivalence", input: "cover", input_schema_id: "build-base-target", modal_schema_id: "build-base-target", argv_prefix: ["build-probability"], public_result_kind: "cover", lifetime: "long-term" },
  { id: "build.cover/slash/finesse/search", capability_id: "build.cover", surface: "discord-slash", path: ["finesse", "search"], classification: "fixed-preset", input: "finesse-search", input_schema_id: "finesse-base-target", modal_schema_id: "finesse-base-target", argv_prefix: ["build-probability"], public_result_kind: "finesse-search", preset: { finesse: "inputs", mirror: "exclude" }, remove_in: "v0.10.0" },
  { id: "build.cover/text/finesse/search", capability_id: "build.cover", surface: "discord-text", name: "finesse search", classification: "fixed-preset", input: "finesse-search", input_schema_id: "finesse-base-target", modal_schema_id: "finesse-base-target", argv_prefix: ["build-probability"], public_result_kind: "finesse-search", preset: { finesse: "inputs", mirror: "exclude" }, lifetime: "long-term" },
  { id: "build.finesse-score/slash/finesse/score", capability_id: "build.finesse-score", surface: "discord-slash", path: ["finesse", "score"], classification: "equivalence", input: "finesse-score", input_schema_id: "finesse-score-document.v2", modal_schema_id: "finesse-score-document.v2", argv_prefix: ["finesse", "score"], public_result_kind: "finesse-score", remove_in: "v0.10.0" },
  { id: "build.finesse-score/text/finesse/score", capability_id: "build.finesse-score", surface: "discord-text", name: "finesse score", classification: "equivalence", input: "finesse-score", input_schema_id: "finesse-score-document.v2", modal_schema_id: "finesse-score-document.v2", argv_prefix: ["finesse", "score"], public_result_kind: "finesse-score", lifetime: "long-term" },
  { id: "setup.joint/slash/pc-setup", capability_id: "setup.joint", surface: "discord-slash", path: ["pc-setup"], classification: "fixed-preset", input: "remaining", input_schema_id: "setup-ranking", modal_schema_id: "setup-ranking", argv_prefix: ["setup-finder"], public_result_kind: "pc-setup", preset: { setupPriority: "all" }, remove_in: "v0.10.0" },
  { id: "setup.joint/text/pc-setup", capability_id: "setup.joint", surface: "discord-text", name: "pc-setup", classification: "fixed-preset", input: "remaining", input_schema_id: "setup-ranking", modal_schema_id: "setup-ranking", argv_prefix: ["setup-finder"], public_result_kind: "pc-setup", preset: { setupPriority: "all" }, lifetime: "long-term" },
  { id: "setup.build/slash/best-setup", capability_id: "setup.build", surface: "discord-slash", path: ["best-setup"], classification: "fixed-preset", input: "remaining", input_schema_id: "setup-ranking", modal_schema_id: "setup-ranking", argv_prefix: ["setup-finder"], public_result_kind: "best-setup", preset: { setupPriority: "build" }, remove_in: "v0.10.0" },
  { id: "setup.build/text/best-setup", capability_id: "setup.build", surface: "discord-text", name: "best-setup", classification: "fixed-preset", input: "remaining", input_schema_id: "setup-ranking", modal_schema_id: "setup-ranking", argv_prefix: ["setup-finder"], public_result_kind: "best-setup", preset: { setupPriority: "build" }, lifetime: "long-term" },
  { id: "setup.pc/slash/dpc-finder", capability_id: "setup.pc", surface: "discord-slash", path: ["dpc-finder"], classification: "fixed-preset", input: "remaining", input_schema_id: "setup-ranking", modal_schema_id: "setup-ranking", argv_prefix: ["setup-finder"], public_result_kind: "dpc-finder", preset: { setupPriority: "pc" }, remove_in: "v0.10.0" },
  { id: "setup.pc/text/dpc-finder", capability_id: "setup.pc", surface: "discord-text", name: "dpc-finder", classification: "fixed-preset", input: "remaining", input_schema_id: "setup-ranking", modal_schema_id: "setup-ranking", argv_prefix: ["setup-finder"], public_result_kind: "dpc-finder", preset: { setupPriority: "pc" }, lifetime: "long-term" },
  { id: "forward.spin/slash/spin", capability_id: "forward.spin", surface: "discord-slash", path: ["spin"], classification: "equivalence", input: "spin", input_schema_id: "forward-spin", modal_schema_id: "forward-spin", argv_prefix: ["spin-finder"], public_result_kind: "spin", remove_in: "v0.10.0" },
  { id: "forward.spin/text/spin", capability_id: "forward.spin", surface: "discord-text", name: "spin", classification: "equivalence", input: "spin", input_schema_id: "forward-spin", modal_schema_id: "forward-spin", argv_prefix: ["spin-finder"], public_result_kind: "spin", lifetime: "long-term" },
  { id: "forward.spin/slash/spin-cover", capability_id: "forward.spin", surface: "discord-slash", path: ["spin-cover"], classification: "equivalence", input: "spin", input_schema_id: "forward-spin", modal_schema_id: "forward-spin", argv_prefix: ["spin-finder"], public_result_kind: "spin-cover", remove_in: "v0.10.0" },
  { id: "forward.spin/text/spin-cover", capability_id: "forward.spin", surface: "discord-text", name: "spin-cover", classification: "equivalence", input: "spin", input_schema_id: "forward-spin", modal_schema_id: "forward-spin", argv_prefix: ["spin-finder"], public_result_kind: "spin-cover", lifetime: "long-term" },
  { id: "forward.damage/slash/damage", capability_id: "forward.damage", surface: "discord-slash", path: ["damage"], classification: "equivalence", input: "fixed-next", input_schema_id: "forward-damage-exact-queue", modal_schema_id: "forward-damage-exact-queue", argv_prefix: ["damage"], public_result_kind: "damage", remove_in: "v0.10.0" },
  { id: "forward.damage/text/damage", capability_id: "forward.damage", surface: "discord-text", name: "damage", classification: "equivalence", input: "fixed-next", input_schema_id: "forward-damage-exact-queue", modal_schema_id: "forward-damage-exact-queue", argv_prefix: ["damage"], public_result_kind: "damage", lifetime: "long-term" },
];

const LEGACY_ROUTE_AUTHORITY_OVERRIDES = Object.freeze({
  "pc.path/slash/path": legacyPcScenarioAuthority(),
  "pc.path/text/path": legacyPcScenarioAuthority(),
  "pc.score-finder/slash/score-finder": legacyPcScenarioAuthority(),
  "pc.score-finder/text/score-finder": legacyPcScenarioAuthority(),
  "setup.joint/slash/pc-setup": legacySetupFinderAuthority("setup-ranking-joint"),
  "setup.joint/text/pc-setup": legacySetupFinderAuthority("setup-ranking-joint"),
  "setup.build/slash/best-setup": legacySetupFinderAuthority("setup-ranking-build"),
  "setup.build/text/best-setup": legacySetupFinderAuthority("setup-ranking-build"),
  "setup.pc/slash/dpc-finder": legacySetupFinderAuthority("setup-ranking-pc"),
  "setup.pc/text/dpc-finder": legacySetupFinderAuthority("setup-ranking-pc"),
  "build.cover/slash/cover": legacyBuildProbabilityAuthority(),
  "build.cover/text/cover": legacyBuildProbabilityAuthority(),
  "build.cover/slash/finesse/search": legacyBuildProbabilityAuthority(),
  "build.cover/text/finesse/search": legacyBuildProbabilityAuthority(),
});

function legacyPcScenarioAuthority() {
  return {
    problemContractId: "pc-clear-to-empty",
    resultContractId: "pc-scenario",
    effectClasses: ["search_space", "supply_semantics", "result_materialization"],
    engineKinds: ["pc-scenario"],
  };
}

function legacySetupFinderAuthority(problemContractId) {
  return {
    problemContractId,
    resultContractId: "setup-ranking",
    effectClasses: ["search_space", "probability_semantics", "result_materialization"],
    engineKinds: ["setup"],
  };
}

function legacyBuildProbabilityAuthority() {
  return {
    problemContractId: "build-base-target",
    resultContractId: "build-probability",
    effectClasses: ["search_space", "reachability_semantics", "result_materialization"],
    engineKinds: ["build-probability"],
  };
}

const GENERIC_COMPATIBILITY_ROWS = [
  { id: "discord.compat.chance", root: "chance", argvPrefix: ["sfinder", "chance"], publicResultKind: "chance", resultAuthorityId: "chance" },
  { id: "discord.compat.percent", root: "percent", argvPrefix: ["sfinder", "percent"], publicResultKind: "percent", resultAuthorityId: "percent" },
  { id: "discord.compat.score", root: "score", argvPrefix: ["sfinder", "score"], publicResultKind: "score", resultAuthorityId: "score" },
];

const CAPABILITY_PRESENTATION = Object.freeze({
  "pc.path": ["Find every represented perfect-clear path", "Discord publishes only the first result in deterministic order and exposes no tie or paging surface.", "path", "pc-path-v2"],
  "pc.chance": ["Calculate exact perfect-clear success probability", null, "chance", "pc-chance"],
  "pc.minimals": ["Find a minimum-cover perfect-clear solution set", null, "minimals"],
  "pc.score": ["Score perfect-clear solutions (basic-approximation; profile_specific_exact=false)", null, "score", "pc-score"],
  "pc.saves": ["Calculate exact probabilities for terminal-hold and active-bag-remainder save groups", "Whole-universe unconditional probability remains distinct from conditional probability given PC.", "saves"],
  "pc.best-save": ["Select clearra-save-v1 best-save witnesses", "Discord displays the first result in deterministic order; typed results retain a normal winner list.", "best-save"],
  "pc.score-minimals": ["Score minimum-cover perfect-clear solution sets", null, "score-minimals"],
  "pc.tiling": ["Enumerate geometric perfect-clear tilings without reachability", "Results may contain tilings that cannot be built.", "tiling"],
  "pc.failed-queue": ["Find queues that cannot complete the requested perfect clear", null, "failed-queue"],
  "pc.score-finder": ["Find the highest Jstris-score perfect clear for one exact queue", "Score equality and ordering are score-only. Discord selects the first result in deterministic order and never uses informational attack as a selector.", "score-finder", "pc-score-finder-v2"],
  "pc.allspin-sol": ["Find a B2B-preserving perfect-clear witness for one exact queue", "Command-intent compatibility only; aliases do not claim identical upstream judgment. Slash aliases are scheduled for removal in v0.10.", "allspin-sol"],
  "pc.allspin-pres-chance": ["Calculate B2B-preserving perfect-clear probability", "Command-intent compatibility only; aliases do not claim identical upstream judgment. Slash aliases are scheduled for removal in v0.10.", "allspin-pres-chance"],
  "build.cover": [
    "Calculate build coverage from a base field to target cells",
    null,
    "cover",
    "build-cover-v2",
  ],
  "build.probability": [
    "Search Build probability and exact result aggregations",
    "The result-mode compatibility matrix is enforced by the CLI-owned Build probability contract.",
    "probability",
    "build-probability-v1",
  ],
  "build.finesse-score": ["Calculate minimum inputs for a fixed placement document", null, "finesse-score"],
  "setup.joint": ["Rank setup candidates by joint build and PC coverage", null, "pc-setup", "setup-joint-v2"],
  "setup.build": ["Rank setup candidates by build coverage", null, "best-setup", "setup-build-v2"],
  "setup.pc": ["Rank setup candidates by perfect-clear coverage", null, "dpc-finder", "setup-pc-v2"],
  "setup.score": ["Score canonical setup-document candidates against an explicit continuation supply", "Discord preserves the ordinary ranked family within its bounded response limit.", "setup-score", "setup-score-v1"],
  "forward.spin": ["Find ordered forward selected-profile spin completions", "Legacy spin aliases are distinct from unordered structural spin search.", "spin"],
  "forward.damage": ["Find damage outcomes for one exact ordered queue", null, "damage"],
  "forward.ren": ["Find maximum exact REN witnesses for one ordered queue", null, "ren"],
  "spin-structure.search": ["Find subset-minimal spin structures from unordered pieces", "Discord preserves the ordinary family within its bounded response limit.", "spin-structure", "spin-structure-search-v2"],
  "spin-structure.cover": ["Find a minimum portfolio that covers the requested unordered spin patterns", "Discord fixes the exact portfolio tie to the first canonical portfolio and exposes no alternative or paging metadata.", "spin-structure-cover", "spin-structure-cover-v1"],
  "spin-structure.guaranteed": ["Find spin structures guaranteed to end with the requested piece", "Discord preserves the ordinary family within its bounded response limit and does not expose dependency reports by default.", "spin-structure-guaranteed", "spin-structure-guaranteed-v1"],
  "utility.sequence": ["Normalize and replay-validate one concrete operation trace", "The document order and coordinates are authoritative; queue and hold inference are forbidden.", "sequence"],
  "utility.sequence-dependencies": ["Analyze exact operation-order dependencies from one concrete placement document", "The document alone owns the initial board and operation multiset; queue and hold inference are forbidden.", "sequence-dependencies"],
  "utility.parity": ["Report bounded coordinate parity observations for every document page", "Parity is observation only: feasibility is always false, pruning authority is none, and pending garbage is reported separately.", "parity"],
  "utility.fumen": ["Apply one closed lossless v115 Fumen transform", "Split documents are a normal ordered document set, never a portfolio or tie family.", "fumen"],
  "utility.render": ["Render an exact bounded PNG page or GIF document timeline", "Discord returns the exact Rust-rendered artifact as one canonical attachment.", "render"],
  "utility.to-gray": ["Normalize occupied field colors to gray", "Only occupancy colors change; page, operation, comment, pending-garbage, and dimension identity remain authoritative.", "to-gray"],
  "utility.mirror": ["Mirror one typed field document", "Field, page, operation piece/rotation, and pending garbage are mirrored together; no portfolio or tie family is exposed.", "mirror"],
  "diagnostic.verify": ["Run hidden installation diagnostics", null, "verify"],
});

// The product runtime projection records the compatibility engine that owns
// each family. Canonical grouped commands may nevertheless use a newer typed
// input/lowering grammar; aliases retain the compatibility engine verbatim.
const CANONICAL_ROUTE_OVERRIDES = Object.freeze({
  "pc.path": { input: "pc-path-v2", argvPrefix: ["pc", "path"] },
  "pc.score-finder": { input: "pc-score-finder-v2", argvPrefix: ["pc", "score-finder"] },
  "build.finesse-score": { input: "finesse-score-v2", argvPrefix: ["finesse", "score"] },
  "setup.joint": { input: "setup-v2", argvPrefix: ["setup", "joint"] },
  "setup.build": { input: "setup-v2", argvPrefix: ["setup", "build"] },
  "setup.pc": { input: "setup-v2", argvPrefix: ["setup", "pc"] },
  "forward.spin": { input: "forward-spin-v2", argvPrefix: ["spin-finder"] },
  "forward.damage": { input: "forward-damage-v2", argvPrefix: ["damage"] },
  "spin-structure.search": { input: "spin-structure-v2", argvPrefix: ["spin-structure", "search"] },
  "spin-structure.cover": { input: "spin-structure-cover-v1", argvPrefix: ["spin-structure", "cover"] },
  "spin-structure.guaranteed": { input: "spin-structure-guaranteed-v1", argvPrefix: ["spin-structure", "guaranteed"] },
});

const LEGACY_ROUTES_BY_CAPABILITY = new Map();
for (const row of LEGACY_ROUTE_ROWS) {
  const routes = LEGACY_ROUTES_BY_CAPABILITY.get(row.capability_id) ?? [];
  routes.push(routeFromLegacyRow(row));
  LEGACY_ROUTES_BY_CAPABILITY.set(row.capability_id, routes);
}

export const productCapabilityRegistry = deepFreeze([
  ...CURRENT_CAPABILITY_ROWS,
  ...PLANNED_CAPABILITY_ROWS,
].map((row) => capabilityFromRow(BUILD_V2_DISCORD_SURFACE_ROWS.get(row.id) ?? row)));

export const discordGenericCompatibilityRoutes = deepFreeze(
  GENERIC_COMPATIBILITY_ROWS.map(genericCompatibilityRoute),
);

const PRODUCT_CAPABILITIES_BY_ID = new Map(
  productCapabilityRegistry.map((entry) => [entry.id, entry]),
);
const GENERIC_COMPATIBILITY_ROUTES_BY_ID = new Map(
  discordGenericCompatibilityRoutes.map((entry) => [entry.id, entry]),
);

export function findProductCapability(id) {
  return typeof id === "string" ? PRODUCT_CAPABILITIES_BY_ID.get(id) ?? null : null;
}

export function findDiscordGenericCompatibilityRoute(id) {
  return typeof id === "string"
    ? GENERIC_COMPATIBILITY_ROUTES_BY_ID.get(id) ?? null
    : null;
}

export function activeDiscordSearchCapabilities() {
  return productCapabilityRegistry.filter(
    ({ kind, status, discordSurfaceStatus }) =>
      (kind === "search" || kind === "utility") &&
      (status === "active" || discordSurfaceStatus === "ready"),
  );
}

export function hiddenTextSearchCapabilities() {
  return productCapabilityRegistry.filter(
    ({ kind, status, canonical }) =>
      kind === "search" && status === "hidden" && canonical.text,
  );
}

export function activeDiscordGenericCompatibilityRoutes() {
  return discordGenericCompatibilityRoutes.filter(
    ({ kind, status }) => kind === "search" && status === "compatibility",
  );
}

export function discordRuntimeProjection() {
  return productCapabilityRegistry;
}

export function discordLegacyRouteProjection() {
  return deepFreeze(LEGACY_ROUTE_ROWS.map((row) => clonePlain(row)));
}

export function discordGenericCompatibilityRouteProjection() {
  return deepFreeze(discordGenericCompatibilityRoutes.map((route) => ({
    id: route.id,
    kind: route.kind,
    status: route.status,
    helpPolicy: route.helpPolicy,
    i18nPolicy: route.i18nPolicy,
    root: route.root,
    slash: route.slash,
    text: route.text,
    classification: route.classification,
    input: route.input,
    inputSchemaId: route.inputSchemaId,
    modalSchemaId: route.modalSchemaId,
    argvPrefix: [...route.argvPrefix],
    problemFamily: route.problemFamily,
    problemContractId: route.problemContractId,
    algorithmFamily: route.algorithmFamily,
    timeoutClass: route.timeoutClass,
    executorTimeoutClass: route.executorTimeoutClass,
    effectClasses: [...route.effectClasses],
    resultContractId: route.resultContractId,
    publicResultKind: route.publicResultKind,
    resultAuthorityId: route.resultAuthorityId,
    resultAllowlist: [...route.engineKinds],
    telemetryIdentity: route.telemetryIdentity,
    loweringAuthority: route.loweringAuthority,
    removeIn: route.removeIn,
    lifetime: route.lifetime,
  })));
}

export function lowerCapabilityRouteRequest(capability, route, parameters = {}) {
  if (!capability || !route || typeof parameters !== "object" || parameters === null) {
    throw new Error("Capability route lowering requires one governed route and parameters object.");
  }
  const fixed = route.preset ?? {};
  for (const [name, value] of Object.entries(fixed)) {
    if (Object.hasOwn(parameters, name) && parameters[name] !== value) {
      throw new Error(
        `Compatibility command '${[route.root, route.subcommand].filter(Boolean).join(" ")}' fixes '${name}' to '${value}'.`,
      );
    }
  }
  return deepFreeze({
    capabilityId: capability.id,
    route: {
      root: route.root,
      subcommand: route.subcommand,
      input: route.input ?? capability.engine?.input ?? null,
      inputSchemaId: route.inputSchemaId ?? capability.inputSchemaId,
      modalSchemaId: route.modalSchemaId ?? capability.modalSchemaId,
      argvPrefix: [...(route.argvPrefix ?? capability.engine?.argvPrefix ?? [])],
      publicResultKind: route.publicResultKind ?? capability.publicResultKind,
      resultAuthorityId: route.resultAuthorityId ?? capability.resultAuthorityId,
    },
    parameters: { ...parameters, ...fixed },
  });
}

export function assertProductCapabilityRegistry() {
  const ids = new Set();
  for (const capability of productCapabilityRegistry) {
    if (!capability?.id || ids.has(capability.id)) {
      throw new Error("Discord capability IDs must be present and unique.");
    }
    ids.add(capability.id);
    if (capability.telemetryIdentity !== capability.id) {
      throw new Error(`Capability '${capability.id}' has a drifting telemetry identity.`);
    }
    if (
      capability.algorithmFamily === capability.timeoutClass ||
      !Array.isArray(capability.effectClasses) ||
      capability.effectClasses.length === 0 ||
      !Array.isArray(capability.engineKinds) ||
      !capability.canonical ||
      !Array.isArray(capability.aliases)
    ) {
      throw new Error(`Capability '${capability.id}' has an incomplete authority record.`);
    }
    const executableCliCapability =
      ["search", "utility"].includes(capability.kind) &&
      (capability.status !== "planned" || capability.discordSurfaceStatus === "ready");
    if (
      executableCliCapability &&
      capability.loweringAuthority !== CLI_COMMAND_LOWERING_AUTHORITY
    ) {
      throw new Error(
        `Executable capability '${capability.id}' escaped CLI command authority.`,
      );
    }
    if (
      !["search", "utility"].includes(capability.kind) &&
      capability.loweringAuthority === CLI_COMMAND_LOWERING_AUTHORITY
    ) {
      throw new Error(
        `Discord-owned capability '${capability.id}' cannot claim CLI command authority.`,
      );
    }
    for (const route of capability.aliases) {
      if (route.slash === route.text) {
        throw new Error(`Capability '${capability.id}' has an ambiguous compatibility route.`);
      }
      if (route.slash && route.deprecateAfter !== "v0.10.0") {
        throw new Error(`Capability '${capability.id}' has an unbounded slash alias.`);
      }
      if (route.text && route.lifetime !== "long-term") {
        throw new Error(`Capability '${capability.id}' has an ungoverned text alias.`);
      }
    }
  }
  const tiling = PRODUCT_CAPABILITIES_BY_ID.get("pc.tiling");
  if (
    tiling?.status !== "active" ||
    tiling.problemContractId !== "pc-clear-to-empty.v2" ||
    tiling.inputSchemaId !== "pc-pattern.v2" ||
    tiling.modalSchemaId !== "pc-pattern.v2" ||
    tiling.resultContractId !== "pc-tiling-family.v1" ||
    !sameStrings(tiling.engineKinds, ["pc-tiling-family.v1"]) ||
    tiling.engine?.input !== "pc-tiling-v2" ||
    !sameStrings(tiling.engine?.argvPrefix, ["pc", "tiling"]) ||
    tiling.aliases.length !== 0 ||
    !tiling.canonical.slash ||
    !tiling.canonical.text
  ) {
    throw new Error("pc.tiling must retain its closed typed family-result authority.");
  }
  const scoreMinimals = PRODUCT_CAPABILITIES_BY_ID.get("pc.score-minimals");
  if (
    scoreMinimals?.status !== "active" ||
    scoreMinimals.problemContractId !== "pc-clear-to-empty.v2" ||
    scoreMinimals.inputSchemaId !== "pc-score.v2" ||
    scoreMinimals.modalSchemaId !== "pc-score.v2" ||
    scoreMinimals.resultContractId !== "pc-score-portfolio.v2" ||
    !sameStrings(scoreMinimals.engineKinds, ["pc-score-portfolio.v2"]) ||
    scoreMinimals.engine?.input !== "pc-score-v2" ||
    !sameStrings(scoreMinimals.engine?.argvPrefix, ["pc", "score-minimals"]) ||
    scoreMinimals.engine?.fixedSemantics?.scoreEquality !== "score-only" ||
    scoreMinimals.engine?.fixedSemantics?.attackRole !== "informational-only" ||
    scoreMinimals.engine?.fixedSemantics?.discordTieSelection !==
      "smallest-canonical-candidate-id" ||
    !scoreMinimals.canonical.slash ||
    !scoreMinimals.canonical.text
  ) {
    throw new Error("pc.score-minimals must retain its score-only closed portfolio authority.");
  }
  return true;
}

export function assertDiscordGenericCompatibilityRoutes() {
  const ids = new Set();
  for (const route of discordGenericCompatibilityRoutes) {
    if (!route.id || ids.has(route.id)) {
      throw new Error("Discord generic compatibility route IDs must be unique.");
    }
    ids.add(route.id);
    if (
      route.classification !== "generic-compatibility" ||
      route.problemContractId !== "pc-clear-to-empty" ||
      route.resultContractId !== "pc-scenario" ||
      !sameStrings(route.engineKinds, ["pc-scenario"]) ||
      route.loweringAuthority !== CLI_COMPATIBILITY_LOWERING_AUTHORITY
    ) {
      throw new Error(`Generic route '${route.id}' escaped compatibility authority.`);
    }
  }
  return true;
}

function capabilityFromRow(row) {
  const [description, note, publicResultKind, resultAuthorityId] =
    CAPABILITY_PRESENTATION[row.id] ?? [
      `Planned ${row.path.join(" ")} capability`,
      null,
      row.path.at(-1),
      undefined,
    ];
  const canonicalOverride = CANONICAL_ROUTE_OVERRIDES[row.id] ?? {};
  const canonical = {
    root: row.path[0],
    subcommand: row.path[1] ?? null,
    slash: row.ingress.slash,
    text: row.ingress.text,
    classification: "canonical",
    input: canonicalOverride.input ?? row.engine?.input ?? null,
    inputSchemaId: row.inputSchemaId,
    modalSchemaId: row.modalSchemaId,
    argvPrefix: [
      ...(canonicalOverride.argvPrefix ?? row.engine?.argvPrefix ?? []),
    ],
    problemContractId: row.problemContractId,
    effectClasses: [...row.effectClasses],
    resultContractId: row.resultContractId,
    engineKinds: [...row.resultAllowlist],
    publicResultKind,
    resultAuthorityId: resultAuthorityId ?? publicResultKind,
    preset: null,
    deprecateAfter: null,
    lifetime: null,
  };
  return {
    id: row.id,
    kind: row.kind,
    status: row.status,
    problemFamily: row.path[0],
    problemContractId: row.problemContractId,
    inputSchemaId: row.inputSchemaId,
    modalSchemaId: row.modalSchemaId,
    resultContractId: row.resultContractId,
    algorithmFamily: row.algorithmFamily,
    timeoutClass: row.timeoutClass,
    executorTimeoutClass: row.timeoutClass,
    effectClasses: [...row.effectClasses],
    helpPolicy: row.helpPolicy,
    i18nPolicy: row.i18nPolicy,
    engineKinds: [...row.resultAllowlist],
    resultAllowlist: [...row.resultAllowlist],
    telemetryIdentity: row.id,
    loweringAuthority: row.loweringAuthority,
    discordSurfaceStatus: row.discordSurfaceStatus,
    productActivationReady: row.productActivationReady,
    description,
    note,
    publicResultKind,
    resultAuthorityId: resultAuthorityId ?? publicResultKind,
    engine: row.engine === null ? null : clonePlain(row.engine),
    canonical,
    aliases: [...(LEGACY_ROUTES_BY_CAPABILITY.get(row.id) ?? [])],
  };
}

function routeFromLegacyRow(row) {
  const path = row.path ?? String(row.name).split(" ");
  const authority = LEGACY_ROUTE_AUTHORITY_OVERRIDES[row.id] ?? {};
  return {
    root: path[0],
    subcommand: path[1] ?? null,
    slash: row.surface === "discord-slash",
    text: row.surface === "discord-text",
    classification: row.classification,
    input: row.input,
    inputSchemaId: row.input_schema_id,
    modalSchemaId: row.modal_schema_id,
    argvPrefix: [...row.argv_prefix],
    ...authority,
    publicResultKind: row.public_result_kind,
    resultAuthorityId: row.public_result_kind,
    preset: row.preset ? clonePlain(row.preset) : null,
    deprecateAfter: row.remove_in ?? null,
    lifetime: row.lifetime ?? null,
  };
}

function genericCompatibilityRoute(row) {
  return {
    id: row.id,
    kind: "search",
    status: "compatibility",
    helpPolicy: "public",
    i18nPolicy: "en-ko",
    root: row.root,
    slash: true,
    text: true,
    classification: "generic-compatibility",
    input: "pc",
    inputSchemaId: "pc-pattern",
    modalSchemaId: "pc-pattern",
    argvPrefix: [...row.argvPrefix],
    problemFamily: "pc",
    problemContractId: "pc-clear-to-empty",
    algorithmFamily: "pc_inverse_lock_clear",
    timeoutClass: "pc_reverse",
    executorTimeoutClass: "pc_reverse",
    effectClasses: ["search_space", "supply_semantics", "result_materialization"],
    resultContractId: "pc-scenario",
    publicResultKind: row.publicResultKind,
    resultAuthorityId: row.resultAuthorityId,
    engineKinds: ["pc-scenario"],
    resultAllowlist: ["pc-scenario"],
    telemetryIdentity: row.id,
    loweringAuthority: CLI_COMPATIBILITY_LOWERING_AUTHORITY,
    removeIn: "v0.10.0",
    lifetime: "long-term",
    description: row.root === "score"
      ? "Score perfect-clear paths through the generic compatibility engine"
      : `Calculate generic perfect-clear ${row.root}`,
  };
}

function sameStrings(left, right) {
  return Array.isArray(left) &&
    left.length === right.length &&
    left.every((value, index) => value === right[index]);
}

function clonePlain(value) {
  if (Array.isArray(value)) return value.map(clonePlain);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([key, entry]) => [key, clonePlain(entry)]),
    );
  }
  return value;
}

function deepFreeze(value) {
  if (!value || typeof value !== "object" || Object.isFrozen(value)) return value;
  for (const entry of Object.values(value)) deepFreeze(entry);
  return Object.freeze(value);
}

assertProductCapabilityRegistry();
assertDiscordGenericCompatibilityRoutes();
