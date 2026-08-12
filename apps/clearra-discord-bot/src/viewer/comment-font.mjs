export function commentGlyphAdvance(character) {
  return isHangulSyllable(character) ? 13 : 6;
}

export function paintCommentLine(pixels, width, x, y, value, color) {
  let cursor = x;
  for (const character of value) {
    if (isHangulSyllable(character)) {
      paintHangulSyllable(pixels, width, cursor, y, character, color);
    } else {
      paintAsciiGlyph(pixels, width, cursor, y, character, color);
    }
    cursor += commentGlyphAdvance(character);
  }
}

function paintAsciiGlyph(pixels, width, x, y, character, color) {
  const key = character >= "a" && character <= "z"
    ? character.toUpperCase()
    : character;
  const glyph = ASCII_FONT[key] ?? REPLACEMENT_GLYPH;
  paintBitmap(pixels, width, x, y, 5, 7, glyph, color);
}

function paintHangulSyllable(pixels, width, x, y, character, color) {
  const syllable = character.codePointAt(0) - 0xac00;
  const initial = Math.floor(syllable / 588);
  const vowel = Math.floor((syllable % 588) / 28);
  const final = syllable % 28;
  const hasFinal = final !== 0;

  if (VERTICAL_VOWELS.has(vowel)) {
    const upperHeight = hasFinal ? 8 : 12;
    paintConsonant(pixels, width, x, y, 6, upperHeight, INITIALS[initial], color);
    paintBitmap(pixels, width, x + 6, y, 6, upperHeight, VOWELS[vowel], color);
  } else if (HORIZONTAL_VOWELS.has(vowel)) {
    const initialHeight = hasFinal ? 5 : 6;
    const vowelHeight = hasFinal ? 3 : 6;
    paintConsonant(pixels, width, x + 1, y, 10, initialHeight, INITIALS[initial], color);
    paintBitmap(
      pixels,
      width,
      x + 1,
      y + initialHeight,
      10,
      vowelHeight,
      VOWELS[vowel],
      color,
    );
  } else {
    const upperHeight = hasFinal ? 8 : 12;
    paintConsonant(pixels, width, x, y, 6, Math.min(6, upperHeight), INITIALS[initial], color);
    paintBitmap(pixels, width, x + 5, y, 7, upperHeight, VOWELS[vowel], color);
  }

  if (!hasFinal) return;
  const components = FINALS[final];
  if (components.length === 1) {
    paintConsonant(pixels, width, x + 1, y + 9, 10, 3, components[0], color);
  } else {
    paintConsonant(pixels, width, x + 1, y + 9, 5, 3, components[0], color);
    paintConsonant(pixels, width, x + 6, y + 9, 5, 3, components[1], color);
  }
}

function paintConsonant(pixels, width, x, y, boxWidth, boxHeight, name, color) {
  const doubled = DOUBLE_CONSONANTS[name];
  if (!doubled) {
    paintBitmap(pixels, width, x, y, boxWidth, boxHeight, CONSONANTS[name], color);
    return;
  }
  const leftWidth = Math.max(1, Math.floor(boxWidth / 2));
  paintBitmap(pixels, width, x, y, leftWidth, boxHeight, CONSONANTS[doubled], color);
  paintBitmap(
    pixels,
    width,
    x + leftWidth,
    y,
    Math.max(1, boxWidth - leftWidth),
    boxHeight,
    CONSONANTS[doubled],
    color,
  );
}

function paintBitmap(pixels, width, x, y, targetWidth, targetHeight, source, color) {
  const rows = source ?? REPLACEMENT_GLYPH;
  const sourceHeight = rows.length;
  const sourceWidth = rows[0].length;
  for (let targetY = 0; targetY < targetHeight; targetY += 1) {
    const sourceY = targetHeight === 1
      ? 0
      : Math.round(targetY * (sourceHeight - 1) / (targetHeight - 1));
    for (let targetX = 0; targetX < targetWidth; targetX += 1) {
      const sourceX = targetWidth === 1
        ? 0
        : Math.round(targetX * (sourceWidth - 1) / (targetWidth - 1));
      if (rows[sourceY][sourceX] !== "1") continue;
      const pixelX = x + targetX;
      const pixelY = y + targetY;
      const index = pixelY * width + pixelX;
      if (pixelX >= 0 && pixelX < width && index >= 0 && index < pixels.length) {
        pixels[index] = color;
      }
    }
  }
}

