# v0.8.1 surface verification on the integrated checkout

Source baseline: `098673d` on the v0.9.0-stacked-on-v0.8.1 branch. These checks
exercise the existing v0.8.1 products; they do not activate online PC4 or close
the exact-SHA release gate, browser runtime audit, or complete surface parity.

## Workspace preparation

The initial missing `ctk3`/`esbuild` errors were module-load failures, not
executed test assertions. Prepared this checkout from its existing lock using:

```powershell
npm ci --ignore-scripts --no-audit --no-fund --offline --workspace ctk3 --workspace @clearra/ui --workspace @clearra/discord-bot --workspace @clearra/web --include-workspace-root
& ./scripts/tools/invoke-clearra-build.ps1 -Purpose experiment -Command node -ArgumentsJson '["packages/ctk3/scripts/build.mjs"]'
```

The final dependency selection installed 58 packages from the local npm cache.
No dependency manifests or lock file were changed. Lifecycle scripts were not
run implicitly. CTK JS/types compilation ran explicitly in the managed single
build root and exported this checkout's package; main's workspace code and
generated CTK output were not borrowed. The web workspace supplies the Svelte
Vite preprocessor needed by the actual pager compilation test.

## Confirmed surface checks

| Explicit selection | Result |
| --- | --- |
| Discord typed-product-result | 20 passed in the preceding source-identical run |
| Discord capability-registry + GUI coveragePortfolioExportSource | 33 passed |
| Build aggregation + PC replay render parity + runtime-shell I18N | 20 passed |
| PC replay pager Svelte compilation/lifecycle boundary | 1 passed after preparing the web dependencies |
| buildV2Model, pcPathReplayGif, pcReplayPager TypeScript contracts | all 3 passed |

The seven-file focused invocation initially failed its Node group because the
pager's Svelte Vite plugin was missing. It still executed and passed the three
TypeScript contracts, preserving independent error collection. Only the missing
pager test was rerun after dependency preparation; already successful tests were
not repeated. Across the recorded selections: 74 JS tests and 3 TS contracts.

Coverage includes complete selected-set copy beyond the visible 100 entries,
copy resumption after page-cache churn, canonical Discord projection, Build
aggregation argv/result binding, replay GIF frames at 500ms, and EN/KO/JA shell
catalogs. Source compilation and contract tests are not a browser end-to-end
search or a real Discord interaction.

## CLI continuation

The managed command below completed with **6 passed**, zero failures, 301
filtered out, and 0.11s test runtime (1m48s compilation):

```powershell
$env:CARGO_PROFILE_TEST_DEBUG = '0'
& ./scripts/tools/invoke-clearra-build.ps1 -Purpose experiment -Command cargo -ArgumentsJson '["test","--locked","--offline","-p","clearra-cli","--features","wasm-cpu-runtime","--lib","cli_routing::tests","-j","2","--","--test-threads=1"]'
```

The explicit feature is required to include these tests. They exercise actual
CLI parsing/routing and include first-canonical minimum with lazy tie continuation.
The other five cover PC save probability/winner-list separation, typed scoring
versus the generic score command, opt-in result surfaces, requested-TB install
failure, and no artifact access when TB was not requested. This is the local
wasm-cpu-runtime route, not a native-C process E2E or deployed Discord execution.
No build/test job remains running at this checkpoint.
