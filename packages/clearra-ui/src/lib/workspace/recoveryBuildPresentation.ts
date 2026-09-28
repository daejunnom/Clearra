import type { RecoveryBuildPayload, RecoveryBuildExamplePayload } from './recoveryBuildPayloadTypes';
import type { PathReplayGeometryWitness } from './pcPathReplayPresentation';
import { buildPcPathReplayFrames, pcPathWitnessExportPage } from './pcPathReplayPresentation';
import { compactRecoveryBoard, countRecoveryCells } from './recoveryBuildModel';
import type { SolutionExportPage, SolutionPiece } from './solutionExport';

const decimal = (x: unknown): x is string => typeof x === 'string' && /^(0|[1-9][0-9]*)$/u.test(x);
const hex = (x: unknown): x is string => typeof x === 'string' && /^0x[0-9a-f]{1,64}$/u.test(x);
const piece = (x: unknown): x is string => typeof x === 'string' && /^[IJLOSTZ]$/u.test(x);
const number = (x: unknown): x is number => typeof x === 'number' && Number.isSafeInteger(x);
const flag = (x: unknown): x is boolean => typeof x === 'boolean';
const probability = (x: unknown): x is string => typeof x === 'string' && /^(?:0(?:\.[0-9]+)?|1(?:\.0+)?|[0-9]+(?:\.[0-9]+)?e-[0-9]+)$/u.test(x) && Number(x) >= 0 && Number(x) <= 1;
function mirror(mask: bigint, height: number): bigint {
  let result = 0n;
  for (let y = 0; y < height; y++) for (let x = 0; x < 10; x++) {
    if (mask & (1n << BigInt(y*10+x))) result |= 1n << BigInt(y*10+9-x);
  }
  return result;
}
function require(ok: unknown): asserts ok { if (!ok) throw new Error('invalid recovery-build evidence'); }

export const recoveryBuildTerminalMask = (example: RecoveryBuildExamplePayload): string => `0x${BigInt(example.terminal_board_mask).toString(16).padStart(64,'0')}`;
const replayMask = (mask: string): string => `0x${BigInt(mask).toString(16).padStart(64,'0')}`;

export function recoveryBuildWitness(report: RecoveryBuildPayload, example: RecoveryBuildExamplePayload): PathReplayGeometryWitness {
  return { maskHexDigits: 64, candidate_id: `${report.input_identity}:${example.status}`, pattern_id: `${example.first_pattern}:${example.second_pattern}`,
    normalized_trace_key: JSON.stringify(example.steps), steps: example.steps.map((step, index) => ({
      step_index: String(index), active_piece: step.piece, placement_mask: replayMask(step.placement_mask),
      board_before_mask: replayMask(step.board_before_mask),
      board_after_placement_mask: replayMask(`0x${(BigInt(step.board_before_mask) | BigInt(step.placement_mask)).toString(16)}`),
      board_after_line_clear_mask: replayMask(step.board_after_mask), cleared_row_mask: replayMask(`0x${step.cleared_rows.toString(16)}`),
      cleared_lines: String(step.cleared_lines) })) };
}

/** Independently checks transport, source use, actual clears and target ownership.
 * Reachability/kick evidence remains the producer's authority; it is not invented
 * from a picture or from this geometric replay projection. */
