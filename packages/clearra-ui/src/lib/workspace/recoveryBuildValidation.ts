import type { RecoveryBuildPayload } from './recoveryBuildPayloadTypes';
export const decimal = (x: unknown): x is string => typeof x === 'string' && /^(0|[1-9][0-9]*)$/u.test(x);
export const hex = (x: unknown): x is string => typeof x === 'string' && /^0x[0-9a-f]{1,64}$/u.test(x);
export const piece = (x: unknown): x is string => typeof x === 'string' && /^[IJLOSTZ]$/u.test(x);
export const number = (x: unknown): x is number => typeof x === 'number' && Number.isSafeInteger(x);
export const flag = (x: unknown): x is boolean => typeof x === 'boolean';
export const probability = (x: unknown): x is string => typeof x === 'string' && /^(?:0(?:\.[0-9]+)?|1(?:\.0+)?|[0-9]+(?:\.[0-9]+)?e-[0-9]+)$/u.test(x) && Number(x) >= 0 && Number(x) <= 1;

export function requireEvidence(ok:unknown):asserts ok {if(!ok) throw new Error('invalid recovery-build evidence');}
export function validateRecoverySummary(p:RecoveryBuildPayload):void {
  const require=requireEvidence;
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
  const solutions = p.solutions ?? [];
  require(Array.isArray(solutions));
  require(p.solutions_complete === undefined || flag(p.solutions_complete));
  require(p.minimum_proven === undefined || flag(p.minimum_proven));
  const keys = new Set<string>();
  for (const row of solutions) {
    require(typeof row.key === 'string' && row.key.length > 0 && !keys.has(row.key));
    keys.add(row.key);
    require(decimal(row.covered_count) && BigInt(row.covered_count)>0n && BigInt(row.covered_count)<=BigInt(p.pattern_count));
    require(probability(row.probability));
  }
  const selected = p.selected_solution_keys ?? [], pinned = p.required_solution_keys ?? [];
  for (const list of [selected,pinned]) require(Array.isArray(list) && new Set(list).size === list.length && list.every(key=>keys.has(key)));
  require(!p.minimum_proven || (p.solutions_complete && pinned.every(key=>selected.includes(key)) && (solutions.length===0 || selected.length>0)));
  require(p.minimum_proven || (selected.length===0 && pinned.length===0));
  require(!p.solutions_complete || ((solutions.length>0)===(BigInt(p.normal_count)+BigInt(p.recovery_count)>0n)));
}
