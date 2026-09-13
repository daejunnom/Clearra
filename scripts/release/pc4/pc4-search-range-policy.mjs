// Transport sizing for an explicitly selected, qualified graph profile. The
// application still admits every exact byte range and owns graph semantics.
// Do not apply large uniform windows to one-shot graph records: real P7P4
// lookup traces exhaust the byte limit long before their useful reads finish.
import { Pc4OnlineError } from './pc4-range-reader.mjs';
export function pc4SearchRangePolicy(generation, profile) {
  const selected = generation?.profiles?.filter(p => p.profile === profile);
  if (selected?.length !== 1 || selected[0].status !== 'ready' ||
      !selected[0].artifacts?.graph?.path) throw new Pc4OnlineError('pc4_online_profile_not_qualified');
  return { windowBytes: 4096, directPaths: [selected[0].artifacts.graph.path] };
}
