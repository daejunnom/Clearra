import { invoke } from '@tauri-apps/api/core';

import type { RenderCapabilityReport } from '../render/renderCapabilityReport';
import type {
  ClearraProductPageWorkerPayload,
  ClearraProductResultPayload,
  ClearraProductBuildIdentity,
  ClearraWasmSearchReport
} from '../wasm/wasmCommandClient';

type ClearraDesktopRequestBase = {
  app_request_model: 'clearra-app/AppRequest';
  language: 'en' | 'ko';
  lines: number;
  queue: string;
  patterns: string;
  queue_knowledge: 'oracle' | 'visible-7';
  hold_enabled: boolean;
  hold_piece: 'empty' | 'I' | 'O' | 'T' | 'S' | 'Z' | 'J' | 'L';
  backend: 'auto' | 'cpu' | 'gpu' | 'hybrid';
  rule: 'srs-plus' | string;
  score_mode:
    | 'tiling'
    | 'path'
    | 'off'
    | 'minimum-cover'
    | 'summary'
    | 'score-finder'
    | 'score-minimals'
    | 'failed-queue'
    | 'saves'
    | 'best-save';
  score_profile: 'guideline' | 'jstris-ultra' | 'tetrio';
  spin_profile:
    | 'disabled'
    | 't-spins'
    | 't-spins-plus'
    | 'all-spin'
    | 'all-spin-plus'
    | 'all-mini'
    | 'all-mini-plus';
  preserve_b2b: boolean;
  precompute_build_dependencies: boolean;
  finesse: 'off' | 'inputs';
  pattern_knowledge: 'both' | 'oracle' | 'visible-7';
  board_mask: string;
  visible_height: number;
  piece_window: number | null;
  count_policy: 'unique' | 'all';
  solution_probabilities: boolean;
  workers: number;
  use_all_logical_processors: boolean;
  gpu_device: string;
  allow_backend_fallback: boolean;
  /** Zero leaves memory unbounded by policy; allocator/host OOM still applies. */
  memory_budget_mb: number;
  candidate_budget: number;
  pattern_budget: number;
  tablebase_requested?: boolean;
  setup_mode?: 'oracle' | 'qb';
  setup_remaining?: string;
  setup_qb?: string;
  setup_next_cycle_remaining?: string;
  setup_allow_post_cycle_borrow?: boolean;
  setup_priority?: 'all' | 'build' | 'pc';
  setup_length?: 'auto' | 'longer' | 'shorter';
  setup_max_pieces?: number;
  setup_path_setup_id?: string;
  setup_path_condition_id?: string;
  base_mask?: string;
  target_mask?: string;
  build_aggregation?: 'buildability' | 'tiling' | 'spin';
  include_horizontal_mirror?: boolean;
};

type ClearraDesktopPcRequest = ClearraDesktopRequestBase & {
  command: 'pc' | 'pc-scenario';
  initial_b2b: number;
  initial_combo?: never;
  damage_aggregation?: never;
  minimum_damage?: never;
  spin_lines?: never;
  spin_category?: never;
};

type ClearraDesktopDamageRequest = ClearraDesktopRequestBase & {
  command: 'damage';
  initial_combo: number;
  initial_b2b: number;
  damage_aggregation: 'maximum' | 'at-least';
  minimum_damage?: number;
  spin_lines?: never;
  spin_category?: never;
};

type ClearraDesktopSpinFinderRequest = ClearraDesktopRequestBase & {
  command: 'spin-finder';
  initial_combo?: never;
  initial_b2b?: never;
  damage_aggregation?: never;
  minimum_damage?: never;
  spin_lines: 'any' | '0' | '1' | '2' | '3' | '4' | '1+' | '2+' | '3+' | '4+';
  spin_category: 'any' | 't' | 'other';
};

type ClearraDesktopRenRequest = ClearraDesktopRequestBase & {
  command: 'ren';
  initial_combo?: never;
  initial_b2b?: never;
  damage_aggregation?: never;
  minimum_damage?: never;
  spin_lines?: never;
  spin_category?: never;
};

