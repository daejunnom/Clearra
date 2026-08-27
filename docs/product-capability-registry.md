# Product Capability Registry

`tests/fixtures/contracts/product_capability_registry.v1.json` is the
machine-readable product contract and requirement-status ledger for the Clearra
v0.8.0 command reorganization. It records accepted intent; it does not make an
implementation complete by describing it. An `implemented` status needs
current-source evidence whose scope proves the complete requirement.

The registry pins three independent upstream references:

- sfinder-man `438187b6a0ce4bf543ffc9faae507fdc11970e13`;
- sfinderbot `0a539c7aa5835b210f8e7aa9248525ba8f3d95ef`;
- solution-finder `0e7c935a5399159a3d9c42fb8721e3c6842ae17d`.

The first source contains exactly 47 active `@bot.command` declarations. The
second contributes exactly 34 command names not present in the first. Every one
of those 81 canonical names has exactly one disposition:

- `canonical`: the upstream problem maps directly to one typed Clearra
  capability;
- `preset`: the upstream command is a fixed semantic preset of a typed
  capability;
- `absorbed`: no separate command is needed because a typed option, result
  artifact, or request-local policy owns the behavior;
- `excluded`: the behavior crosses an explicit product or authority boundary.

An upstream name is never considered covered merely because a similarly named
Clearra command exists. The validator requires a live capability or exclusion
target for every inventory row.

## Integration boundary

Two operations may share a public command path only when all of the following
are identical:

1. product problem definition;
2. required and forbidden inputs;
3. result meaning and completeness contract;
4. guided Discord Modal schema;
5. canonical typed request after fixed preset lowering.

Sharing an executor, traversal direction, or timeout is insufficient. In
particular:

- PC fixes the goal at clear-to-empty and cannot accept an arbitrary target;
- target Build accepts a base plus target delta;
- supplied-solution Build evaluates a document instead of generating a target;
- ordered forward spin retains queue and hold state;
- spin-structure consumes unordered inventory and has no hold;
- finesse search is `/build cover` with
  `finesse=inputs mirror=exclude`, while finesse score remains a distinct
  document-evaluation contract at `/build finesse-score`.

This produces the public search roots `/pc`, `/build`, `/setup`, `/forward`, and
`/spin-structure`. The registry deliberately contains no public catch-all or
standalone finesse root.

## Algorithm, timeout, and effect axes

`algorithm_family`, `timeout_class`, and `effect_classes` are separate required
fields on every capability.

- `algorithm_family` identifies semantic execution authority, such as
  `pc_inverse_lock_clear`, `forward_state_expansion`, or `structural_exact`.
- `algorithm_phases` records the relevant product stages without implying that
  every capability needs a separate physical executor.
- `timeout_class` selects resource policy. The fact that Build, Setup, Forward,
  and Structure initially use 15 minutes does not merge their algorithms or
  input contracts.
- `effect_classes` describes what can change: search space, supply,
  reachability, objective, score, probability, materialization, artifact,
  representation, configuration, or external state.

Host-performance settings such as worker count are not search-semantic effects.
They belong to host policy and must not be used to claim GUI/Discord result
option parity.

## Non-public surfaces

Advanced objective selection uses layered discoverability.

`advanced.objective` is available only to `$`/`>` text commands and the advanced
CLI syntax. It is absent from slash registration, Modal components, and
autocomplete. General help may acknowledge that advanced objectives exist;
only the layered objective help explains IDs and grammar. An objective can
select a registered semantic preset but cannot change its problem family or
make a forbidden input legal.

Excluded raw gateways, user filesystems, process controls, destructive
administration, impersonation, database views, scraping, and message/fun
commands have no surface profile at all. They cannot become public by adding a
new alias.

## Compatibility and status

Grouped commands begin in v0.8.0. A legacy slash alias may remain through two
released versions only when it lowers to a fieldwise-identical typed request;
the registry therefore permits it in v0.8 and v0.9 and schedules removal in
v0.10.0. `$`/`>` text aliases may remain long-term only while that typed
equivalence continues to hold. Slash and text are separate inventory rows; a
slash route can be removed without retiring its exact-compatible text spelling.
A same-name or same-result-looking command is not an alias
when its input or result contract differs. Examples include PC saves versus
best-save, exact All-Spin witness versus preservation probability, target Build
versus supplied-solution coverage, and forward spin versus structure search.

