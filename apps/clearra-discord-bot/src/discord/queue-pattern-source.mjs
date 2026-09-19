const STANDARD_PIECES = "IOTSZJL";

export function parseQueuePatternSource(source, { name = "next", maxLength = 2048 } = {}) {
  if (typeof source !== "string") {
    throw new Error(`${name} must be text.`);
  }
  const trimmed = source.trim();
  if (!trimmed) throw new Error(`${name} must not be empty.`);
  if ([...trimmed].length > maxLength) {
    throw new Error(`${name} exceeds the ${maxLength}-character limit.`);
  }
  const normalized = normalizeSfinderPatternForLength(trimmed, name);
  const lengths = normalized.split(";").map((alternative) => (
    patternAlternativeLength(alternative, name)
  ));
  if (lengths.some((length) => length !== lengths[0])) {
    throw new Error(`${name} pattern alternatives must have the same piece count.`);
  }
  return Object.freeze({
    source: normalized,
    sequenceLength: lengths[0],
    kind: /^[IOTSZJL]+$/.test(normalized) ? "fixed" : "pattern",
  });
}

export function isStandardBagQueue(source) {
  for (let start = 0; start < source.length; start += 7) {
    const bag = source.slice(start, start + 7);
    if (new Set(bag).size !== bag.length) return false;
  }
  return true;
}

function normalizeSfinderPatternForLength(source, name) {
  const characters = [...source];
  let output = "";
  for (let index = 0; index < characters.length; index += 1) {
    const character = characters[index];
    if (/\s/.test(character) || character === ",") continue;
    if (character === "*") {
      if (characters[index + 1] === "!") {
        output += "P7";
        index += 1;
        continue;
      }
      if (/^[pP]$/.test(characters[index + 1] ?? "")) {
        output += "P";
        index += 1;
        continue;
      }
      throw new Error(`${name} '*' must be followed immediately by ! or pN.`);
    }
    if (/^[pP]$/.test(character) && index > 0 && characters[index - 1] === "]") {
      continue;
    }
    output += asciiUppercase(character);
  }
  return output;
}

function asciiUppercase(character) {
  const code = character.codePointAt(0);
  return code >= 0x61 && code <= 0x7a
    ? String.fromCodePoint(code - 0x20)
    : character;
}

function patternAlternativeLength(source, name) {
  if (!source) throw new Error(`${name} pattern contains an empty alternative.`);
  let count = 0;
  let cursor = 0;
  while (cursor < source.length) {
    const character = source[cursor];
    if (STANDARD_PIECES.includes(character)) {
      count += 1;
      cursor += 1;
      continue;
    }
    if (character === "P") {
      const parsed = readPatternCount(source, cursor + 1, name);
      if (parsed.value > 7) {
        throw new Error(`${name} standard-bag draws may not exceed seven pieces per group.`);
      }
      count += parsed.value;
      cursor = parsed.cursor;
      continue;
    }
    if (character === "[") {
      const close = source.indexOf("]", cursor + 1);
      if (close < 0) throw new Error(`${name} pattern has an unterminated piece group.`);
      const group = source.slice(cursor + 1, close);
      const complement = group.startsWith("^");
      const pieces = complement ? group.slice(1) : group;
      if (!pieces || !/^[IOTSZJL]+$/.test(pieces)) {
        throw new Error(`${name} pattern contains an invalid piece group.`);
      }
      const unique = new Set(pieces);
      const groupSize = complement ? 7 - unique.size : unique.size;
      if (groupSize < 1) {
        throw new Error(`${name} pattern piece group must leave at least one choice.`);
      }
      cursor = close + 1;
      if (source[cursor] === "!") {
        count += groupSize;
        cursor += 1;
        continue;
      }
      if (source[cursor] === "P") {
        throw new Error(`${name} pattern has an unexpected bag token after a piece group.`);
      }
      if (!/\d/.test(source[cursor] ?? "")) {
        count += 1;
        continue;
      }
      const parsed = readPatternCount(source, cursor, name);
      if (parsed.value > groupSize) {
        throw new Error(`${name} pattern draws more pieces than its group contains.`);
      }
      count += parsed.value;
      cursor = parsed.cursor;
      continue;
    }
    throw new Error(`${name} pattern contains unsupported token '${character}'.`);
  }
  if (count < 1) throw new Error(`${name} pattern must contain at least one piece.`);
  return count;
}

function readPatternCount(source, cursor, name) {
  const start = cursor;
  while (cursor < source.length && /\d/.test(source[cursor])) cursor += 1;
  if (cursor === start) throw new Error(`${name} pattern draw count is missing.`);
  const value = Number(source.slice(start, cursor));
  if (!Number.isSafeInteger(value) || value < 1) {
    throw new Error(`${name} pattern draw count must be a positive integer.`);
  }
  return { value, cursor };
}