type ClearraDesktopSetupRequest = ClearraDesktopRequestBase & {
  command: 'setup';
  source_piece_count?: never;
  initial_combo?: never;
  initial_b2b?: never;
  damage_aggregation?: never;
  minimum_damage?: never;
  spin_lines?: never;
  spin_category?: never;
};

type ClearraDesktopBuildProbabilityRequest = ClearraDesktopRequestBase & {
  command: 'build-probability';
  source_piece_count?: number;
  initial_combo?: never;
  initial_b2b?: never;
  damage_aggregation?: never;
  minimum_damage?: never;
  spin_lines?: never;
  spin_category?: never;
};

export type ClearraDesktopBuildV2Request = {
  app_request_model: 'clearra-app/AppRequest';
  command: 'build-v2';
  language: 'en' | 'ko';
  capability_id:
    | 'build.cover'
    | 'build.setup'
    | 'build.congruent'
    | 'build.congruent-cover'
    | 'build.setup-cover'
    | 'build.setup-cover-percent'
    | 'build.setup-cover-score'
    | 'build.evaluate.cover'
    | 'build.evaluate.minimals'
    | 'build.evaluate.score'
    | 'build.evaluate.b2b-cover'
    | 'build.evaluate.cover-percent';
  base_mask?: string;
  target_mask?: string;
  visible_height?: number;
  source_piece_count?: number;
  target_format?: 'ctk3' | 'fumen';
  target_document?: string;
  solution_format?: 'ctk3' | 'fumen';
  solution_document?: string;
  queue: string;
  patterns: string;
  queue_knowledge: 'oracle' | 'visible-7';
  hold_enabled: boolean;
  hold_piece: 'empty' | 'I' | 'O' | 'T' | 'S' | 'Z' | 'J' | 'L';
  objective: 'all' | 'unique' | 'min-cover' | 'max-probability-minimum' | 'max-score-cover';
  score_profile?: 'guideline' | 'jstris-ultra' | 'tetrio';
  initial_b2b?: number;
  rule: string;
  workers: number;
  use_all_logical_processors: boolean;
  backend: 'cpu';
  allow_backend_fallback: false;
  max_memory_mib?: never;
  memory_budget_mb?: never;
};

export type ClearraDesktopSetupScoreRequest = {
  app_request_model: 'clearra-app/AppRequest';
  command: 'setup-score';
  language: 'en' | 'ko';
  document_format: 'ctk3' | 'fumen';
  document: string;
  setup_queue: string;
  setup_patterns: string;
  solution_queue: string;
  solution_patterns: string;
  clear_height: number;
  hold_enabled: boolean;
  score_profile: 'guideline' | 'jstris-ultra' | 'tetrio';
  initial_b2b: number;
  rule: string;
  max_patterns: number;
  workers: number;
  use_all_logical_processors: boolean;
  backend: 'cpu';
  allow_backend_fallback: false;
  hold_piece?: never;
  gpu_device?: never;
  max_memory_mib?: never;
  memory_budget_mb?: never;
};

type ClearraDesktopSpinStructureBase = {
  app_request_model: 'clearra-app/AppRequest';
  command: 'spin-structure';
  language: 'en' | 'ko';
  board_mask_v1: string;
  visible_height: number;
  inventory: string;
  spin_profile:
    | 't-spins'
    | 't-spins-plus'
    | 'all-mini'
    | 'all-mini-plus'
    | 'all-spin'
    | 'all-spin-plus';
  lines: 'any' | '0' | '1' | '2' | '3' | '4' | '1+' | '2+' | '3+' | '4+';
  fill_bottom: number;
  fill_top: number;
  rule: 'srs-plus' | 'srs' | 'srs-x' | 'jstris-180' | 'no-kick';
  max_placements: number;
  minimality: 'subset-minimal' | 'minimum-piece-count';
  workers: number;
  use_all_logical_processors: boolean;
  backend: 'cpu';
  allow_backend_fallback: false;
  queue?: never;
  patterns?: never;
  hold_enabled?: never;
  hold_piece?: never;
  gpu_device?: never;
  max_memory_mib?: never;
  memory_budget_mb?: never;
};

