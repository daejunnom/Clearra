// SRP: validate and load owner-bound, bounded PC replay pages; only explicit
// export materializes every witness of the currently selected geometry.
import type { ClearraPcPathStepPayload, ClearraPcPathWitnessPayload, ClearraPcReplayRuntimePage } from '../wasm/wasmCommandClient';
import type { ProductMemberPageLoader } from './productResultPager';
import { pcPathWitnessExportPage } from './pcPathReplayPresentation';
import type { SolutionExportPage } from './solutionExport';

const PAGE_SIZE = 100n;
const decimal = (value: unknown): value is string =>
  typeof value === 'string' && /^(?:0|[1-9][0-9]*)$/u.test(value);
const signed = (value: string) => /^(?:0|[1-9][0-9]*|-[1-9][0-9]*)$/u.test(value);
const nonempty = (value: unknown): value is string => typeof value === 'string' && value.length > 0;
const piece = (value: unknown) => value === null || (typeof value === 'string' && /^[IOTSZJL]$/u.test(value));

export function comparePcReplayWitnesses(left: ClearraPcPathWitnessPayload, right: ClearraPcPathWitnessPayload): number {
  for (const key of ['candidate_id', 'pattern_id'] as const) {
    const comparison = BigInt(left[key]) - BigInt(right[key]);
    if (comparison !== 0n) return comparison < 0n ? -1 : 1;
  }
  for (const key of ['normalized_trace_key', 'trace_identity'] as const) {
    if (left[key] !== right[key]) return left[key] < right[key] ? -1 : 1;
  }
  return 0;
}

export function validatePcReplayWitnesses(witnesses: readonly ClearraPcPathWitnessPayload[]): boolean {
  if (!Array.isArray(witnesses)) return false;
  return witnesses.every((witness, index) => witness != null &&
    [witness.candidate_id, witness.producer_candidate_id, witness.pattern_id, witness.consumed_piece_count].every(decimal) &&
    nonempty(witness.trace_identity) && nonempty(witness.normalized_trace_key) && piece(witness.terminal_hold_piece) &&
    Array.isArray(witness.steps) && witness.steps.length > 0 &&
    witness.steps.every((step: ClearraPcPathStepPayload, stepIndex: number) => step != null &&
      step.step_index === String(stepIndex) && nonempty(step.operation_id) &&
      typeof step.active_piece === 'string' && /^[IOTSZJL]$/u.test(step.active_piece) &&
      [step.input_cursor, step.output_cursor, step.cleared_lines].every(decimal) &&
      piece(step.input_hold_piece) && piece(step.output_hold_piece) &&
      nonempty(step.hold_decision) && nonempty(step.rotation) && signed(step.x) && signed(step.y) &&
      [step.placement_mask, step.board_before_mask, step.board_after_placement_mask,
        step.board_after_line_clear_mask, step.cleared_row_mask].every((mask) => /^0x[0-9a-f]{16}$/u.test(mask)) &&
      nonempty(step.line_clear_identity)) &&
    (index === 0 || comparePcReplayWitnesses(witnesses[index - 1]!, witness) < 0));
}

export function validatePcReplayPage(page: ClearraPcReplayRuntimePage): string | null {
  if (!page || page.page_contract !== 'pc-replay-member-page.v2' ||
      page.page_source_available !== true || !/^[0-9a-f]{64}$/u.test(page.page_source_identity_sha256) ||
      ![page.geometry_count, page.geometry_page_number, page.candidate_id,
        page.geometry_witness_count, page.geometry_pattern_count, page.member_page_number,
        page.member_page_count, page.witness_count, page.materialized_pattern_count].every(decimal) ||
      !Array.isArray(page.witnesses) || page.witnesses.length > 100 ||
      !validatePcReplayWitnesses(page.witnesses)) return 'invalid PC replay page contract';
  const geometry = BigInt(page.geometry_page_number);
  const geometries = BigInt(page.geometry_count);
  const count = BigInt(page.geometry_witness_count);
  const pageNumber = BigInt(page.member_page_number);
  const pageCount = BigInt(page.member_page_count);
  const patterns = BigInt(page.geometry_pattern_count);
  const offset = (pageNumber - 1n) * PAGE_SIZE;
  const remaining = count - offset;
  if (geometry < 1n || geometry > geometries || count < 1n ||
      BigInt(page.witness_count) < count + geometries - 1n || pageNumber < 1n || pageNumber > pageCount ||
      pageCount !== (count + PAGE_SIZE - 1n) / PAGE_SIZE || patterns < 1n ||
      patterns > count || patterns > BigInt(page.materialized_pattern_count) ||
      BigInt(new Set(page.witnesses.map((witness) => witness.pattern_id)).size) > patterns ||
      BigInt(page.witnesses.length) !== (remaining < PAGE_SIZE ? remaining : PAGE_SIZE) ||
      page.witnesses.some((witness) => witness.candidate_id !== page.candidate_id ||
        BigInt(witness.pattern_id) >= BigInt(page.materialized_pattern_count) ||
        witness.steps.at(-1)?.board_after_line_clear_mask !== '0x0000000000000000')) {
    return 'invalid PC replay page counts or witness identity';
  }
  return null;
}

