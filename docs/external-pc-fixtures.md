# External PC Fixtures

External PC fixtures are human-verified product E2E fixtures.

They are not source image mirrors. Clearra does not store source images as
golden evidence for these fixtures, and image pixel equality is not a worker
correctness signal.

They must use typed board masks or normalized fumen.

`input.initial_fumen` is the source of truth for external PC scenario
materialization. `input.materialized_scenario` is optional cache material; when
present, it must match the Fumen-derived scenario or Clearra reports
`E_EXTERNAL_PC_MATERIALIZED_SCENARIO_MISMATCH`.

Policy markers:

- `initial_fumen_is_source_of_truth`
- `materialized_scenario_is_cache_only`
- `worker_e2e_rejects_trivial_stub_materialization`

Raw fumen string equality is not a correctness contract; fumen input is decoded
and compared through normalized solution keys.

Minimal solve set is metadata unless explicitly used as a learning-cover test.
Worker correctness uses unique normalized solution set.

For Tsar Cannon, Clearra's worker correctness fixture uses the full unique solve set, not the minimal solve set.

The published `98.69%` value is the 90-degree SRS result (`4974 / 5040`).
Clearra's product rule is SRS+, whose 180-degree transitions additionally make
`SIZLTOJ` and `ISZLTOJ` buildable, so the corresponding complete product result
is `4976 / 5040 = 98.73015873015873%`. These values are separate rule-profile
results, not a floating-point discrepancy or a probability renormalization.

For PCO, Clearra uses the I-hold 6p PCO setup only. 7p PCO, no-hold PCO, and I-placed PCO are out of fixture scope.

## Fixture Scope

The PCO fixture uses I-hold 6p PCO only. It preserves PC Info Korea labels as
metadata and decodes the mirrored 63-page fumen as the exact normalized colored
tiling-set oracle. The fumen has no operation objects, so it proves the 63
colored tilings but does not prove a particular BuildUp order or replay trace.
Reachability includes empty spawn space above the four-row target; clipping the
movement graph to the target height creates both false accepts and false rejects.

The Tsar Cannon fixture uses hse30 full 42 solve fumen only. The worker
correctness basis is the full unique normalized solution set from the hse30
full 42 fumen link.

The user-confirmed Tsar Cannon v115 source fumen decodes to 42 pages. Clearra
stores those labels separately from the normalized solution-key artifact, and
the raw fumen string is not compared for equality.

Hard Drop and John Beak Tsar data may be reference metadata but not primary correctness source.