function isHangulSyllable(character) {
  const code = character.codePointAt(0) ?? 0;
  return code >= 0xac00 && code <= 0xd7a3;
}

const VERTICAL_VOWELS = new Set([0, 1, 2, 3, 4, 5, 6, 7, 20]);
const HORIZONTAL_VOWELS = new Set([8, 12, 13, 17, 18]);
const INITIALS = [
  "ㄱ", "ㄲ", "ㄴ", "ㄷ", "ㄸ", "ㄹ", "ㅁ", "ㅂ", "ㅃ", "ㅅ",
  "ㅆ", "ㅇ", "ㅈ", "ㅉ", "ㅊ", "ㅋ", "ㅌ", "ㅍ", "ㅎ",
];
const FINALS = [
  [], ["ㄱ"], ["ㄲ"], ["ㄱ", "ㅅ"], ["ㄴ"], ["ㄴ", "ㅈ"], ["ㄴ", "ㅎ"],
  ["ㄷ"], ["ㄹ"], ["ㄹ", "ㄱ"], ["ㄹ", "ㅁ"], ["ㄹ", "ㅂ"], ["ㄹ", "ㅅ"],
  ["ㄹ", "ㅌ"], ["ㄹ", "ㅍ"], ["ㄹ", "ㅎ"], ["ㅁ"], ["ㅂ"], ["ㅂ", "ㅅ"],
  ["ㅅ"], ["ㅆ"], ["ㅇ"], ["ㅈ"], ["ㅊ"], ["ㅋ"], ["ㅌ"], ["ㅍ"], ["ㅎ"],
];
const DOUBLE_CONSONANTS = Object.freeze({
  "ㄲ": "ㄱ", "ㄸ": "ㄷ", "ㅃ": "ㅂ", "ㅆ": "ㅅ", "ㅉ": "ㅈ",
});

const CONSONANTS = Object.freeze({
  "ㄱ": bitmap("11111", "00001", "00001", "00001", "00001"),
  "ㄴ": bitmap("10000", "10000", "10000", "10000", "11111"),
  "ㄷ": bitmap("11111", "10001", "10001", "10001", "11111"),
  "ㄹ": bitmap("11111", "00001", "11111", "10000", "11111"),
  "ㅁ": bitmap("11111", "10001", "10001", "10001", "11111"),
  "ㅂ": bitmap("10001", "10001", "11111", "10001", "11111"),
  "ㅅ": bitmap("00100", "01010", "10001", "00000", "00000"),
  "ㅇ": bitmap("01110", "10001", "10001", "10001", "01110"),
  "ㅈ": bitmap("11111", "00100", "01010", "10001", "00000"),
  "ㅊ": bitmap("00100", "11111", "00100", "01010", "10001"),
  "ㅋ": bitmap("11111", "00001", "11111", "00001", "00001"),
  "ㅌ": bitmap("11111", "00000", "11111", "00000", "11111"),
  "ㅍ": bitmap("10001", "11111", "10001", "11111", "10001"),
  "ㅎ": bitmap("00100", "11111", "01110", "10001", "01110"),
});

