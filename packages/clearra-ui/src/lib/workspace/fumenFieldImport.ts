import { decodeFieldInput, type ImportedField } from './fieldInterchange';

export type ImportedFumenField = ImportedField;

export function decodeInterchangeField(
  input: string,
  maximumHeight = 6
): ImportedField {
  return decodeFieldInput(input, maximumHeight);
}

// Kept for source compatibility with callers outside the workspace package.
export const decodeFumenField = decodeInterchangeField;
