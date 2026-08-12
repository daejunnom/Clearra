/**
 * The public Discord identity is the trust boundary for result projection.
 * Each row names the only engine result kind(s) the curated translator may
 * return and the timeout class applied to that public search path.
 */
export const DISCORD_PUBLIC_SEARCH_CONTRACT = Object.freeze([
  row("path", "pc-scenario", "reverse"),
  row("percent", "pc-scenario", "reverse"),
  row("chance", "pc-scenario", "reverse"),
  row("minimals", "pc-scenario", "reverse"),
  row("score", "pc-scenario", "reverse"),
  row("score-minimals", "pc-scenario", "reverse"),
  row("saves", "pc-scenario", "reverse"),
  row("best-save", "pc-scenario", "reverse"),
  row("cover", "build-probability", "forward"),
  row("setup", "build-probability", "forward"),
  row("congruent", "build-probability", "forward"),
  row("congruent-cover", "build-probability", "forward"),
  row("setup-cover", "build-probability", "forward"),
  row("cover-percent", "build-probability", "forward"),
  row("special-cover", "build-probability", "forward"),
  row("spin-cover", "spin-finder", "forward"),
  row("spin", "spin-finder", "forward"),
  row("score-finder", "pc-scenario", "reverse"),
  row("damage", "damage", "forward"),
  row("spin-structure", "spin-structure", "forward"),
  row("pc-setup", "setup", "setup"),
  row("best-setup", "setup", "setup"),
  row("dpc-finder", "setup", "setup"),
  row("finesse-search", "build-probability", "forward"),
  row("finesse-score", "build-probability", "forward"),
  row("verify", ["verify", "verify-kicks"], "default"),
]);

function row(id, engineKinds, timeoutClass) {
  const kinds = Array.isArray(engineKinds) ? engineKinds : [engineKinds];
  return Object.freeze({
    id,
    engineKinds: Object.freeze([...kinds]),
    resultKey: id.replaceAll("-", "_"),
    timeoutClass,
  });
}