const VOWELS = [
  bitmap("00100", "00100", "00111", "00100", "00100"), // ㅏ
  bitmap("00101", "00101", "00111", "00101", "00101"), // ㅐ
  bitmap("00100", "00111", "00100", "00111", "00100"), // ㅑ
  bitmap("00101", "00111", "00101", "00111", "00101"), // ㅒ
  bitmap("00100", "00100", "11100", "00100", "00100"), // ㅓ
  bitmap("00101", "00101", "11101", "00101", "00101"), // ㅔ
  bitmap("00100", "11100", "00100", "11100", "00100"), // ㅕ
  bitmap("00101", "11101", "00101", "11101", "00101"), // ㅖ
  bitmap("00100", "00100", "11111", "00000", "00000"), // ㅗ
  bitmap("00101", "00101", "11111", "00100", "00100"), // ㅘ
  bitmap("00101", "00111", "11111", "00101", "00101"), // ㅙ
  bitmap("00101", "00101", "11111", "00001", "00001"), // ㅚ
  bitmap("01010", "01010", "11111", "00000", "00000"), // ㅛ
  bitmap("00000", "00000", "11111", "00100", "00100"), // ㅜ
  bitmap("00101", "00101", "11111", "00100", "00100"), // ㅝ
  bitmap("00101", "00111", "11111", "00101", "00101"), // ㅞ
  bitmap("00001", "00001", "11111", "00101", "00101"), // ㅟ
  bitmap("00000", "00000", "11111", "01010", "01010"), // ㅠ
  bitmap("00000", "00000", "11111", "00000", "00000"), // ㅡ
  bitmap("00001", "00001", "11111", "00001", "00001"), // ㅢ
  bitmap("00100", "00100", "00100", "00100", "00100"), // ㅣ
];

const REPLACEMENT_GLYPH = bitmap(
  "11111", "10001", "10101", "10001", "10101", "10001", "11111",
);

