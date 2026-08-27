import {
  activeDiscordSearchCapabilities,
  activeDiscordGenericCompatibilityRoutes,
  hiddenTextSearchCapabilities,
} from "./capability-registry.mjs";

/**
 * Result projection is generated from typed product capabilities plus the
 * separately governed generic-compatibility routes. Route authority IDs keep
 * identical user-facing labels from cross-accepting engine result kinds.
 */
export const DISCORD_PUBLIC_SEARCH_CONTRACT = Object.freeze(
  resultRows(
    activeDiscordSearchCapabilities(),
    activeDiscordGenericCompatibilityRoutes(),
  ),
);

// Hidden text diagnostics use the same fail-closed result translator without
// becoming public slash/help capabilities.
export const DISCORD_HIDDEN_TEXT_SEARCH_CONTRACT = Object.freeze(
  resultRows(hiddenTextSearchCapabilities()),
);

function resultRows(capabilities, genericRoutes = []) {
  const rows = new Map();
  for (const capability of capabilities) {
    for (const route of [capability.canonical, ...capability.aliases]) {
      if (!route.slash && !route.text) continue;
      const publicResultKind = route.publicResultKind ??
        (route.subcommand ? capability.publicResultKind : route.root);
      const id = route.resultAuthorityId ?? publicResultKind;
      if (!id || !publicResultKind) continue;
      const candidate = row(id, publicResultKind, capability, route);
      const previous = rows.get(id);
      if (previous && !sameContract(previous, candidate)) {
        throw new Error(`Discord result identity '${id}' has conflicting capability contracts.`);
      }
      rows.set(id, candidate);
    }
  }
  for (const route of genericRoutes) {
    const candidate = row(
      route.resultAuthorityId,
      route.publicResultKind,
      route,
    );
    const previous = rows.get(candidate.id);
    if (previous && !sameContract(previous, candidate)) {
      throw new Error(
        `Discord result identity '${candidate.id}' has conflicting route contracts.`,
      );
    }
    rows.set(candidate.id, candidate);
  }
  return [...rows.values()];
}

function row(id, publicResultKind, capability, route = null) {
  const problemContractId = route?.problemContractId ?? capability.problemContractId;
  const resultContractId = route?.resultContractId ?? capability.resultContractId;
  const effectClasses = route?.effectClasses ?? capability.effectClasses;
  const engineKinds = route?.engineKinds ?? capability.engineKinds;
  return Object.freeze({
    id,
    publicResultKind,
    capabilityId: capability.id,
    problemContractId,
    resultContractId,
    algorithmFamily: capability.algorithmFamily,
    effectClasses: Object.freeze([...effectClasses]),
    productTimeoutClass: capability.timeoutClass,
    timeoutClass: capability.timeoutClass,
    engineKinds: Object.freeze([...engineKinds]),
    telemetryIdentity: capability.telemetryIdentity,
    loweringAuthority: capability.loweringAuthority,
    resultKey: publicResultKind.replaceAll("-", "_"),
  });
}

function sameContract(left, right) {
  return left.publicResultKind === right.publicResultKind &&
    left.problemContractId === right.problemContractId &&
    left.resultContractId === right.resultContractId &&
    left.algorithmFamily === right.algorithmFamily &&
    JSON.stringify(left.effectClasses) === JSON.stringify(right.effectClasses) &&
    left.productTimeoutClass === right.productTimeoutClass &&
    left.timeoutClass === right.timeoutClass &&
    JSON.stringify(left.engineKinds) === JSON.stringify(right.engineKinds) &&
    left.telemetryIdentity === right.telemetryIdentity &&
    left.loweringAuthority === right.loweringAuthority;
}
