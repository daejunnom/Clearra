/**
 * The run grant authorizes the receiver-owned buffer. The synchronous WASM
 * start call copies it into linear memory, so the staged request must not
 * keep the JS backing store alive across solver yields.
 */
export function takeVerifierBatch(owner: { batch: ArrayBuffer | null }): ArrayBuffer {
  const batch = owner.batch;
  if (!(batch instanceof ArrayBuffer)) throw new Error('verifier batch is missing or already consumed');
  owner.batch = null;
  return batch;
}
