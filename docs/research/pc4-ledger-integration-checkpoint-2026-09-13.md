# PC4 ledger integration checkpoint — 2026-09-13

This is implementation evidence, not profile qualification or release acceptance.
The v0.8.1/v0.9.0 scope and release order remain in the active release plan.

## Integrated changes

- `8030715` integrates the graph-derived observation outcome ledger. Every bag
  reveal remains in the exact probability universe, including zero-solution
  outcomes. Hold siblings do not duplicate random mass. Graph traversal,
  concrete materialization and the reveal ledger must all exhaust before the
  canonical candidate union can acquire reducer authority.
- `5d4bfba` updates the architecture mutation test to require the actual sealed
  evidence constructor, not an obsolete raw evidence struct literal.
- `6d55e0e` replaces the Windows managed compiler's cmd transport with a native
  argv launcher. The shared Node guard still owns output and transaction checks.
  The launcher is built inside the same generation and has no external cache.
  Bootstrap failure follows normal owner cleanup, including failed product
  deletion and lease release.

## Confirmed verification

- Managed `cargo test --locked --offline -p clearra-pc4-tablebase --lib`:
  **147 passed**, zero failures; test runtime 0.05 seconds. These are local
  algorithm/contract fixtures, not upstream graph completeness evidence.
- Node build policy suite: **11 passed**, including a Windows argument-vector
  test exceeding cmd's command-line limit with quotes, Unicode and backslashes.
- PowerShell artifact path/lifecycle suite: passed, including injected launcher
  failure cleanup, mixed Windows/WSL metadata, product history and stale leases.
  Its deliberate owner-mismatch fixture emits a lease-preservation warning;
  that warning is expected negative-path evidence, not a leaked real build.
- PC4 full-solution authority validator mutation suite: passed.
- Native launcher rustfmt check and Git whitespace check: passed.

## App validation and discovered constraints

The original Windows cmd wrapper failed while compiling `windows-sys` with
`The command line is too long`. The native launcher passed that same dependency
compilation boundary without removing the guard.

The default-feature App unit-test binary with `online-pc4-tablebase` subsequently
failed in LLVM with `out of memory`. No App test success is claimed for that run.
An attempted no-default-features test exposed a separate feature boundary:
`commands/render_app_command.rs` imports bitmap-render-only output types even
when `bitmap-render` is disabled. That configuration cannot currently compile;
it is not a PC4 algorithm test failure and must not be used as a passing shortcut.

A lower-symbol retry uses the real default product features, sets only
`CARGO_PROFILE_TEST_DEBUG=0`, bounds Cargo jobs to two, and selects App tests
matching `pc4_`. This preserves test assertions and the product feature set.
This retry completed compilation and ran **60 tests: 59 passed, one failed**.
The failure was the synthetic Setup helper `admitted_for`, which attached a 4L
candidate target to requests for every target from 1L through 4L. Production
correctly refused that mismatch. The helper now passes the request's exact
target to `reducer_for_target`; this fixture correction still needs a compiled
rerun and is not reported as a passing 60-test suite.

Before changing that fixture, the same completed test executable was reused
without rebuilding to run `pc4_observation_candidate_adapter`: **19 passed**,
zero failures, 0.01 seconds. It covers zero-solution mass, hold deduplication,
scope/profile/source binding, cancellation/rollback, budgets, and delayed
finalization. Its SHA-256 was
`DC8777D40CB25952ACD9BAF69C009DB50E1C9350946ACC28658AF092C975D900`.
The compiled tracked source was `6d55e0e`; the subsequent fixture edit is not
covered by this binary. There are no running build/test jobs at this checkpoint.

## Remaining integration boundaries

### Follow-up verification at df2919b

The managed `check-pc4-app-contracts.mjs` batch completed successfully after the
fixture and feature-boundary fixes: default-feature PC4 **60 passed** (0.02s),
default-feature render **1 passed**, and no-bitmap render **1 passed**. All three
commands ran nonzero test counts and the batch exited zero. This closes the
compiled-rerun requirement below; the earlier failures above remain historical
evidence, not current failures. These are local contract checks, not upstream
qualification or release acceptance. No build/test job remains running.

- The unpublished aliases `successful_reveals`, `observed_successful_reveals`,
  and `observed_successful_reveal_count` were removed in the follow-up working
  changes; callers now use explicitly named all-outcome accessors.
- The App bitmap-render imports, render implementation and helper functions are
  feature-gated in the follow-up. Feature-off requests return typed Unsupported
  without an artifact, instead of enabling bitmap support implicitly. Both PNG
  and GIF rejection and feature-on PNG output have test coverage in source.
- Compile and execute these follow-ups and the corrected Setup fixture using
  `scripts/tools/check-pc4-app-contracts.mjs` under the managed build owner.
  It shares one generation across default-feature PC4, default-feature render,
  and no-bitmap render tests, uses no debug symbols to avoid the observed local
  LLVM memory exhaustion, and stops on the first failed compilation/test.
- Complete surface/reducer and fallback integration with actual runtime tests.
- Qualify each independently activated HF profile and target using upstream
  bytes/KAT/completeness and signed generation evidence. Synthetic fixtures do
  not authorize a production profile. V*/policy/Krylov remain out of this runtime.
- Final exact-SHA v0.8.1 acceptance/deployment must precede v0.9.0 activation.
  Main has not been merged, pushed, or deployed by this checkpoint.