export function validateRecoveryBuildPayload(value: unknown): value is RecoveryBuildPayload {
  try {
    const p = value as RecoveryBuildPayload;
    require(p && /^[0-9a-f]{64}$/u.test(p.input_identity) && number(p.height) && p.height >= 1 && p.height <= 24);
    const bound = 1n << BigInt(p.height * 10);
    require([p.start_board_mask, p.middle_target_mask, p.result_target_mask].every(x => hex(x) && BigInt(x) < bound));
    const start = BigInt(p.start_board_mask), middle = BigInt(p.middle_target_mask), result = BigInt(p.result_target_mask);
    const n = countRecoveryCells(middle) / 4, m = countRecoveryCells(result) / 4;
    require(Number.isInteger(n) && n > 0 && Number.isInteger(m) && m > 0 && !(start & middle));
    const base = compactRecoveryBoard(start | middle, p.height);
    require(!(base & result));
    require(typeof p.first_supply === 'string' && p.first_supply.trim() && typeof p.second_supply === 'string' && p.second_supply.trim());
    require(p.early_limit === null || decimal(p.early_limit));
    require([p.allow_piece_exchange, p.hold_enabled, p.preserve_b2b, p.initial_b2b, p.complete, p.all_paths_enumerated].every(flag));
    require(p.complete && !p.all_paths_enumerated);
    require(['srs', 'srs-plus', 'srs-x', 'jstris-180'].includes(p.rule_profile));
    require(['disabled','t-spin-simple','t-spins','t-spins-plus','all-spin','all-spin-plus','all-mini','all-mini-plus'].includes(p.spin_profile));
    const counts = [p.pattern_count,p.evaluated_pattern_count,p.normal_count,p.recovery_count,p.no_path_count,p.state_count];
    require(counts.every(decimal));
    require(BigInt(p.pattern_count) > 0n && p.pattern_count === p.evaluated_pattern_count);
    require(BigInt(p.normal_count) + BigInt(p.recovery_count) + BigInt(p.no_path_count) === BigInt(p.pattern_count));
    require([p.normal_probability,p.recovery_probability,p.no_path_probability].every(probability));
    require(Math.abs(Number(p.normal_probability)+Number(p.recovery_probability)+Number(p.no_path_probability)-1) < 1e-9);
    require(Array.isArray(p.examples) && p.examples.length <= 2 && new Set(p.examples.map(e => e.status)).size === p.examples.length);
    require(p.examples.some(e => e.status === 'normal') === (BigInt(p.normal_count)>0n));
    require(p.examples.some(e => e.status === 'recovery') === (BigInt(p.recovery_count)>0n));
    for (const e of p.examples) {
      const resultHex = e.result_target_mask ?? p.result_target_mask;
      require(hex(resultHex) && BigInt(resultHex) < bound);
      const targetResult = BigInt(resultHex);
      require(targetResult === result || (base === mirror(base,p.height) && targetResult === mirror(result,p.height)));
      require(['normal','recovery'].includes(e.status) && decimal(e.first_pattern) && decimal(e.second_pattern));
      require(/^[IJLOSTZ]+$/u.test(e.first_queue) && /^[IJLOSTZ]+$/u.test(e.second_queue));
      require(decimal(e.effective_max_early) && decimal(e.actual_early));
      const effective = Math.min(m,p.early_limit === null ? Infinity : Number(p.early_limit));
      require(e.effective_max_early === String(effective) && BigInt(e.actual_early) <= BigInt(e.effective_max_early));
      require(hex(e.terminal_board_mask) && BigInt(e.terminal_board_mask) === compactRecoveryBoard(base|targetResult,p.height));
      require(Array.isArray(e.exchange_balance) && e.exchange_balance.length === 7 && e.exchange_balance.every(number));
      require(Array.isArray(e.steps) && e.steps.length === n+m);
      const combined = e.first_queue+e.second_queue;
      let active: number | null = 0, held: number | null = null, cursor = 1;
      let board = compactRecoveryBoard(start,p.height), usedMiddle = 0n, usedResult = 0n, early = 0, firstUsed = 0, b2b = p.initial_b2b;
      const balance = Array<number>(7).fill(0), deleted = new Set<number>();
      for(let y=0;y<p.height;y++) if(((start>>BigInt(y*10))&1023n)===1023n) deleted.add(y);
      const completeRows = new Set<number>();
      for(let y=0;y<p.height;y++) if((((start|middle)>>BigInt(y*10))&1023n)===1023n) completeRows.add(y);
      let liftedResult=0n, physical=0;
      for(let logical=0;physical<p.height;logical++) if(!completeRows.has(logical)) {
        liftedResult |= ((targetResult>>BigInt(physical*10))&1023n)<<BigInt(logical*10);physical++;
      }
      const usedSources = new Set<number>();
      for(const step of e.steps) {
        require(decimal(step.source_index) && piece(step.piece) && [step.result_target,step.recognized_spin,step.b2b_active,step.middle_complete].every(flag));
        require(number(step.rotation) && step.rotation>=0 && step.rotation<4 && number(step.x) && number(step.y));
        require([step.board_before_mask,step.placement_mask,step.board_after_mask].every(x=>hex(x) && BigInt(x)<bound));
        const source=Number(step.source_index);require(Number.isSafeInteger(source) && !usedSources.has(source) && combined[source]===step.piece);
        usedSources.add(source);
        if(step.hold_decision==='none') require(active===source);
        else if(step.hold_decision==='swap') { require(p.hold_enabled && active!==null && held===source);held=active; }
        else if(step.hold_decision==='store') { require(p.hold_enabled && active!==null && held===null && cursor===source);held=active;cursor++; }
        else if(step.hold_decision==='release-held-at-terminal') { require(p.hold_enabled && active===null && held===source);held=null; }
        else require(false);
        active=cursor<combined.length?cursor++:null;
        const lock=BigInt(step.placement_mask);require(countRecoveryCells(lock)===4 && !(lock&board) && BigInt(step.board_before_mask)===board);
        const map:number[]=[];for(let y=0;map.length<p.height;y++) if(!deleted.has(y))map.push(y);
        let logical=0n;for(let y=0;y<p.height;y++)logical|=((lock>>BigInt(y*10))&1023n)<<BigInt(map[y]*10);
        const target=step.result_target?liftedResult:middle, used=step.result_target?usedResult:usedMiddle;
        require((logical&target)===logical && !(logical&used));
        if(step.result_target && usedMiddle!==middle)early++;
        if(e.status==='normal' && step.result_target)require(usedMiddle===middle);
        if(step.result_target)usedResult|=logical;else usedMiddle|=logical;
        if(source<e.first_queue.length)firstUsed++;
        balance['IJLOSTZ'.indexOf(step.piece)]+=Number(source<e.first_queue.length)-Number(!step.result_target);
        let full=0;for(let y=0;y<p.height;y++) if((((board|lock)>>BigInt(y*10))&1023n)===1023n)full+=2**y;
        require(number(step.cleared_rows) && step.cleared_rows===full && step.cleared_lines===countRecoveryCells(BigInt(full)));
        board=compactRecoveryBoard(board|lock,p.height);require(board===BigInt(step.board_after_mask));
        for(let y=0;y<p.height;y++) if(full&(2**y))deleted.add(map[y]);
        if(step.cleared_lines>0)b2b=step.cleared_lines===4 || board===0n || step.recognized_spin;
        require(b2b===step.b2b_active && (!p.preserve_b2b || step.cleared_lines===0 || b2b));
        require(step.middle_complete===(usedMiddle===middle));
      }
      require(firstUsed===n && usedMiddle===middle && usedResult===liftedResult && e.actual_early===String(early));
      require(e.status==='normal'?early===0:early>0);
      require(balance.every((v,i)=>v===e.exchange_balance[i]) && balance.reduce((a,b)=>a+b,0)===0);
      require(p.allow_piece_exchange || balance.every(v=>v===0));
      buildPcPathReplayFrames(recoveryBuildWitness(p,e),p.height,recoveryBuildTerminalMask(e));
    }
    return true;
  } catch { return false; }
}