const GLOBAL_FIELDS = ['page_source_identity_sha256', 'geometry_count', 'witness_count', 'materialized_pattern_count'] as const;
const GEOMETRY_FIELDS = ['candidate_id', 'geometry_witness_count', 'geometry_pattern_count', 'member_page_count'] as const;

function ensureCurrent(signal: AbortSignal | undefined, isCurrent: () => boolean) {
  if (signal?.aborted || !isCurrent()) throw new DOMException('Replay page request is stale or cancelled.', 'AbortError');
}

export async function loadPcReplayPage(options: {
  loadMemberPage: ProductMemberPageLoader;
  reference: ClearraPcReplayRuntimePage;
  geometryPageNumber: string;
  memberPageNumber: string;
  signal?: AbortSignal;
  isCurrent: () => boolean;
}): Promise<ClearraPcReplayRuntimePage> {
  const { reference, geometryPageNumber, memberPageNumber, signal, isCurrent } = options;
  ensureCurrent(signal, isCurrent);
  if (validatePcReplayPage(reference) || !decimal(geometryPageNumber) || geometryPageNumber === '0' ||
      !decimal(memberPageNumber) || memberPageNumber === '0') {
    throw new Error('Invalid PC replay page reference or requested ordinal.');
  }
  let event;
  for (;;) {
    event = await options.loadMemberPage(geometryPageNumber, memberPageNumber, signal, 64);
    ensureCurrent(signal, isCurrent);
    if (event.product_page_kind !== 'pc-replay' || event.schema_version !== 1 ||
        !['clearra-wasm', 'clearra-desktop'].includes(event.runtime)) {
      throw new Error('The active result did not return a PC replay page.');
    }
    if (event.state === 'page') break;
    if (event.page_contract !== 'pc-replay-member-page.v2' ||
        event.page_source_identity_sha256 !== reference.page_source_identity_sha256 ||
        event.geometry_page_number !== geometryPageNumber || event.member_page_number !== memberPageNumber ||
        !decimal(event.work_steps) || BigInt(event.work_steps) > 64n) {
      throw new Error('The pending PC replay page is not bound to the requested source and geometry.');
    }
    if (event.state === 'cancelled') throw new DOMException('Replay page request was cancelled.', 'AbortError');
    if (event.state !== 'pending' || event.work_steps === '0') throw new Error('The PC replay page made no bounded progress.');
    // A host turn must be available for cancellation, source replacement and
    // UI input even when a mock/local bridge settles its Promise immediately.
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    ensureCurrent(signal, isCurrent);
  }
  const page = event.page;
  const failure = validatePcReplayPage(page);
  if (failure) throw new Error(failure);
  if (page.geometry_page_number !== geometryPageNumber || page.member_page_number !== memberPageNumber ||
      GLOBAL_FIELDS.some((key) => page[key] !== reference[key]) ||
      (geometryPageNumber === reference.geometry_page_number && GEOMETRY_FIELDS.some((key) => page[key] !== reference[key]))) {
    throw new Error('The PC replay page is not bound to the requested source and geometry.');
  }
  const direction = BigInt(geometryPageNumber) - BigInt(reference.geometry_page_number);
  const candidateDirection = BigInt(page.candidate_id) - BigInt(reference.candidate_id);
  if ((direction > 0n && candidateDirection <= 0n) || (direction < 0n && candidateDirection >= 0n)) {
    throw new Error('The PC replay geometry order is invalid.');
  }
  return page;
}

