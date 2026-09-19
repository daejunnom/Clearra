export type Pc4GraphBlockDirectory = Readonly<{
  bytes: Uint8Array;
  contentIdentity: string;
  blockRecords: number;
  fieldCount: number;
  maximumBlockBytes: number;
}>;

export function buildPc4GraphBlockDirectory(
  readOffsets: (offset: number, length: number) => Promise<Uint8Array>,
  options: { fieldCount: number; graphBytes: number; blockRecords?: number; maxBlockBytes?: number;
    readBytes?: number },
): Promise<Pc4GraphBlockDirectory>;

export function parsePc4HydraGraphBlock(
  bytes: Uint8Array,
  options: { recordCount: number; targetWidth: 3 | 4; fieldCount: number },
): ReadonlyArray<readonly [number, number]>;