type ClearraDesktopSpinStructureSearchRequest = ClearraDesktopSpinStructureBase & {
  capability_id: 'spin-structure.search';
  objective?: never;
  max_patterns?: never;
  final_piece?: never;
  dependency_report?: never;
};

type ClearraDesktopSpinStructureCoverRequest = ClearraDesktopSpinStructureBase & {
  capability_id: 'spin-structure.cover';
  objective: 'min-cover';
  max_patterns: number;
  final_piece?: never;
  dependency_report?: never;
};

type ClearraDesktopSpinStructureGuaranteedRequest = ClearraDesktopSpinStructureBase & {
  capability_id: 'spin-structure.guaranteed';
  objective?: never;
  max_patterns: number;
  final_piece: 'I' | 'O' | 'T' | 'S' | 'Z' | 'J' | 'L';
  dependency_report: boolean;
};

export type ClearraDesktopSpinStructureRequest =
  | ClearraDesktopSpinStructureSearchRequest
  | ClearraDesktopSpinStructureCoverRequest
  | ClearraDesktopSpinStructureGuaranteedRequest;

export type ClearraDesktopSpinStructureRequestInput =
  | Omit<
      ClearraDesktopSpinStructureSearchRequest,
      'app_request_model' | 'command' | 'backend' | 'allow_backend_fallback'
    >
  | Omit<
      ClearraDesktopSpinStructureCoverRequest,
      'app_request_model' | 'command' | 'backend' | 'allow_backend_fallback'
    >
  | Omit<
      ClearraDesktopSpinStructureGuaranteedRequest,
      'app_request_model' | 'command' | 'backend' | 'allow_backend_fallback'
    >;

export type ClearraDesktopSequenceDependenciesRequest = {
  app_request_model: 'clearra-app/AppRequest';
  command: 'utility-sequence-dependencies';
  language: 'en' | 'ko';
  document: string;
  rule_profile: string;
  kick_profile: string;
  timeout_seconds: number;
  // These properties make forbidden search-resource fields explicit at the
  // discriminated-union boundary. The builder never serializes them.
  backend?: never;
  workers?: never;
  use_all_logical_processors?: never;
};

export type ClearraDesktopOperationSequenceRequest = {
  app_request_model: 'clearra-app/AppRequest';
  command: 'utility-sequence';
  language: 'en' | 'ko';
  document: string;
  rule_profile: string;
  kick_profile: string;
  timeout_seconds: number;
  backend?: never;
  workers?: never;
  use_all_logical_processors?: never;
};

export type ClearraFumenTransform =
  | 'roundtrip'
  | 'combine'
  | 'split'
  | 'get-page'
  | 'page-shift'
  | 'clean-comments'
  | 'preserve-comments'
  | 'to-gray'
  | 'mirror'
  | 'text-to-fumen';

export type ClearraDesktopParityRequest = {
  app_request_model: 'clearra-app/AppRequest';
  command: 'utility-parity';
  language: 'en' | 'ko';
  format: 'ctk3' | 'fumen';
  document: string;
  backend?: never;
  workers?: never;
  use_all_logical_processors?: never;
};

export type ClearraDesktopFumenRequest = {
  app_request_model: 'clearra-app/AppRequest';
  command: 'utility-fumen';
  language: 'en' | 'ko';
  format: 'fumen';
  transform: ClearraFumenTransform;
  documents: string[];
  page_number?: number;
  page_shift?: number;
  comments: string[];
  backend?: never;
  workers?: never;
  use_all_logical_processors?: never;
};

