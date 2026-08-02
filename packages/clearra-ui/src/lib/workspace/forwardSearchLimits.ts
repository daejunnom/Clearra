export const MAX_FORWARD_CHAIN = 0xffff;

export function isValidForwardChain(value: number): boolean {
  return Number.isInteger(value) && value >= 0 && value <= MAX_FORWARD_CHAIN;
}