The `capability_implementation`, `result_affecting_option_exposure`, and
`requirements` sections form the implementation ledger. Every entry uses one
of four states: `implemented`, `partial`, `missing`, or `excluded`. `accepted`
on a requirement means the product decision is fixed; it does not mean its
implementation is complete. `partial` and `missing` always block v0.8.0. An
`implemented` claim needs current evidence, and only an intentional upstream
boundary may use `excluded`. The ledger must be refreshed from current evidence
as implementation lands and again at the exact-source release freeze.

### Upstream drift audit lifecycle

`node scripts/tools/audit-upstream-drift.mjs --phase implementation-start`
resolves every upstream repository `HEAD`, re-reads the pinned and observed
source bytes, and compares the active `sfinder-man` 47-command inventory plus
the 34-command `sfinderbot`-only difference against the registry. It also
hashes the pinned `solution-finder` documentation tree beneath the registered
path. Any moved `HEAD`, changed source snapshot, truncated tree response, or
command-set mismatch fails closed.

The implementation-start observation is retained in
`tests/fixtures/contracts/upstream_drift_implementation_start.v1.json`. The
separate exact-source release observation is retained in
`tests/fixtures/contracts/upstream_drift_release_freeze.v1.json`; it reports
`phase=release-freeze` and `status=no-drift`, so both halves of
`REQ-V080-020` are implemented. Explicit output is validated against the exact
release-freeze phase and current registry identity, written only to a new
regular path beneath non-link directories, flushed, and never overwrites an
earlier audit. The historical implementation-start registry date is retained
rather than rewritten; its pinned source and command inventory are still
checked fieldwise against the current registry.

## Current-source reconciliation (2026-08-28)

The final current-byte audits reconcile the formerly stale ledger rows and bind
the completed v0.8 product decisions to executable evidence:

- `pc.allspin-sol` and `pc.allspin-pres-chance` are implemented across the
  grouped and compatibility Discord routes, private text aliases, CLI, Web,
  App, cooperative execution, and typed result projections. The former retains
  its full normalized B2B-preserving solution family while Discord projects its
  canonical first witness; this is a normal family, not a portfolio tie. Its
  exact-queue contract remains distinct from the latter's pattern-probability
  contract.
- `pc.failed-queue` is implemented across canonical grouped Discord slash and
  text routes, CLI, Web, App, and Core. `/pc failed-queue` lowers exactly to
  `pc failed-queue`, and Discord accepts only the public CLI result kind
  `pc-failed-queue.v2`. Legacy top-level forms remain generic Percent routes
  and are not typed aliases.
- `pc.chance` is implemented across canonical grouped Discord slash and text
  routes, CLI, Web, App, and Core. `/pc chance` lowers exactly to `pc chance`,
  and Discord accepts only the public CLI result kind `pc-probability.v2`.
  Native, direct-Wasm, and cooperative execution preserve the complete typed
  probability evidence through one App validation and then remove transient
  authority. Top-level `chance` and `percent` remain independently governed
  generic compatibility routes, not aliases of the typed v2 capability.
- `pc.score` has the canonical grouped Discord slash and text route, CLI, Web,
  GUI, Desktop, App, and Core implementation. `/pc score` lowers exactly to
  `pc score` and Discord
  accepts only `pc-score-summary.v2`. The direct and cooperative paths both use
  the fixed Wasm CPU, single-session contract; each execution owns one authority
  and preserves the typed request, executed-problem, completeness, and
  score-result evidence through App validation. Native and distributed
  execution are not authorized for this product route and fail closed.
  Eligibility, ordering, family membership, and the Discord representative are
  score-only. Equal-score candidates with different attack remain in the same
  ordinary result family; attack is retained only as an informational
  canonical-trace observation. Every built-in profile currently reports
  `accuracy_level=basic-approximation` and
  `profile_specific_exact=false`; choosing a profile is not evidence of exact
  profile-specific scoring. Top-level `score` remains an independently
  governed generic Jstris Ultra compatibility route, not an alias of this typed
  v2 capability.