export type ClearraDesktopRenderRequest = {
  app_request_model: 'clearra-app/AppRequest';
  command: 'utility-render';
  language: 'en' | 'ko';
  format: 'ctk3' | 'fumen';
  document: string;
  artifact_format: 'png' | 'gif';
  page_number?: number;
  backend?: never;
  workers?: never;
  use_all_logical_processors?: never;
};

export type ClearraDesktopFieldDocumentTransformRequest = {
  app_request_model: 'clearra-app/AppRequest';
  command: 'utility-to-gray' | 'utility-mirror';
  language: 'en' | 'ko';
  format: 'ctk3' | 'fumen';
  document: string;
  backend?: never;
  workers?: never;
  use_all_logical_processors?: never;
};

export type ClearraDesktopRequest =
  | ClearraDesktopPcRequest
  | ClearraDesktopDamageRequest
  | ClearraDesktopSpinFinderRequest
  | ClearraDesktopRenRequest
  | ClearraDesktopSetupRequest
  | ClearraDesktopBuildProbabilityRequest
  | ClearraDesktopBuildV2Request
  | ClearraDesktopSetupScoreRequest
  | ClearraDesktopSpinStructureRequest
  | ClearraDesktopOperationSequenceRequest
  | ClearraDesktopSequenceDependenciesRequest
  | ClearraDesktopParityRequest
  | ClearraDesktopFumenRequest
  | ClearraDesktopRenderRequest
  | ClearraDesktopFieldDocumentTransformRequest;

export type ClearraDesktopRequestInput = Partial<ClearraDesktopRequestBase> & {
  command?: ClearraDesktopRequest['command'];
  initial_combo?: number;
  initial_b2b?: number;
  damage_aggregation?: ClearraDesktopDamageRequest['damage_aggregation'];
  minimum_damage?: number;
  spin_lines?: ClearraDesktopSpinFinderRequest['spin_lines'];
  spin_category?: ClearraDesktopSpinFinderRequest['spin_category'];
  source_piece_count?: number;
  document?: string;
  rule_profile?: string;
  kick_profile?: string;
  timeout_seconds?: number;
  format?: 'ctk3' | 'fumen';
  transform?: ClearraFumenTransform;
  documents?: string[];
  page_number?: number;
  page_shift?: number;
  comments?: string[];
  artifact_format?: 'png' | 'gif';
};

export type ClearraDesktopAppResponse = {
  runtime_identity: ClearraProductBuildIdentity;
  status: 'success' | 'validation-failed' | 'unsupported' | 'execution-failed';
  diagnostics: Array<{ code: string; severity: string; message: string }>;
  product_result_payload?: ClearraProductResultPayload | null;
  solution_set_artifact?: import('../wasm/wasmCommandClient').ClearraSolutionSetArtifactPayload | null;
  capability_report: {
    app_request_boundary: string;
    executor_boundary: string;
    render_capability: RenderCapabilityReport;
  };
  backend_report?: ClearraDesktopBackendStatus;
  resource_report?: ClearraDesktopResourceStatus;
  [key: string]: unknown;
};

export type ClearraDesktopBackendStatus = {
  backend_requested?: string;
  backend_selected?: string;
  fallback_used?: boolean;
  backend_fallback_reason?: string | null;
};

export type ClearraDesktopMemoryStatus = {
  state?: string;
  leak_report_clean?: boolean;
  raw_pointer_exposed?: boolean;
};

export type ClearraDesktopResourceStatus = {
  budget_status?: string;
  done?: number;
  total?: number;
  truncated?: boolean;
  truncation_reason?: string | null;
  probability_complete?: boolean;
};

export type ClearraDesktopJobEvent = {
  schema_version: 1;
  event: 'started' | 'progress' | 'diagnostic' | 'completed' | 'failed' | 'cancelled';
  job_id: number;
  done?: number;
  total?: number;
  label?: string;
  code?: string;
  severity?: string;
  response?: ClearraDesktopAppResponse;
  search_report?: ClearraWasmSearchReport | null;
  scope_released?: boolean;
  backend_status?: ClearraDesktopBackendStatus;
  memory_status?: ClearraDesktopMemoryStatus;
  resource_status?: ClearraDesktopResourceStatus;
};

