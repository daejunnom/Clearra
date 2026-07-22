# Rules And Kicks

MVP2 keeps built-in rule presets, capability metadata, verified kick-table import/export, and a guarded search path for imported kick profiles.

MVP2 kick semantics are still intentionally explicit:

- `srs` uses Clearra's built-in numeric 90-degree SRS kick tables.
- `no-kick` uses only `(0, 0)` rotation offsets and harddrop/no-kick candidates.
- `srs-plus` uses Clearra's built-in exact SRS+ table: regular SRS JLSTZ 90-degree kicks, y-axis-symmetric I-piece 90-degree kicks, and transition-specific 180-degree kicks. `rules list` and `rules inspect` disclose `source_kind=built-in-exact` and `supports_exact_180=true`.
- `srs-x` uses SRS 90-degree kicks with the built-in Nullpomino/Heboris-style 180 table. The WASM product backend consumes this table directly and exactly. The C compact lowering remains a separate compatibility boundary and accepts SRS-X only through a verified imported profile; it must never fall back to SRS+, SRS, or NoKick.
- `asc` and `ars` remain selectable registry descriptors whose spawn-aware reachability is unsupported. CLI inspection must report `search_backend_supported=false`, `c_compact_descriptor_ready=false`, and an `unsupported_backend_reason`, and validation must reject them before search.

Clearra does not yet import solution-finder-style property kick files. Those files preserve details such as direction-specific transition keys, aliases, 180-degree variants, and annotated T kick entries. MVP2 import/export currently uses strict JSON to produce `KickTableProfile`, and search can consume only `VerifiedKickTableProfile` overrides. Verified imported 180 profiles are the exact-180 path (`supports_exact_180=true`) and are reported as `c_compact_descriptor_ready=true` only when their source rule can be lowered to the current C compact descriptor. Raw kick JSON or property text must not enter search directly.

MVP2 owns the long-lived kick contract types:

- `KickTableProfile` identifies a kick table preset or future imported custom profile.
- `KickTransition` identifies a piece and rotation transition.
- `KickOffsetSequence` preserves ordered first-success offsets.
- `KickVerificationCase` checks a profile transition against an expected sequence.
- `KickProfileDescriptor` exposes `transition_count`, `supports_180`,
  piece-specific transitions through profile entries, first-success order
  preservation, source/provenance, and `verified=true|false`.

`verify kicks` uses these contracts for built-in SRS 90, no-kick, and SRS+ 180 profiles. `rules verify --input` reports `missing_transition_count`, `duplicate_transition_count`, `unsupported_annotation_count`, `verified_profile`, `c_compact_descriptor_ready`, and `unsupported_backend_reason` without pretending a profile was imported. `rules import --input` succeeds only for `VerifiedKickTableProfile` values; unverified profiles are rejected, while verified-but-backend-unsupported profiles must still disclose the unsupported backend reason. Imported kick JSON is strict: unknown fields are rejected, duplicate transitions keep the profile unverified, missing transitions are reported, invalid rotation transitions are rejected, unsupported piece ids are rejected, and first-success offset order is preserved. Future property import/export should produce and consume verified `KickTableProfile` values instead of letting search or CLI parse raw property text directly.

X1 Rule / Kick Expansion keeps `SRS-X`, `ASC`, and `ARS` visible as named
profiles while preventing silent runtime fallback. WASM uses the built-in
verified SRS-X table; SRS-X can reach the C compact descriptor only through an
imported verified exact 180 profile. `ASC` and `ARS` remain guarded until
spawn-aware reachability is connected.
Unsupported profiles must not fall back to NoKick. 180 candidates must not be
generated unless the compact profile has `supports_180=true`.

Canonical PC inputs carry imported profile overrides as verified values. Opening and scenario presets may receive a `VerifiedKickTableProfile` from CLI assembly, but `clearra-problem` lowers them into `PcQuery -> OpeningPreset -> SearchProblem` or a scenario preset before execution. Runtime code consumes verified kick profiles only after validation; raw kick JSON and unverified profile drafts must not enter search or the core executor. Cache identity includes a kick-profile fingerprint so two different imported tables cannot reuse each other's result.

## Full Custom Rule Editor

MVP3 full custom rule editing extends the MVP2 verified kick-profile path with spawn, rotation system, lock/reachability, and line-clear policy sections. The editor contract must keep this pipeline:

`raw editor schema -> validation -> VerifiedCustomRuleProfile -> CustomRuleSearchCapabilityReport -> search execution`

`CustomRuleEditorDraft` is the raw typed editor draft. It is not a search input. Validation must produce `VerifiedCustomRuleProfile`, which embeds a `VerifiedKickTableProfile`, `SpawnProfile`, `RotationSystem`, `LockReachabilityPolicy`, and `LineClearPolicy`. Search capability is reported separately through `CustomRuleSearchCapabilityReport`; until generalized custom-rule search is connected, verified custom rules must report `search_backend_supported=false` with `custom_rule_search_backend_not_connected`.

Raw custom rule JSON, raw `CustomRuleEditorDraft`, and unverified `KickTableProfile` values must not enter the executor path. Core execution may consume only verified profile values after validation and capability reporting.

## KickEvidence

Kick-sensitive scoring and spin target predicates require replay-visible kick
evidence. C reachability may compute first-success kicks internally, but exact
spin classification cannot depend on hidden C state. BuildVariant replay must be
able to expose:

```rust
pub struct KickEvidence {
    pub from_rotation: RotationState,
    pub to_rotation: RotationState,
    pub rotation_request: RotationRequest,
    pub kick_index: u8,
    pub kick_dx: i16,
    pub kick_dy: i16,
    pub kick_table_id: KickTableProfileId,
    pub kick_profile_id: Option<VerifiedKickTableProfileId>,
    pub first_success_confirmed: bool,
    pub predecessor_anchor: BoardAnchor,
    pub result_anchor: BoardAnchor,
}
```

If a profile needs kick evidence and the replay path cannot provide it, output
must disclose `spin_accuracy=kick_sensitive_unavailable` or fail an exact
SpinTarget request.

Kick evidence is first-success evidence. It is not enough to say a rotation
ended at a target placement; the evidence must identify the forward transition
that won first-success order. Reverse reachability may find predecessor states,
but exact spin classification consumes the confirmed forward kick index and
offset.

## Kick-Sensitive Spin Cases

Fin / ISO / NEO are not modeled as independent base kick tables. They are
modeled as profile-specific special spin classification cases that require kick
evidence and board signature evidence.

Fin / ISO / NEO는 독립적인 기본 킥테이블로 모델링하지 않는다. 기본 SRS 계열
kick evidence와 board signature를 해석하는 profile-specific special spin
classification case로 모델링한다.

Verified classification preserves the named-case semantics: Fin is a regular
T-spin and NEO is a mini T-spin. A normal-placement interpretation may coexist
only when an independent non-rotation lock path reaches the same placement;
rotation-only reachability must not manufacture normal-placement evidence.

Forbidden:

- `KickTableProfileId::FinSpecial`
- `KickTableProfileId::IsoSpecial`
- `KickTableProfileId::NeoSpecial`

Allowed:

- `SpecialSpinCaseId::Fin`
- `SpecialSpinCaseId::Iso`
- `SpecialSpinCaseId::Neo`
- `VerifiedSpecialSpinProfile`

## SpecialSpinCaseRegistry

`KickTableProfileId` describes how a rotation result is produced:

```rust
pub enum KickTableProfileId {
    Srs90,
    SrsPlus,
    NoKick,
    SrsX,
    Asc,
    Ars,
    Imported(ImportedKickProfileId),
}
```

Special spin ids describe how a rotation result and board signature are
classified:

```rust
pub enum SpecialSpinCaseId {
    Fin,
    Iso,
    Neo,
    ImportedSpecialSpin(ImportedSpecialSpinId),
    CustomSpecialSpin(CustomSpecialSpinId),
}

pub struct SpecialSpinCase {
    pub id: SpecialSpinCaseId,
    pub display_name: String,
    pub piece: PieceKind,
    pub required_kick_signature: Option<KickSignature>,
    pub board_signature_predicate: BoardSignaturePredicate,
    pub corner_rule_override: CornerRuleOverride,
    pub mini_override: Option<bool>,
    pub regular_override: Option<bool>,
    pub allowed_profiles: Vec<ScoreProfileId>,
    pub verification_state: SpecialSpinVerificationState,
}

pub enum SpecialSpinVerificationState {
    SourcePinnedFixture,
    VerifiedImport,
    DescriptorOnly,
    Disabled,
}
```

## VerifiedSpecialSpinProfile

```rust
pub struct VerifiedSpecialSpinProfile {
    pub id: VerifiedSpecialSpinProfileId,
    pub base_kick_profile: KickTableProfileId,
    pub special_cases: Vec<SpecialSpinCaseId>,
    pub fixture_set_id: FixtureSetId,
    pub spin_classifier_capability: SpinClassifierCapability,
}
```

`SourcePinnedFixture` permits exact classification only for covered fixture
cases. `VerifiedImport` may enable exact classification only when replay carries
kick evidence. `DescriptorOnly` is not a search backend capability.

## Unsupported Special Spin Profile Behavior

Descriptor-only cases report `search_backend_supported=false` and either
`E_SPIN_PROFILE_UNVERIFIED` or `W_SPIN_CLASSIFICATION_ESTIMATED`, depending on
whether the user requested exact classification. Missing kick evidence reports
`E_SPIN_KICK_EVIDENCE_MISSING` for exact SpinTarget queries and a warning for
optional approximate score output.

`VerifiedImport` and `SourcePinnedFixture` are the only states that may enable
exact special-spin classification. Even then, exact classification is available
only when the replay contains the required `KickEvidence` and board signature
evidence. A descriptor may be shown in UI as disabled, but it must not advertise
`search_backend_supported=true`.

## Rule/Kick Ownership Boundary

`clearra-rules` owns profile source, import/export, verification, and capability
metadata. The C core owns only compact runtime descriptors used by candidate
generation, reachability, and BuildUp. CLI and GUI may select a rule or import
a verified profile, but they must not reinterpret raw kick property text or
special spin descriptors into search behavior.

## Required Tests

- `special_spin_case_is_not_kick_table_profile`
- `special_spin_case_requires_kick_evidence`
- `unverified_fin_iso_neo_profile_is_disabled`
- `verified_special_spin_profile_enables_kick_sensitive_classifier`
- `kick_sensitive_rule_uses_first_success_kick_index`
- `kick_sensitive_spin_requires_kick_evidence`
- `missing_kick_evidence_downgrades_spin_accuracy`