- `pc.tiling` is implemented across canonical grouped Discord slash and text
  routes, CLI, Web, GUI, App, and Core. `/pc tiling` lowers exactly to
  `pc tiling`, and Discord accepts only `pc-tiling-family.v1`. Its dedicated
  compiler selects `SearchOutputPolicy::TilingOnly`; Native Core and Wasm CPU
  materialize the complete sorted and deduplicated raw-geometry family without
  BuildUp, retain the full family in one pageable store, and publish the first
  100 keys together with exact count and set-hash evidence. Native execution
  admits and attests the retained family internally, while direct and
  cooperative Wasm execution validate the same family under terminal memory
  authority. CLI artifact publication consumes every page before an atomic
  commit. The generic `--tiling-only` and `--objective tiling` spellings retain
  their untyped Trace contract and fail closed as `noncanonical-tiling-objective`;
  they do not acquire the `pc.tiling` product claim.
- `advanced.objective` is implemented as `$`/`>` text option grammar plus the
  advanced CLI boundary. It is intentionally not a runtime command capability.
- `REQ-V080-002` and `REQ-V080-003` are implemented by the family-specific
  command/Modal registry and the v2 result-affecting option contract. The
  latter requires every current GUI search option to have one named Discord
  field, semantic preset, advanced-objective boundary, or explicit exclusion.
- `REQ-V080-010`, `REQ-V080-011`, and `REQ-V080-012` are implemented by the
  shared supply automaton and the typed, fail-closed solution-set audit,
  equivalent-coverage classes, and single canonical portfolio foundation.
  Complete lazy enumeration of all optimal public portfolios is the separate
  accepted `REQ-V080-021` and does not reopen `REQ-V080-012`.

`build.source-pieces` is implemented across the native Web command and desktop
lowering, GUI request/control code, Discord named-option lowering, and the
cross-surface option contract. Omission retains the native automatic supply
window, while an explicit positive value is preserved fieldwise through every
surface and changes the executed Build universe.

The v2 option contract and its production-lowering/UI projection tests close
the other 71 formerly stale option rows. `build.solution-probabilities` now
owns a typed Build-only request policy from Discord and Web lowering through
Core execution and the direct, cooperative, and worker result paths. The App
boundary reconstructs and validates canonical solution keys, coverage rows,
probabilities, and completeness before publishing the result.

`pc.minimals` owns the typed `pc-clear-to-empty.v2` request and
`pc-minimum-cover.v2` result boundary across Discord, CLI, Web, GUI, Desktop,
App, and Core.
The App boundary recompiles the exact query, validates the producer coverage
universe, runs the deterministic two-pass exact minimum-cover primitive, and
binds the selected identities, coverage rows, hashes, probability evidence,
and completeness before publication. The result-bound portfolio store retains
every equal-cardinality optimum in canonical numeric-vector order. GUI, Web,
and Desktop page the entire store; native CLI exposes it only through explicit
durable tie snapshots; Discord publishes the canonical first portfolio with no
tie metadata. Explicit memory caps and unsupported distributed authority still
fail closed rather than weakening completeness.

All twelve Build rows, the remaining PC/Setup/Spin rows, and request-local
profile selection are implemented. The formerly open Build queue-observation,
advanced objective, score-profile, and initial-B2B inputs now change the typed
compiled problem and executed result instead of being accepted as inert echo
fields.

All accepted requirements are in a terminal implemented state. For
`REQ-V080-016` and `REQ-V080-017`, that status means the exact-source evidence
machinery, fail-closed deployment authorities, and runbook are implemented in
the release source. It does not predeclare the later Pages, Oracle, Discord,
observation, tag, or immutable-release events; those are recorded only in the
external hash-chained release-attempt journal after they occur.