export function buildDesktopAppRequest(
  input: ClearraDesktopRequestInput
): ClearraDesktopRequest {
  const command = input.command ?? 'pc';
  if (command === 'build-v2') {
    throw new TypeError('Build v2 requires the nominal buildDesktopBuildV2Request builder');
  }
  if (command === 'setup-score') {
    throw new TypeError('Setup score requires the nominal buildDesktopSetupScoreRequest builder');
  }
  if (command === 'spin-structure') {
    throw new TypeError(
      'Spin structure requires the nominal buildDesktopSpinStructureRequest builder'
    );
  }
  if (command === 'utility-sequence' || command === 'utility-sequence-dependencies') {
    return {
      app_request_model: 'clearra-app/AppRequest',
      command,
      language: input.language ?? 'en',
      document: input.document ?? '',
      rule_profile: input.rule_profile ?? 'srs-plus',
      kick_profile: input.kick_profile ?? 'srs-plus',
      timeout_seconds: input.timeout_seconds ?? 900
    };
  }
  if (command === 'utility-parity') {
    return {
      app_request_model: 'clearra-app/AppRequest',
      command,
      language: input.language ?? 'en',
      format: input.format ?? 'ctk3',
      document: input.document ?? ''
    };
  }
  if (command === 'utility-fumen') {
    return {
      app_request_model: 'clearra-app/AppRequest',
      command,
      language: input.language ?? 'en',
      format: 'fumen',
      transform: input.transform ?? 'roundtrip',
      documents: input.documents ?? [],
      ...(input.page_number === undefined ? {} : { page_number: input.page_number }),
      ...(input.page_shift === undefined ? {} : { page_shift: input.page_shift }),
      comments: input.comments ?? []
    };
  }
  if (command === 'utility-render') {
    return {
      app_request_model: 'clearra-app/AppRequest',
      command,
      language: input.language ?? 'en',
      format: input.format ?? 'ctk3',
      document: input.document ?? '',
      artifact_format: input.artifact_format ?? 'png',
      ...(input.page_number === undefined ? {} : { page_number: input.page_number })
    };
  }
  if (command === 'utility-to-gray' || command === 'utility-mirror') {
    return {
      app_request_model: 'clearra-app/AppRequest',
      command,
      language: input.language ?? 'en',
      format: input.format ?? 'ctk3',
      document: input.document ?? ''
    };
  }
  const base: ClearraDesktopRequestBase = {
    app_request_model: 'clearra-app/AppRequest',
    language: input.language ?? 'en',
    lines: input.lines ?? 2,
    queue: input.queue ?? '',
    patterns: input.patterns ?? '',
    queue_knowledge: input.queue_knowledge ?? 'oracle',
    hold_enabled: input.hold_enabled ?? true,
    hold_piece: input.hold_piece ?? 'empty',
    backend: input.backend ?? 'auto',
    rule: input.rule ?? 'srs-plus',
    score_mode: input.score_mode ?? 'off',
    score_profile: input.score_profile ?? 'tetrio',
    spin_profile: input.spin_profile ?? (command === 'ren' ? 'disabled' : 't-spins'),
    preserve_b2b: input.preserve_b2b ?? false,
    precompute_build_dependencies: input.precompute_build_dependencies ?? false,
    finesse: input.finesse ?? 'off',
    pattern_knowledge: input.pattern_knowledge ?? 'both',
    board_mask: input.board_mask ?? '0x0000000000000000',
    visible_height: input.visible_height ?? input.lines ?? 2,
    piece_window: input.piece_window ?? null,
    count_policy: input.count_policy ?? 'unique',
    solution_probabilities: input.solution_probabilities ?? false,
    workers: input.workers ?? 0,
    use_all_logical_processors: input.use_all_logical_processors ?? false,
    gpu_device: input.gpu_device ?? 'auto',
    allow_backend_fallback:
      input.allow_backend_fallback ?? ((input.backend ?? 'auto') === 'auto'),
    memory_budget_mb: input.memory_budget_mb ?? 0,
    candidate_budget: input.candidate_budget ?? 10_000_000,
    pattern_budget: input.pattern_budget ?? 5040,
    tablebase_requested: input.tablebase_requested ?? false,
    setup_mode: input.setup_mode ?? 'oracle',
    setup_remaining: input.setup_remaining ?? 'IOTSZJL',
    setup_qb: input.setup_qb ?? '',
    setup_next_cycle_remaining: input.setup_next_cycle_remaining ?? '',
    setup_allow_post_cycle_borrow: input.setup_allow_post_cycle_borrow ?? false,
    setup_priority: input.setup_priority ?? 'all',
    setup_length: input.setup_length ?? 'auto',
    setup_max_pieces: input.setup_max_pieces ?? 9,
    setup_path_setup_id: input.setup_path_setup_id,
    setup_path_condition_id: input.setup_path_condition_id,
    base_mask: input.base_mask ?? '0x0',
    target_mask: input.target_mask ?? '0x0',
    build_aggregation: input.build_aggregation ?? 'buildability',
    include_horizontal_mirror: input.include_horizontal_mirror ?? true,
  };

  if (command === 'damage') {
    const damageAggregation = input.damage_aggregation ?? 'maximum';
    return {
      ...base,
      command,
      initial_combo: input.initial_combo ?? 0,
      initial_b2b: input.initial_b2b ?? 0,
      damage_aggregation: damageAggregation,
      ...(damageAggregation === 'at-least'
        ? { minimum_damage: input.minimum_damage ?? 0 }
        : {})
    };
  }
  if (command === 'spin-finder') {
    return {
      ...base,
      command,
      spin_lines: input.spin_lines ?? 'any',
      spin_category: input.spin_category ?? 'any'
    };
  }
  if (command === 'pc' || command === 'pc-scenario') {
    return {
      ...base,
      command,
      initial_b2b: input.initial_b2b ?? 0
    };
  }
  if (command === 'build-probability') {
    return {
      ...base,
      command,
      ...(input.source_piece_count === undefined
        ? {}
        : { source_piece_count: input.source_piece_count })
    };
  }
  if (command === 'ren') return { ...base, command };
  return { ...base, command: 'setup' };
}