/** Cumulative target-coordinate snapshots, not one page per lock. The first
 * checkpoint is the middle stage's placement budget; it includes real early
 * result placements. Deleted rows and all colored cell ownership are retained.
 * The second checkpoint contains every placement (14 for P7/P7). */
export function recoveryBuildExamplePages(
  report: RecoveryBuildPayload, example: RecoveryBuildExamplePayload, resultOnly = false
): SolutionExportPage[] {
  const witness = recoveryBuildWitness(report, example);
  const final = pcPathWitnessExportPage(witness, report.height, recoveryBuildTerminalMask(example));
  if (!final) throw new Error('recovery history exceeds export geometry');
  if (resultOnly) return [final];
  const count = countRecoveryCells(BigInt(report.middle_target_mask)) / 4;
  const steps = witness.steps.slice(0, count);
  const terminal = steps.at(-1)?.board_after_line_clear_mask;
  if (!terminal) throw new Error('missing recovery checkpoint');
  const first = pcPathWitnessExportPage({ ...witness, steps }, report.height, terminal);
  if (!first) throw new Error('recovery checkpoint exceeds export geometry');
  return [first, final];
}
export function recoveryBuildExportPages(report: RecoveryBuildPayload, resultOnly = false): SolutionExportPage[] {
  if (!validateRecoveryBuildPayload(report)) throw new Error('invalid recovery-build output');
  return report.examples.flatMap(example => recoveryBuildExamplePages(report, example, resultOnly));
}