const ASCII_FONT = Object.freeze({
  " ": bitmap("00000", "00000", "00000", "00000", "00000", "00000", "00000"),
  "!": bitmap("00100", "00100", "00100", "00100", "00100", "00000", "00100"),
  "\"": bitmap("01010", "01010", "01010", "00000", "00000", "00000", "00000"),
  "#": bitmap("01010", "11111", "01010", "01010", "11111", "01010", "00000"),
  "$": bitmap("00100", "01111", "10100", "01110", "00101", "11110", "00100"),
  "%": bitmap("11001", "11010", "00100", "01000", "10110", "00110", "00000"),
  "&": bitmap("01100", "10010", "10100", "01000", "10101", "10010", "01101"),
  "'": bitmap("00100", "00100", "00000", "00000", "00000", "00000", "00000"),
  "(": bitmap("00010", "00100", "01000", "01000", "01000", "00100", "00010"),
  ")": bitmap("01000", "00100", "00010", "00010", "00010", "00100", "01000"),
  "*": bitmap("00000", "10101", "01110", "11111", "01110", "10101", "00000"),
  "+": bitmap("00000", "00100", "00100", "11111", "00100", "00100", "00000"),
  ",": bitmap("00000", "00000", "00000", "00000", "00100", "00100", "01000"),
  "-": bitmap("00000", "00000", "00000", "11111", "00000", "00000", "00000"),
  ".": bitmap("00000", "00000", "00000", "00000", "00000", "00100", "00100"),
  "/": bitmap("00001", "00010", "00100", "01000", "10000", "00000", "00000"),
  "0": bitmap("01110", "10001", "10011", "10101", "11001", "10001", "01110"),
  "1": bitmap("00100", "01100", "00100", "00100", "00100", "00100", "01110"),
  "2": bitmap("01110", "10001", "00001", "00010", "00100", "01000", "11111"),
  "3": bitmap("11110", "00001", "00001", "01110", "00001", "00001", "11110"),
  "4": bitmap("00010", "00110", "01010", "10010", "11111", "00010", "00010"),
  "5": bitmap("11111", "10000", "10000", "11110", "00001", "00001", "11110"),
  "6": bitmap("01110", "10000", "10000", "11110", "10001", "10001", "01110"),
  "7": bitmap("11111", "00001", "00010", "00100", "01000", "01000", "01000"),
  "8": bitmap("01110", "10001", "10001", "01110", "10001", "10001", "01110"),
  "9": bitmap("01110", "10001", "10001", "01111", "00001", "00001", "01110"),
  ":": bitmap("00000", "00100", "00100", "00000", "00100", "00100", "00000"),
  ";": bitmap("00000", "00100", "00100", "00000", "00100", "00100", "01000"),
  "<": bitmap("00010", "00100", "01000", "10000", "01000", "00100", "00010"),
  "=": bitmap("00000", "11111", "00000", "11111", "00000", "00000", "00000"),
  ">": bitmap("01000", "00100", "00010", "00001", "00010", "00100", "01000"),
  "?": bitmap("01110", "10001", "00001", "00010", "00100", "00000", "00100"),
  "@": bitmap("01110", "10001", "10111", "10101", "10111", "10000", "01110"),
  A: bitmap("01110", "10001", "10001", "11111", "10001", "10001", "10001"),
  B: bitmap("11110", "10001", "10001", "11110", "10001", "10001", "11110"),
  C: bitmap("01111", "10000", "10000", "10000", "10000", "10000", "01111"),
  D: bitmap("11110", "10001", "10001", "10001", "10001", "10001", "11110"),
  E: bitmap("11111", "10000", "10000", "11110", "10000", "10000", "11111"),
  F: bitmap("11111", "10000", "10000", "11110", "10000", "10000", "10000"),
  G: bitmap("01111", "10000", "10000", "10111", "10001", "10001", "01111"),
  H: bitmap("10001", "10001", "10001", "11111", "10001", "10001", "10001"),
  I: bitmap("01110", "00100", "00100", "00100", "00100", "00100", "01110"),
  J: bitmap("00001", "00001", "00001", "00001", "10001", "10001", "01110"),
  K: bitmap("10001", "10010", "10100", "11000", "10100", "10010", "10001"),
  L: bitmap("10000", "10000", "10000", "10000", "10000", "10000", "11111"),
  M: bitmap("10001", "11011", "10101", "10101", "10001", "10001", "10001"),
  N: bitmap("10001", "11001", "10101", "10011", "10001", "10001", "10001"),
  O: bitmap("01110", "10001", "10001", "10001", "10001", "10001", "01110"),
  P: bitmap("11110", "10001", "10001", "11110", "10000", "10000", "10000"),
  Q: bitmap("01110", "10001", "10001", "10001", "10101", "10010", "01101"),
  R: bitmap("11110", "10001", "10001", "11110", "10100", "10010", "10001"),
  S: bitmap("01111", "10000", "10000", "01110", "00001", "00001", "11110"),
  T: bitmap("11111", "00100", "00100", "00100", "00100", "00100", "00100"),
  U: bitmap("10001", "10001", "10001", "10001", "10001", "10001", "01110"),
  V: bitmap("10001", "10001", "10001", "10001", "10001", "01010", "00100"),
  W: bitmap("10001", "10001", "10001", "10101", "10101", "10101", "01010"),
  X: bitmap("10001", "10001", "01010", "00100", "01010", "10001", "10001"),
  Y: bitmap("10001", "10001", "01010", "00100", "00100", "00100", "00100"),
  Z: bitmap("11111", "00001", "00010", "00100", "01000", "10000", "11111"),
  "[": bitmap("01110", "01000", "01000", "01000", "01000", "01000", "01110"),
  "\\": bitmap("10000", "01000", "00100", "00010", "00001", "00000", "00000"),
  "]": bitmap("01110", "00010", "00010", "00010", "00010", "00010", "01110"),
  "^": bitmap("00100", "01010", "10001", "00000", "00000", "00000", "00000"),
  "_": bitmap("00000", "00000", "00000", "00000", "00000", "00000", "11111"),
  "`": bitmap("01000", "00100", "00000", "00000", "00000", "00000", "00000"),
  "{": bitmap("00010", "00100", "00100", "01000", "00100", "00100", "00010"),
  "|": bitmap("00100", "00100", "00100", "00100", "00100", "00100", "00100"),
  "}": bitmap("01000", "00100", "00100", "00010", "00100", "00100", "01000"),
  "~": bitmap("00000", "00000", "01001", "10110", "00000", "00000", "00000"),
  "…": bitmap("00000", "00000", "00000", "00000", "00000", "10101", "10101"),
});

function bitmap(...rows) {
  return Object.freeze(rows);
}
