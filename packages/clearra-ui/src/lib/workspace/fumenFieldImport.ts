import { decoder } from 'tetris-fumen';

export type ImportedFumenField = {
  boardMask: bigint;
  occupiedHeight: number;
};

export function decodeFumenField(input: string, maximumHeight = 6): ImportedFumenField {
  if (!Number.isInteger(maximumHeight) || maximumHeight < 1 || maximumHeight > 24) {
    throw new Error('Fumen field height limit must be between one and 24 rows.');
  }
  const pages = decoder.decode(extractFumenCode(input));
  const page = pages[0];
  if (!page) throw new Error('Fumen has no pages.');

  let boardMask = 0n;
  let occupiedHeight = 0;
  for (let y = 0; y < 23; y += 1) {
    for (let x = 0; x < 10; x += 1) {
      if (page.field.at(x, y) === '_') continue;
      if (y >= maximumHeight) throw new Error(`Fumen field exceeds the ${maximumHeight}-line range.`);
      boardMask |= 1n << BigInt(y * 10 + x);
      occupiedHeight = Math.max(occupiedHeight, y + 1);
    }
  }
  return { boardMask, occupiedHeight };
}

function extractFumenCode(input: string): string {
  let decoded = input.trim();
  try {
    decoded = decodeURIComponent(decoded);
  } catch {
    // Raw Fumen text is already in the decoder's expected form.
  }
  const match = decoded.match(/v11(?:0|5)@[A-Za-z0-9+/?]+/);
  if (!match) throw new Error('No v110 or v115 Fumen code was found.');
  return match[0];
}