export function buildDesktopBuildV2Request(
  request: Omit<
    ClearraDesktopBuildV2Request,
    'app_request_model' | 'command' | 'backend' | 'allow_backend_fallback'
  >
): ClearraDesktopBuildV2Request {
  return {
    app_request_model: 'clearra-app/AppRequest',
    command: 'build-v2',
    backend: 'cpu',
    allow_backend_fallback: false,
    ...request
  };
}

export function buildDesktopSetupScoreRequest(
  request: Omit<
    ClearraDesktopSetupScoreRequest,
    'app_request_model' | 'command' | 'backend' | 'allow_backend_fallback'
  >
): ClearraDesktopSetupScoreRequest {
  return {
    app_request_model: 'clearra-app/AppRequest',
    command: 'setup-score',
    backend: 'cpu',
    allow_backend_fallback: false,
    ...request
  };
}

export function buildDesktopSpinStructureRequest(
  request: ClearraDesktopSpinStructureRequestInput
): ClearraDesktopSpinStructureRequest {
  return {
    app_request_model: 'clearra-app/AppRequest',
    command: 'spin-structure',
    backend: 'cpu',
    allow_backend_fallback: false,
    ...request
  } as ClearraDesktopSpinStructureRequest;
}

export async function runRequest(
  request: ClearraDesktopRequest
): Promise<ClearraDesktopAppResponse> {
  const response = await invoke<string>('run_request', { requestJson: JSON.stringify(request) });
  return JSON.parse(response) as ClearraDesktopAppResponse;
}

