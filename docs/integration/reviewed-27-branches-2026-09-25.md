# Reviewed 27-branch integration — 2026-09-25

This is a semantic reconciliation on candidate `77880df9c2b0e919cacf5b10145cfab9e5f2b1e8`,
not blanket selection of old branch files. The exact branch heads and decisions
are in `approved-branches-2026-09-25.json`. All reviewed histories are retained as
parents; patch-equivalent or subsequently corrected behavior keeps the newer
candidate implementation. A history receipt is not a test pass.

PC root scheduling, adaptive Forward batches, control-only minimum managers,
complete result draining, larger-worker replacement and canonical PieceDecision
replay semantics are preserved. In particular, verifier replacement is forbidden
until geometry completion and output-lease draining; completed work is not lost
when a warm minimum worker becomes a geometry verifier.

M4/checker pruning is retained. Its theoretical basis is LLY's *Four-colour
Parity Theory: Parities and 4-remainders*, supplied for review (2024-10-20 update).
The implementation uses actual catalog rows and normalized target-frame cells;
its additive domain is necessary, not sufficient. Out-of-domain states fail open.
No current solver algorithm is replaced with a rejected A/B experiment.

The final Rust harness scheduler compiles a complete inventory once and isolates
process-global resource owners while allowing bounded independent processes.
No tests are removed. The declared ignored build root is checked by the existing
root authority rather than a contradictory blanket blacklist. Boundary Recovery
tests are separated from the engine without changing the implementation.

Windows compiler paths preserve native filesystem spelling; case-folded strings
remain ownership identities only. A pinned Windows SvelteKit/Vite fixture
reproduced the manifest failure for lower-case aliases and passed with physical
spelling. Path/link/manifest validation is not bypassed.

Current Fast Fix/component-ledger finalization is the single selective release
authority. The old Fast Correction dual-source route would conflict with current
Discord recovery/checkpoint ownership and is not reintroduced. Its source history
and historical design are retained. Rejected cache A/B implementations likewise
do not enter default execution or CI triggers.

Main may only fast-forward to the exact candidate after the nonpublishing gate
passes. Canonical Product Release and Pages publication are separate operations;
the integration workflow does not invoke them, change main or create tags.