`REQ-V080-013` is implemented. The shared two-phase state machine is connected
to both the admitted native producer and registered native host boundaries,
uses the native JSONL, browser IndexedDB, and injected-failure test journals,
and requires durable acknowledgement before publication. Current-source tests
cover every operation, nonterminal acknowledgement failure, heartbeat/expiry,
stale fencing, terminal replay, tombstone compaction, single-writer recovery,
quarantine, and torn or corrupted journal records across the native and browser
implementations.

`REQ-V080-018` is implemented by the typed `runtime_projection`. It records
current runtime problem/input/Modal/result IDs separately from the target v2
capability contract, and validates every active, hidden, and planned Discord
runtime entry fieldwise. It also governs path and ingress, algorithm, timeout,
effects, help/i18n policy, result allowlist, telemetry identity, and exact
lowering authority. This is not permission to rename a current runtime ID to a
target v2 ID before the matching semantic validator exists.

`REQ-V080-019` is implemented. Its Rust proof parses every slash and text
equivalence or fixed-preset spelling into the same typed App request on the
current source; semantically different names remain `distinct`.
`legacy_alias_equivalence.v1.json` binds the 30 fieldwise-identical ingress
rows to 15 paired parser cases. Three independent generic-compatibility
descriptors govern six top-level slash/text ingress forms: `chance`, `percent`,
and `score`. Canonical `pc chance` and `pc score` own different typed v2 problem
and result contracts, so none of those generic forms may enter the equivalence
fixture. The focused Discord test executes the real
`buildSlashCommandArguments` and `parseClearraTextRequest` paths and compares
their exact argv with that fixture. Registry projection retains each route's
resolved input, input schema, Modal schema, argv prefix, and public result kind
instead of pretending that different current parser contracts are
raw-identical. Both JSON authorities freeze those five values independently
for every ingress row; the live slash and text commands are compared against
the frozen rows, so a forged parser/schema/argv/result identity fails rather
than being compared with another projection of itself.

Every typed-alias fixed preset is applied at the slash argument-builder boundary. A
matching explicit value is accepted, an omitted value is supplied by the
route's fixed semantics, and a conflicting slash or text value fails closed.
This rule covers score-minimals, the finesse/mirror preset, and all three setup
priorities without affecting canonical commands. Legacy `/score` instead owns
an independent generic translator contract with fixed Jstris Ultra semantics;
the upstream sfinder-man `score` command is therefore recorded as a preset of
the migration target, not as canonical typed `pc.score`, and is never compared
as raw argv with TETR.IO-default `/pc score`.

The Rust fixture test feeds each frozen canonical and alias argv for both
surfaces through the authoritative `WebCommandParser`, converts both into typed
`AppRequest`s, and compares the `AppCommand` family, query envelope, complete
normalized command, and request policy fields. The text transport's exact
terminal `--format text` option is asserted and removed before semantic parsing
because output formatting is not an `AppCommand` field. This yields 15 logical
cases times two surfaces, or 30 typed comparisons, and also fail-closes
problem/result-family drift. Separate public Web tests prove that top-level
`chance`/`percent` and `score` retain no Product claim while canonical
`pc chance` and `pc score` do.
The focused Rust proof compiles and passes on the current exact fixture bytes.
Final-source revalidation remains a separate
`REQ-V080-016` release-freeze obligation rather than weakening the current
`REQ-V080-019` implementation claim.

`REQ-V080-008` is implemented: `/finesse search` is a fieldwise Build-cover
compatibility preset (`finesse=inputs`, `mirror=exclude`), while canonical
`/build finesse-score` retains its distinct document input, Modal,
fixed-placement algorithm family, and result contract.

`REQ-V080-014` and `artifact.solution-set` are implemented by the typed
solution-set model; streaming Compact-v1, JSON, CTK3, and Fumen encoders;
bounded sinks; canonical comments; strict source-availability gates; and
capability-bound no-replace native publication on Linux and Windows. Every
solution-bearing native CLI command can publish the complete ordered family or
optimal portfolio without applying the interactive 100-member view limit, and
the native CTK3/Fumen path uses only Rust codecs with no JavaScript or network
dependency. `REQ-V080-015` is implemented by typed execution
availability and resource reports, shared compute/memory leases, and
fail-closed admission before work starts. The browser distributed Build path
accounts for the producer and every verifier under one memory authority before
allocating the producer; if the aggregate plan does not fit, it selects the
typed serial terminal path instead of starting partial work. Standalone
verifiers retain their own typed admission, and resource-limited or unavailable
work remains explicitly not-started and incomplete rather than becoming an
empty complete result. This claim does not manufacture availability for larger
6-line or GPU plans: an unsupported or unadmitted plan remains unavailable.
The machine-readable ledger is the exhaustive authority. At release freeze it
contains zero `partial` or `missing` capability, option, or accepted-requirement
rows. `release_readiness.status` is therefore `ready`; the hard gate would turn
red again if any nonterminal row were reintroduced.

