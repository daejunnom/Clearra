// SRP rationale: this module owns the JavaScript Fumen comment escape contract.
export const FUMEN_COMMENT_MAX_ESCAPED_LENGTH = 4_095;

export type FumenCommentCodecErrorCode =
  | "fumen-comment-too-long"
  | "invalid-fumen-comment";

export class FumenCommentCodecError extends Error {
  readonly code: FumenCommentCodecErrorCode;

  constructor(code: FumenCommentCodecErrorCode) {
    super(code);
    this.name = "FumenCommentCodecError";
    this.code = code;
  }
}

export function escapeFumenComment(value: string): string {
  if (typeof value !== "string") {
    throw new FumenCommentCodecError("invalid-fumen-comment");
  }
  let escaped = "";
  for (const character of value) {
    const code = character.codePointAt(0)!;
    if (/^[A-Za-z0-9@*_+\-./]$/.test(character)) {
      escaped += character;
    } else if (code <= 0xff) {
      escaped += `%${code.toString(16).toUpperCase().padStart(2, "0")}`;
    } else if (code <= 0xffff) {
      if (code >= 0xd800 && code <= 0xdfff) {
        throw new FumenCommentCodecError("invalid-fumen-comment");
      }
      escaped += `%u${code.toString(16).toUpperCase().padStart(4, "0")}`;
    } else {
      const scalar = code - 0x10000;
      const high = 0xd800 + (scalar >> 10);
      const low = 0xdc00 + (scalar & 0x3ff);
      escaped += `%u${high.toString(16).toUpperCase()}%u${low
        .toString(16)
        .toUpperCase()}`;
    }
    if (escaped.length > FUMEN_COMMENT_MAX_ESCAPED_LENGTH) {
      throw new FumenCommentCodecError("fumen-comment-too-long");
    }
  }
  return escaped;
}
