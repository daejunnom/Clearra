export type ClearraProductBuildIdentity = Readonly<{
  source_commit: string;
  engine_build_id: string;
  contract_schema_version: string;
  supply_semantics_id: string;
  artifact_schema_version: string;
}>;

export type ClearraWasmBuildContract = Readonly<{
  contract_version: number;
  source_sha256: string;
  source_file_count: number;
  capabilities_sha256: string;
  runtime_identity: ClearraProductBuildIdentity;
}>;

export const CLEARRA_WASM_BUILD_CONTRACT_VERSION: number;
export const CLEARRA_WASM_REQUIRED_CAPABILITIES: readonly string[];
export function createClearraWasmBuildContract(
  repositoryRoot: string,
  environment?: Readonly<{
    CLEARRA_SOURCE_COMMIT?: string;
    CLEARRA_ENGINE_BUILD_ID?: string;
  }>
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
