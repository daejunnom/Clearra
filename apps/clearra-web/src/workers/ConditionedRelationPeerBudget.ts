// SRP rationale: reserve optional verifier relation caches without changing
// compute-worker authority or hiding shared/transport memory in per-peer caps.
export const RELATION_PEER_WIRE_CAP = 256 * 1024;
const MIB = 1024 * 1024;
const SESSION_CAP = 128 * MIB;
const ROOT_TRANSIENT_RESERVE = 4 * MIB;
const ALL_PEER_CAP = 16 * MIB;

export function relationPeerReservation(rootResidentBytes: number, workerCount: number): number | null {
  if (!Number.isSafeInteger(rootResidentBytes) || rootResidentBytes < 0 ||
      !Number.isSafeInteger(workerCount) || workerCount < 1) return null;
  const available = Math.min(ALL_PEER_CAP, SESSION_CAP - rootResidentBytes - ROOT_TRANSIENT_RESERVE);
  const reservation = Math.min(2 * MIB, Math.floor(available / workerCount));
  return reservation >= MIB ? reservation : null;
}
