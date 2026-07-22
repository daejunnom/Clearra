# TETR.IO Tetra League Season 2 Attack Reference

This document records the public Season 2 Tetra League garbage-attack rules as
an external profile reference for Clearra. It describes attack and cancellation
state, not score-attack points.

The reference was checked on 2026-07-21. Season 2 began on 2024-08-16. The
primary change record is the official
[BETA 1.2.0 patch note](https://tetr.io/about/patchnotes/#chlog_BETA_1_2_0),
with the later
[BETA 1.3.0 garbage-special change](https://tetr.io/about/patchnotes/#chlog_BETA_1_3_0)
and
[BETA 1.5.0 spin-rule change](https://tetr.io/about/patchnotes/#chlog_BETA_1_5_0).
The public [Mechanics](https://tetrio.wiki.gg/wiki/Mechanics) and
[Tetra League](https://tetrio.wiki.gg/wiki/TETRA_LEAGUE) pages provide the
combined formula and mode context.

## Season 2 Boundary

Tetra League Season 2 uses Back-to-Back Charging. The legacy Season 1
Back-to-Back Chaining table must not be substituted for it. Quick Play also
uses Charging, but has different defaults such as RNG rounding and a smaller
Surge start, so Quick Play values must not leak into a Tetra League profile.

Season 2 launched with All-Mini. Since BETA 1.5.0, the default multiplayer spin
rule is All-Mini+, which also permits an immobile T placement that fails the
normal corner rule to be classified as a Mini. A current Season 2 profile must
pin this later rule revision instead of treating the launch-day rules as
timeless.

## Base Attack

The ordinary default attack weights are:

| Clear | Base attack |
| --- | ---: |
| Single | 0 |
| Double | 1 |
| Triple | 2 |
| Quad | 4 |
| T-Spin Zero | 0 |
| T-Spin Single | 2 |
| T-Spin Double | 4 |
| T-Spin Triple | 6 |
| T-Spin Mini Zero | 0 |
| T-Spin Mini Single | 0 |
| T-Spin Mini Double | 1 |

For the damage state machine, every zero-line spin is normalized to the same
`NoClear` action as an ordinary placement: it sends no attack, resets combo,
and does not advance or break B2B. Spin evidence remains available to replay,
spin search, and score profiles; only the damage transition is normalized.

Non-T All-Mini+ spins have no direct base attack under the Season 2 launch
contract, but they are difficult clears and can establish or preserve B2B.
Consequently, any final attack they produce must be attributed to an active B2B
bonus, combo behavior, garbage-special bonus, or another separately identified
rule, not to an invented full-spin base value.

An All Clear sends a flat 5 attack, replacing the underlying clear, combo, and
active-B2B attack calculation for that lock, and advances B2B by 2. This is the
Tetra League value; the Quick Play All Clear value is different.

## Combo Multiplier And Rounding

Let `combo_index` be zero for the first line clear in a consecutive clear
streak. For a positive attack weight, TETR.IO's multiplier combo formula is:

```text
multiplied = attack_weight * (1 + 0.25 * combo_index)
```

When the attack weight is zero, combo index 2 and above uses:

```text
multiplied = ln(1 + 1.25 * combo_index)
```

Tetra League uses `DOWN` rounding, so the multiplied result is rounded toward
negative infinity to an integer. All values are nonnegative, making this an
integer floor. Quick Play's probabilistic `RNG` rounding must not be used.

## Back-to-Back Charging

A difficult line clear includes a Quad, a qualifying Spin Mini clear, or a
qualifying full Spin clear. The first difficult clear establishes B2B but is
not displayed as `B2B x1`; the displayed count starts with the next difficult
clear.

While B2B is active, a qualifying attack receives a flat `+1` attack weight.
The public mechanics formula places this B2B weight before the combo
multiplier:

```text
attack_weight = base_attack + active_b2b_bonus
combo_attack = floor(combo_formula(attack_weight, combo_index))
```

At displayed `B2B x4`, Tetra League starts a Surge with power 4. Further B2B
increments raise the stored power one for one, so `B2B x8` stores 8. A
non-difficult line clear breaks the chain and releases the stored Surge as a
separate attack.

Surge power `n` is split into three packets. The first packet receives the
first remainder and the second packet receives the second remainder:

```text
q = floor(n / 3)
r = n mod 3
packets = [q + (r >= 1), q + (r >= 2), q]
```

Examples are `4 -> [2, 1, 1]`, `5 -> [2, 2, 1]`, and `8 -> [3, 3, 2]`.
Ordinary clear attack and Surge packets are distinct outputs and must not be
collapsed into one packet before cancellation and garbage processing.

## Garbage Special Bonus

Since BETA 1.3.0, a Quad or Spin, including an All-Spin, that clears at least
one garbage cell sends a flat `+1` attack. This bonus is explicitly not affected
by multipliers, so it is added after combo rounding.

A normalized clear-event calculation is therefore:

```text
weight = base_attack + active_b2b_bonus
combo_adjusted = floor(combo_formula(weight, combo_index))
clear_packet = combo_adjusted
             + (5 if all_clear else 0)
             + (1 if qualifying_clear_removed_garbage else 0)
```

This normalization follows the public rule descriptions. A
`profile_specific_exact` implementation still requires replay fixtures that
pin event ordering, B2B transition timing, and the active server preset.

## Opening Defense And Garbage Cancellation

For the first 14 placed pieces, when pending incoming garbage is greater than
the amount already sent, outgoing attack can cancel twice as many pending
lines. This is cancellation strength, not doubled damage. The extra defensive
capacity must not be converted into attack sent to the opponent.

Ordinary outgoing attack first cancels pending incoming garbage; only the
uncancelled remainder is sent. Passthrough is disabled for Tetra League by
default, although network delay can still produce small apparent passthrough
in a live match.

Margin-time multipliers, garbage caps, and room/server preset values affect the
final packet flow. Public prose does not provide a durable, complete current
Tetra League preset snapshot for every such field. Clearra must therefore
materialize these values from a pinned profile or report the attack evaluation
as incomplete rather than silently assuming a custom-room default.

## Worked Examples

A zero-combo Double without active B2B sends:

```text
floor(1 * 1.0) = 1
```

A combo-index-2 T-Spin Double with active B2B sends:

```text
floor((4 + 1) * (1 + 0.25 * 2)) = floor(7.5) = 7
```

If that same clear removes garbage, the flat special bonus is added after
rounding:

```text
7 + 1 = 8
```

## Exact Evaluation Requirements

A profile-exact evaluator needs the following ordered state:

- clear and spin classification under the pinned All-Mini+ revision;
- combo index;
- B2B active state, displayed count, and stored Surge power;
- All Clear status;
- whether the clear removed garbage cells;
- piece count, pending garbage, and sent garbage for opening defense;
- margin-time and garbage-processing preset identity;
- separate ordinary and Surge packets;
- complete replay event ordering.

Attack is post-processing evidence. It must never prune perfect-clear search or
change PatternBitSet coverage probability.

## Source-Certainty Rule

Official patch notes are authoritative for dated changes. The combined formula
and explanatory tables are community-maintained. Clearra must retain source
revision metadata and replay conformance evidence before claiming exact Tetra
League Season 2 attack support.