export async function validateRequest(request: ClearraDesktopRequest): Promise<unknown> {
  const response = await invoke<string>('validate_request', {
    requestJson: JSON.stringify(request)
  });
  return JSON.parse(response);
}

export async function startJob(request: ClearraDesktopRequest): Promise<number> {
  return await invoke<number>('start_job', { requestJson: JSON.stringify(request) });
}

export async function cancelJob(jobId: number): Promise<void> {
  await invoke<void>('cancel_job', { jobId });
}

export async function getJobEvents(jobId: number): Promise<ClearraDesktopJobEvent[]> {
  const response = await invoke<string>('get_job_events', { jobId });
  return JSON.parse(response) as ClearraDesktopJobEvent[];
}

export async function loadNextProductPage(
  maximumWorkSteps = 10_000,
  signal?: AbortSignal
): Promise<ClearraProductPageWorkerPayload> {
  if (signal?.aborted) throw abortError(signal);
  const releaseOnAbort = () => {
    void releaseProductPages().catch(() => undefined);
  };
  signal?.addEventListener('abort', releaseOnAbort, { once: true });
  try {
    if (signal?.aborted) {
      releaseOnAbort();
      throw abortError(signal);
    }
    const response = await invoke<string>('product_page_next', {
      maximumWorkSteps
    });
    if (signal?.aborted) throw abortError(signal);
    return JSON.parse(response) as ClearraProductPageWorkerPayload;
  } catch (error) {
    if (signal?.aborted) throw abortError(signal);
    throw error;
  } finally {
    signal?.removeEventListener('abort', releaseOnAbort);
  }
}

export async function loadProductMemberPage(
  alternativeIndex: string,
  memberPageNumber: string,
  signal?: AbortSignal
): Promise<ClearraProductPageWorkerPayload> {
  if (signal?.aborted) throw abortError(signal);
  requireCanonicalProductPageCoordinate(alternativeIndex, 'alternative index');
  requireCanonicalProductPageCoordinate(memberPageNumber, 'member page number');
  const releaseOnAbort = () => {
    void releaseProductPages().catch(() => undefined);
  };
  signal?.addEventListener('abort', releaseOnAbort, { once: true });
  try {
    if (signal?.aborted) {
      releaseOnAbort();
      throw abortError(signal);
    }
    const response = await invoke<string>('product_page_get', {
      alternativeIndex,
      memberPageNumber
    });
    if (signal?.aborted) throw abortError(signal);
    return JSON.parse(response) as ClearraProductPageWorkerPayload;
  } catch (error) {
    if (signal?.aborted) throw abortError(signal);
    throw error;
  } finally {
    signal?.removeEventListener('abort', releaseOnAbort);
  }
}

function requireCanonicalProductPageCoordinate(value: string, label: string): void {
  if (!/^[1-9][0-9]*$/u.test(value)) {
    throw new Error(`${label} must be a canonical positive decimal string`);
  }
}

export async function releaseProductPages(): Promise<void> {
  await invoke<void>('product_page_release');
}

function abortError(signal: AbortSignal): Error {
  if (signal.reason instanceof Error) return signal.reason;
  const error = new Error('Product page load was aborted.');
  error.name = 'AbortError';
  return error;
}

export type ClearraGpuWarmupReport = {
  state: 'connected' | 'unavailable';
  device_index: number | null;
  device_name: string | null;
  unavailable_reason: string | null;
  session_cached: boolean;
  session_reused: boolean;
  initialization_elapsed_ns: number;
};

export async function prewarmSearchBackend(
  gpuDevice: number | null = null
): Promise<ClearraGpuWarmupReport> {
  const response = await invoke<string>('prewarm_search_backend', { gpuDevice });
  return JSON.parse(response) as ClearraGpuWarmupReport;
}