export async function collectPcReplayGeometryExportPages(options: {
  initialPage: ClearraPcReplayRuntimePage;
  loadMemberPage: ProductMemberPageLoader;
  targetLines: number;
  signal?: AbortSignal;
  isCurrent: () => boolean;
}): Promise<SolutionExportPage[]> {
  const { initialPage, signal, isCurrent } = options;
  const failure = validatePcReplayPage(initialPage);
  if (failure || initialPage.member_page_number !== '1') throw new Error(failure ?? 'Replay export must begin at the first member page.');
  const output: SolutionExportPage[] = [];
  const exportBudget = createReplayExportBudget(initialPage.geometry_witness_count);
  let previous: ClearraPcPathWitnessPayload | null = null;
  let distinctPatterns = 0n;
  let current = initialPage;
  for (let ordinal = 1n; ordinal <= BigInt(initialPage.member_page_count); ordinal += 1n) {
    ensureCurrent(signal, isCurrent);
    if (ordinal > 1n) current = await loadPcReplayPage({
      ...options, reference: initialPage, geometryPageNumber: initialPage.geometry_page_number, memberPageNumber: String(ordinal)
    });
    for (const witness of current.witnesses) {
      if (previous && comparePcReplayWitnesses(previous, witness) >= 0) throw new Error('Replay member pages overlap or are out of order.');
      if (previous?.pattern_id !== witness.pattern_id) distinctPatterns += 1n;
      exportBudget.admitConversion(witness);
      const page = pcPathWitnessExportPage(witness, options.targetLines);
      if (!page) throw new Error('A replay witness could not be converted to a field export.');
      exportBudget.retain(page);
      output.push(page);
      previous = witness;
    }
  }
  ensureCurrent(signal, isCurrent);
  if (BigInt(output.length) !== BigInt(initialPage.geometry_witness_count)) throw new Error('Replay export did not include the complete selected geometry.');
  if (distinctPatterns !== BigInt(initialPage.geometry_pattern_count)) throw new Error('Replay export pattern count does not match the selected geometry.');
  return output;
}

// A separate explicit clipboard-work admission, not a measurement of JS heap
// or a replacement for the WASM source's governed 64MiB budget. No partial
// export is returned when this conservative declared work allowance is spent.
const REPLAY_EXPORT_WORK_BYTES = 64n * 1024n * 1024n;
export function createReplayExportBudget(memberCount: string, maximumBytes = REPLAY_EXPORT_WORK_BYTES) {
  if (!decimal(memberCount) || maximumBytes <= 0n) throw new Error('Invalid replay export admission.');
  // Covers all accumulated array slots and old/new backing storage overlap.
  let retained = BigInt(memberCount) * 64n;
  const admit = (required: bigint) => {
    if (required > maximumBytes) throw new Error(`Complete replay export exceeds its work budget: required_memory_bytes=${required}, max_memory_bytes=${maximumBytes}. No partial export was copied.`);
  };
  admit(retained);
  let pendingScratch = 0n;
  return {
    admitConversion(witness: ClearraPcPathWitnessPayload) {
      if (witness.steps.length > 256) throw new Error('Replay export exceeds the admitted path depth.');
      // Frames, cells, row maps, BigInt conversion and encoder carrier reserve.
      pendingScratch = (32768n + BigInt(witness.steps.length) * 4096n) * 16n;
      admit(retained + pendingScratch);
    },
    retain(page: SolutionExportPage) {
      if (!Number.isSafeInteger(page.height) || page.height < 0 || page.height > 1056 || page.placements.length > 256) throw new Error('Replay export exceeds the admitted field shape.');
      const maskBytes = (BigInt(page.height) * 10n + 7n) / 8n;
      const added = 512n + maskBytes * 2n + BigInt(page.placements.length) * (128n + maskBytes * 2n) + BigInt(page.comment?.length ?? 0) * 2n;
      admit(retained + added + pendingScratch);
      retained += added;
      pendingScratch = 0n;
    }
  };
}