## Portfolio alternative applicability

`portfolio_alternative_policy` is exhaustive for v0.8 public portfolio ties.
It applies to `pc.minimals`; `pc.score-minimals`; the minimum-cover and
maximum-probability-minimum forms of `build.cover`, `build.congruent-cover`,
`build.setup-cover`, and `build.evaluate.minimals`; the max-score-cover forms
of `build.setup-cover-score` and `build.evaluate.score`; and
`spin-structure.cover`.

The final representative metric for each form is minimum integer member
cardinality after the form's eligible set is fixed. For
`max-probability-minimum`, first maximize exact union probability and then
minimize member count. For `max-score-cover`, first retain every candidate tied
at the maximum integer score for each pattern and then minimize member count.
Attack is not an objective coordinate or tie breaker.

Candidate IDs are one-based after sorting stable normalized solution keys. A
portfolio sorts those IDs, and portfolios use numeric lexicographic vector
order. Enumeration is exact, unbounded at the semantic layer, two-pass, lazy,
and restartable. A progressive known count is not an exact total until the
traversal seals. GUI surfaces page every portfolio; explicit CLI requests use a
durable result-bound snapshot; Discord emits only the canonical first
portfolio and no tie metadata.

The registry separately enumerates normal families which must not be
reclassified as portfolio ties. This includes `pc.score`, `pc.best-save`,
`pc.score-finder`, `pc.allspin-sol`, Setup rankings, Forward outcomes,
spin-structure search/guaranteed results, and operation sequence/order lists.

## Image recognition exclusion and typed encoders

`utility.to-fumen` was an image-recognition proposal rather than typed Fumen
encoding. It is removed from required capabilities, implementation rows, and
the planned Discord projection. Upstream `tofumen` and `calibrate` now map to
`exclusion.image-to-fumen-recognition`. Palette recognition, calibration,
auto-crop, and image inference are outside v0.8.

This does not remove `utility.fumen` or native Fumen output. The v2
`artifact.solution-set` contract requires `text`, `json`, `ctk3`, and `fumen`
formats on solution-bearing native CLI paths. CTK3 must use a Rust-native codec
that shares the language-neutral KAT authority with the TypeScript package;
native output cannot invoke JavaScript or a network service.

## Exact operation sequence utility

`utility.sequence` is implemented across the native command/app, CLI, WASM,
Web, GUI, and Discord surfaces. It is a trace normalization and replay
validation utility, not an operation-order search. Its only input authority is
one `operation-document.v1` CTK3 or operation-preserving v115 Fumen document;
every page must retain one concrete locked operation. Queue and hold fields are
rejected instead of being inferred, and incomplete or non-operation-preserving
documents fail closed.

The canonical CLI form is
`clearra utility sequence --document <CTK3_OR_FUMEN>` with optional built-in
rule and kick profiles and a `1..900` second timeout. Both profile defaults are
`srs-plus`; the timeout default and maximum are 900 seconds. The deterministic
typed `operation-sequence.v1` result preserves document order, piece, rotation,
and centered coordinates, then reports per-step board-before, lock mask,
board-after, cleared-row, reachability, and kick evidence. Replay supports up
to 4096 concrete operations and publishes no partial success: invalid input,
timeout, cancellation, and incomplete analysis remain typed terminal states.
Discord reuses the bounded operation-document Modal or attachment and returns
only the canonical summary plus a bounded trace preview, without replay
evidence or portfolio/tie metadata.

## Exact operation dependency utility

