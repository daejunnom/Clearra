export type ClearraWasmBuildContract = Readonly<{
  contract_version: number;
  source_sha256: string;
  source_file_count: number;
  capabilities_sha256: string;
}>;

export const CLEARRA_WASM_BUILD_CONTRACT_VERSION: number;
export const CLEARRA_WASM_REQUIRED_CAPABILITIES: readonly string[];
export function createClearraWasmBuildContract(
  repositoryRoot: string
): Promise<ClearraWasmBuildContract>;
export function clearraWasmCapabilitiesSha256(): string;
export function isClearraWasmBuildContract(
  value: unknown
): value is ClearraWasmBuildContract;
export function clearraWasmBuildContractsEqual(
  left: unknown,
  right: unknown
): boolean;
export function collectClearraWasmBuildSourceFiles(repositoryRoot: string): Promise<string[]>;
