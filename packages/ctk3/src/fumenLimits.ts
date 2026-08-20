// SRP rationale: this module owns host-side resource limits for Fumen data.
export const FUMEN_MAX_SOURCE_CHARACTERS = 16 << 20;
export const FUMEN_MAX_PAGES = 4_096;

const FUMEN_WIDTH = 10;
const FUMEN_V115_FIELD_ROWS_WITH_GARBAGE = 24;
const FUMEN_V110_FIELD_ROWS_WITH_GARBAGE = 24;
const FUMEN_VALUE_ALPHABET =
  "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const FUMEN_VALUE_INDEX = new Map(
  Array.from(FUMEN_VALUE_ALPHABET, (character, index) => [character, index]),
);

export function decodeFumenWithinPageLimit<T>(
  input: string,
  decode: (boundedInput: string) => T,
  maximumPages = FUMEN_MAX_PAGES,
): T {
  inspectFumenPageCount(input, maximumPages);
  return decode(input);
}

/**
 * Counts Fumen pages directly from the compact wire stream. This guard must
 * run before tetris-fumen because unchanged-field runs can expand one small
 * input into many eagerly allocated page objects.
 */
export function inspectFumenPageCount(
  input: string,
  maximumPages = FUMEN_MAX_PAGES,
): number {
  if (input.length > FUMEN_MAX_SOURCE_CHARACTERS) {
    throw new Error("fumen-input-too-large");
  }
  if (!Number.isSafeInteger(maximumPages) || maximumPages < 1) {
    throw new RangeError("Fumen page limit is out of range.");
  }
  const marker = findFumenMarker(input);
  if (!marker) throw new Error("invalid-fumen-input");
  const fieldBlocks =
    marker.version === "115"
      ? FUMEN_WIDTH * FUMEN_V115_FIELD_ROWS_WITH_GARBAGE
      : FUMEN_WIDTH * FUMEN_V110_FIELD_ROWS_WITH_GARBAGE;
  const reader = new FumenValueReader(input, marker.dataStart);
  let pageCount = 0;
  let repeatCount = 0;

  while (!reader.isEmpty()) {
    if (pageCount === maximumPages) throw new Error("fumen-page-limit");
    if (repeatCount > 0) {
      repeatCount -= 1;
    } else if (!readFieldDiff(reader, fieldBlocks)) {
      repeatCount = reader.poll(1);
    }

    const action = reader.poll(3);
    if (actionHasComment(action, fieldBlocks)) {
      const commentLength = reader.poll(2);
      reader.skip(Math.ceil(commentLength / 4) * 5);
    }
    pageCount += 1;
  }
  if (pageCount < 1) throw new Error("invalid-fumen-input");
  return pageCount;
}

function findFumenMarker(
  input: string,
): { version: "110" | "115"; dataStart: number } | null {
  const match = /(?:v|m|d)11(0|5)@/iu.exec(input);
  if (!match) return null;
  return {
    version: match[1] === "5" ? "115" : "110",
    dataStart: match.index + match[0].length,
  };
}

function readFieldDiff(reader: FumenValueReader, fieldBlocks: number): boolean {
  let index = 0;
  let changed = true;
  while (index < fieldBlocks) {
    const diffBlock = reader.poll(2);
    const diff = Math.floor(diffBlock / fieldBlocks);
    const blockCount = diffBlock % fieldBlocks;
    if (diff > 16) throw new Error("invalid-fumen-input");
    if (diff === 8 && blockCount === fieldBlocks - 1) changed = false;
    index += blockCount + 1;
    if (index > fieldBlocks) throw new Error("invalid-fumen-input");
  }
  return changed;
}

function actionHasComment(action: number, fieldBlocks: number): boolean {
  let value = action;
  value = Math.floor(value / 8);
  value = Math.floor(value / 4);
  value = Math.floor(value / fieldBlocks);
  value = Math.floor(value / 2);
  value = Math.floor(value / 2);
  value = Math.floor(value / 2);
  return value % 2 !== 0;
}

class FumenValueReader {
  private cursor: number;

  constructor(
    private readonly input: string,
    dataStart: number,
  ) {
    this.cursor = dataStart;
  }

  poll(count: number): number {
    let value = 0;
    let digit = 0;
    while (digit < count) {
      this.skipSeparators();
      if (this.cursor >= this.input.length) throw new Error("invalid-fumen-input");
      const encoded = FUMEN_VALUE_INDEX.get(this.input[this.cursor]);
      if (encoded === undefined) throw new Error("invalid-fumen-input");
      this.cursor += 1;
      value += encoded * 64 ** digit;
      digit += 1;
    }
    return value;
  }

  skip(count: number): void {
    for (let index = 0; index < count; index += 1) this.poll(1);
  }

  isEmpty(): boolean {
    this.skipSeparators();
    return this.cursor >= this.input.length;
  }

  private skipSeparators(): void {
    while (
      this.cursor < this.input.length &&
      (this.input[this.cursor] === "?" || /\s/u.test(this.input[this.cursor]))
    ) {
      this.cursor += 1;
    }
  }
}