`utility.sequence-dependencies` is implemented across the native command/app,
CLI, WASM, Web, GUI, and Discord surfaces. Its only input authority is one
`operation-document.v1` CTK3 or operation-preserving v115 Fumen document: the
document owns both the initial board and the concrete operation multiset, while
queue and hold fields are rejected instead of being inferred. The canonical
CLI form is `clearra utility sequence-dependencies --document <CTK3_OR_FUMEN>`
with optional built-in rule and kick profiles and a `1..900` second timeout;
both profile defaults are `srs-plus`, and the timeout default and maximum are
900 seconds.

The typed `operation-dependency-report.v1` result preserves the exact accepted
order language and arbitrary-precision decimal count, supports up to 4096
operations with dynamic bitsets, and reports universal precedence closure,
deterministic transitive reduction, independent pairs, and reachability/kick
evidence after line-clear remapping. Timeout, cancellation, and incomplete
analysis remain typed terminal states. Discord uses the shared operation
document Modal or one bounded attachment and returns only the small canonical
candidate report; it does not expose the full evidence payload.

## Product authority to Discord runtime projection

The structural validator imports the live Discord capability registry and
compares the complete current projection, not an implemented-only subset. The
comparison covers every active and hidden entry fieldwise and expands the
typed planned defaults over every planned entry before comparing it too. The
current and target contracts remain deliberately separate, so the comparison
does not mistake a desired v2 schema ID for present runtime semantics.

The same validator exhaustively compares the live alias list with the product
`legacy_routes` inventory and the cross-language alias fixture. It checks the
separate ingress, stable target, classification, preset, v0.10 slash removal,
long-term text lifetime, parser input family, exact slash/text argv, and
problem/result family. Each live route must also match the independently frozen
input schema, Modal schema, argv prefix, and public result kind. The
three independent generic compatibility descriptors are compared separately;
their six slash/text forms intentionally do not enter `legacy_routes` or the
typed-equivalence fixture. The
catalog's original-GIF slash/text/message routes are explicitly recorded as
distinct non-capability routes; they are not falsely relabeled as target
`utility.render`.
Advanced objective remains a text/CLI option which resolves to a canonical PC
capability; it is not a slash command, Modal, or autocomplete surface.

## Validation

Run the complete contract test with:

```powershell
node --test tests/contracts/product_capability_registry.test.mjs
```

It fails when:

- either pinned inventory loses or gains a command;
- an ID is duplicated;
- a command is unmapped or targets a missing exclusion/capability;
- a preset lacks explicit lowering;
- an algorithm, timeout, effect, schema, or surface reference is missing;
- two public command paths collide;
- a catch-all or standalone finesse root becomes public;
- advanced objective gains slash, Modal, autocomplete, or incorrect help
  exposure;
- known non-equivalent contracts are collapsed;
- a current runtime field or live legacy ingress escapes the product runtime
  projection;
- a fixed preset can be overridden by a caller or is compared as raw
  equivalence;
- a slash alias survives at v0.10.0 or later;
- unsafe upstream commands map to a public capability;
- a requirement claims completion without evidence.

It also contains a hard release-readiness assertion. The ordinary invocation
must pass for the frozen v0.8.0 source and fails if any required capability,
result-affecting option, or accepted requirement becomes `partial` or `missing`;
documenting a gap does not turn it green. During ledger editing, the structural
checks can be isolated with:

```powershell
node --test --test-skip-pattern="v0.8.0 readiness closes" tests/contracts/product_capability_registry.test.mjs
```

The focused runtime tests additionally prove catalog/help/Modal/result
allowlist/timeout/telemetry fields are generated from, or fail-closed against,
the live capability registry. The release workflow runs the complete product
contract, including the hard readiness assertion, before packaging. These
checks close REQ-V080-018 and REQ-V080-019. Readiness still describes source
completion, not deployment completion; the external release-attempt journal is
the authority for post-source production events.

The fieldwise slash/text compatibility proof is reproducible with:

```powershell
cargo test -p clearra-web-command legacy_alias_fixture_parses_30_surface_pairs_to_identical_public_app_requests
```
