# Local kick-bound legal-board evaluation

This branch is isolated from the product and CI. Its independent executable
prototype and tests live only in ignored `_local/legal-board/`. No external
source, generated legal-board list or GPL component was copied or translated.

Command: `node --test _local/legal-board/kick-index.test.mjs`.

Five Node tests passed on 2026-09-07. All 512 three-state directed graphs agreed
with independent forward reachability (1,536 queries). Tests also cover ordered
SRS-X kick-fixture changes, regeneration after changed movement, arbitrary initial
fields outside the domain, closure, cancellation, exceptions and memory bounds.

The generation key must bind the whole ordered kick profile, not its display
name alone, together with movement/spawn/lock/clear semantics, dimensions, engine,
piece domain and initial-state domain. Missing or incomplete authority returns
unknown and uses normal Clearra search. Positive membership is never a solution
or queue coverage proof. Complete negative membership applies only inside the
explicit closed domain.

This test uses synthetic transitions plus a real kick-fixture identity test. It
does not generate or certify all 4L Tetris states for any kick profile. Production
ILC/BuildUp integration remains off. The minimum fixture spends tens of ms in
source generation and seconds in set-cover proofs, so this is not a substitute
for the minimum-algorithm work or evidence for a three-second result.
