# TETR.IO Score Reference

This document records the public TETR.IO score-attack rules used as an
external rules reference by Clearra. It is a source target, not evidence that
Clearra's canonical `tetrio` evaluator is profile-exact.

The reference was checked on 2026-07-21 against the public
[TETR.IO rules summary](https://tetris.wiki/Tetr.io), and the official
[TETR.IO patch notes](https://tetr.io/about/patchnotes/).

## Mode Boundary

The referenced mode is a two-minute solo score attack. It uses level-multiplied score values,
not the multiplayer garbage-attack formula. Tetra League combo attack,
Back-to-Back Charging, Surge, garbage cancellation, and margin time must not be
used when evaluating this score profile.

The profile does not award All-Spins. Spin score is restricted to T-piece spins
recognized by the T-spin corner rules. A non-T immobile spin must therefore not
be promoted to a score-bearing spin.

## Base Score Table

The selected action contributes the following base value before the current
level multiplier is applied.

| Action | Base points |
| --- | ---: |
| Single | 100 |
| Double | 300 |
| Triple | 500 |
| Quad | 800 |
| Spin Zero | 400 |
| Spin Single | 800 |
| Spin Double | 1,200 |
| Spin Triple | 1,600 |
| Spin Quad | 2,600 |
| Mini Spin Zero | 100 |
| Mini Spin Single | 200 |
| Mini Spin Double | 400 |
| Mini Spin Triple | 800 |
| Mini Spin Quad | 1,600 |
| All Clear | 3,500 |

The public table exposes All Clear as its own score action. An implementation
must classify the replay event before selecting a row; it must not invent an
additional ordinary line-clear award merely because the same lock also cleared
lines.

Clearra's PC projection deliberately fixes the level multiplier to one and
disables both soft- and hard-drop score at the user's requested boundary. It is therefore
an action-table projection rather than a complete timed score-attack model.
The initial B2B chain is configurable but defaults to zero.

## Back-to-Back, Combo, And Drop Score

A difficult clear performed with an active Back-to-Back chain multiplies the
action value by `1.5`. The multiplier applies to the action value, not to drop
points.

This score profile does not use the Season 2 multiplayer rule that advances B2B by two for
every All Clear. An All Clear adds no separate B2B increment; the underlying
line-clear or T-spin action alone determines the ordinary B2B transition.

The combo bonus is `50 * combo_index`, where the first line clear in a streak
has `combo_index = 0`, the next consecutive line clear has `combo_index = 1`,
and so on. The combo bonus is multiplied by the current level.

Drop score is independent of level:

| Drop action | Points |
| --- | ---: |
| Soft drop | 1 per descended cell |
| Hard drop | 2 per descended cell |

For one lock event, the public table can be normalized as:

```text
adjusted_action = base_action * (1.5 if active_b2b_difficult_clear else 1.0)
combo_bonus = 50 * combo_index if lines_cleared > 0 else 0
clear_score = level * (adjusted_action + combo_bonus)
drop_score = soft_drop_cells + 2 * hard_drop_cells
event_score = clear_score + drop_score
```

All public base values produce integral results under the `1.5` multiplier.
Score arithmetic should nevertheless use integer or exact rational arithmetic,
not binary floating-point accumulation.

## Level Progression

The referenced score-attack mode uses a leveling speed of `0.42`. The public level goal is equivalent to:

```text
lines_required_for_level(level) = ceil(level * 0.42 * 5)
                                = ceil(2.1 * level)
```

The first levels therefore require `3, 5, 7, 9, 11, 13, 15, 17, 19, 21, 24,
26, 28, 30, 32` lines respectively. Level is part of event state and must not
be reconstructed from final total lines alone when a replay crosses a level
boundary.

## Worked Examples

A level 5 Spin Double with no active B2B, no combo bonus, and a 10-cell hard
drop scores:

```text
1,200 * 5 + 10 * 2 = 6,020
```

A level 5 Back-to-Back Quad at combo index 3 with no drop points scores:

```text
(800 * 1.5 + 50 * 3) * 5 = 6,750
```

A level 15 All Clear with no B2B, combo, or drop contribution scores:

```text
3,500 * 15 = 52,500
```

## Exact Evaluation Requirements

A profile-exact evaluator needs, for every lock event:

- the level at the event;
- the recognized clear or All Clear action;
- exact T-spin and Mini classification;
- whether a qualifying B2B chain was active;
- the combo index before score calculation;
- soft-drop and hard-drop distances;
- a complete replay trace preserving event order.

Missing kick, corner, drop, combo, or level-transition evidence must downgrade
the result or reject an exact claim. Scoring remains post-processing and
must not alter perfect-clear coverage probability.

## Source Notes

Community-maintained rule transcriptions describe the live mode. The official
patch notes are authoritative for changes but do not publish one complete
formula. Clearra must pin a profile revision and confirm it with
known replays before setting `profile_specific_exact=true`.
