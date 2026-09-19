export type Pc4ReadDemand = {
  artifact: { path: string; byte_length: number; content_identity: string };
  offset: number; length: number;
};
export type Pc4ReadSpan = Pc4ReadDemand & {
  demands: Array<{ index: number; offset: number; length: number }>;
};
export function checkedPc4Read(input: Pc4ReadDemand['artifact'], offset: number, length: number): Pc4ReadDemand;
export function planPc4ReadBatch(demands: Pc4ReadDemand[], options?: { maxGapBytes?: number }): Pc4ReadSpan[];
