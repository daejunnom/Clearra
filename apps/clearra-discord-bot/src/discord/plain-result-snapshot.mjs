// Shared mechanics for Discord's JSON-like results and capability snapshots.
// The owning projection still decides which fields it may expose.
export function plainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

export function clonePlain(value) {
  if (Array.isArray(value)) return value.map(clonePlain);
  if (plainObject(value)) {
    return Object.fromEntries(
      Object.entries(value).map(([key, nested]) => [key, clonePlain(nested)]),
    );
  }
  return value;
}

export function deepFreeze(value) {
  if (!value || typeof value !== "object" || Object.isFrozen(value)) return value;
  for (const nested of Object.values(value)) deepFreeze(nested);
  return Object.freeze(value);
}

// Used only by projections that hide every attack observation. Score
// minimals retains its separate path-sensitive policy marker.
export function stripAttackFields(value) {
  if (Array.isArray(value)) return value.map(stripAttackFields);
  if (!plainObject(value)) return value;
  return Object.fromEntries(Object.entries(value)
    .filter(([key]) => !key.toLowerCase().includes("attack"))
    .map(([key, nested]) => [key, stripAttackFields(nested)]));
}
