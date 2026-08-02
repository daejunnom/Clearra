# Runtime Budgets

Runtime budgets keep local analysis predictable. MVP1 defaults should prioritize performance, deterministic ordering, bounded candidate growth, and clear partial-result diagnostics when limits are exceeded.

## Browser Runtime Lifecycle

- Automatic execution reserves exactly one logical processor: a host reporting `L` logical processors receives `max(1, L-1)` total workers. Selecting **Use every logical processor** permits exactly `L`; browser, desktop, CLI, and Discord execution boundaries reject or clamp any larger allocation. Windows desktop resolves `L` from native `available_parallelism()` rather than trusting the WebView report.
- Native `wasm-cpu-runtime` releases (including Linux CLI/Discord jobs and the Windows Tauri host) enable the core parallel worker pool. The selected budget therefore reaches real native worker threads instead of failing at the serial-runtime feature boundary.
- Browser startup eagerly prewarms at most 9 total WASM workers to bound idle memory and startup work. This is only a speculative prewarm ceiling: foreground execution expands the pool to the selected `L-1` or `L` budget. Setup path-detail expansion remains limited to one worker because it reuses a single selected result rather than starting another broad search.
- A cooperative browser cancellation gets 100 ms to acknowledge before the owned worker tree is force-terminated. The forced boundary exists to release descendants and memory; it does not return partial results as complete.
- Manifest and verified artifact network reads have a 30 s deadline. Bindings import, WASM compile, and instantiation each have a 60 s deadline. A deadline failure is terminal for that artifact generation and is not retried as if it were a fresh artifact.
- Distributed verifier initialization has a 90 s deadline, leaving headroom for the normal 60 s module-load budget and structured-clone fallback.
- Active verifier consumption has a 120 s stall window. Cooperative verifier slices emit a heartbeat every second, so this is an inactivity detector rather than a whole-search duration limit.
- Synchronous verifier finalization has a separate 10 minute stall window. It cannot heartbeat while inside the current ABI call, so the larger window avoids treating valid large post-processing as a hung search.

Timeouts release the affected worker tree and use only the existing transport-safe recovery policy. They do not add candidate pruning, change exact merge order, or convert an incomplete result into success.
