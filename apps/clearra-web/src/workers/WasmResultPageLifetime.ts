import type { ClearraWasmHostCapabilities, ClearraWasmModule } from './clearraWasmRuntime';

type ResultPageRuntime = Pick<
  ClearraWasmModule,
  | 'tiling_solution_available'
  | 'tiling_solution_release'
  | 'product_page_available'
  | 'product_page_release'
>;

export function retainedWasmResultPages(wasm: ResultPageRuntime) {
  return {
    tiling: wasm.tiling_solution_available(),
    product: wasm.product_page_available()
  };
}

/** Only an explicit new search owns the replacement of both page stores.
 * Reconfigure after releasing them: the ABI rejects host mutation while a
 * governed page store is live, even though its search has completed. */
export function prepareWasmForNewResult(
  wasm: ResultPageRuntime & Pick<ClearraWasmModule, 'configure_host'>,
  host: ClearraWasmHostCapabilities
) {
  const pages = retainedWasmResultPages(wasm);
  if (pages.product) wasm.product_page_release();
  if (pages.tiling) wasm.tiling_solution_release();
  wasm.configure_host(host);
}
