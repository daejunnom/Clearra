import {
  decoder as fumenDecoder,
  encoder as fumenEncoder,
  Field,
  Mino,
  type EncodePages,
  type Page as TetrisFumenPage,
  type Pages as TetrisFumenPages,
} from "tetris-fumen";

import {
  decodeCtk3,
  defaultCtk3Flags,
  encodeCtk3,
  type Ctk3Color,
  type Ctk3Document,
  type Ctk3Operation,
  type Ctk3Page,
  type Ctk3PageFlags,
  type Ctk3Piece,
} from "./codec.js";

const FUMEN_WIDTH = 10;
const FUMEN_HEIGHT = 23;
const FUMEN_COLORS = new Set(["I", "O", "T", "S", "Z", "J", "L"]);

export type Operation = NonNullable<TetrisFumenPage["operation"]>;
export type PageRefs = {
  field?: number;
  comment?: number;
};

export class Ctk3FumenCompatibilityError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "Ctk3FumenCompatibilityError";
  }
}

class CompatiblePage {
  index: number;
  operation: Operation | undefined;
  comment: string;
  flags: Ctk3PageFlags;
  refs: PageRefs;
  private _field: Field;

  constructor(
    index: number,
    field: Field,
    operation: Operation | undefined,
    comment: string,
    flags: Ctk3PageFlags,
    refs: PageRefs,
  ) {
    this.index = index;
    this._field = field.copy();
    this.operation = operation;
    this.comment = comment;
    this.flags = { ...flags };
    this.refs = { ...refs };
  }

  get field(): Field {
    return this._field.copy();
  }

  set field(field: Field) {
    this._field = field.copy();
  }

  mino(): Mino {
    return Mino.from(this.operation!);
  }
}

export type Page = TetrisFumenPage;
export type Pages = TetrisFumenPages;

export function decodeFumenCompatible(input: string): Pages {
  const document = decodeCtk3(input);
  assertFumenDocument(document);

  const pages: CompatiblePage[] = [];
  let expectedField = Field.create();
  let expectedFieldKey = fieldKey(expectedField);
  let lastFieldReference = 0;
  let previousComment = "";
  let lastCommentReference = 0;

  for (let index = 0; index < document.pages.length; index += 1) {
    const source = document.pages[index];
    const field = ctkPageToField(source);
    const currentFieldKey = fieldKey(field);
    const refs: PageRefs = {
      field: undefined,
      comment: undefined,
    };
    if (index === 0 || currentFieldKey !== expectedFieldKey) {
      lastFieldReference = index;
    } else {
      refs.field = lastFieldReference;
    }

    const comment = source.comment ?? "";
    if (index === 0 || comment !== previousComment) {
      lastCommentReference = index;
    } else {
      refs.comment = lastCommentReference;
    }

    const flags = {
      ...defaultCtk3Flags(),
      ...(source.flags ?? {}),
    };
    const operation = source.operation
      ? Mino.from(ctkOperationToFumen(source.operation))
      : undefined;
    pages.push(new CompatiblePage(index, field, operation, comment, flags, refs));

    const postField = postActionField(field, operation, flags);
    expectedField = postField ?? Field.create();
    expectedFieldKey = postField ? fieldKey(postField) : `invalid:${index}`;
    previousComment = comment;
  }
  return pages as unknown as Pages;
}

export function encodeFumenCompatible(pages: EncodePages): string {
  if (!Array.isArray(pages) || pages.length === 0) {
    throw new Ctk3FumenCompatibilityError(
      "CTK3 requires at least one Fumen-compatible page.",
    );
  }
  const normalized = fumenDecoder.decode(fumenEncoder.encode(pages));
  return encodeCtk3({
    width: FUMEN_WIDTH,
    pages: normalized.map(fumenPageToCtkPage),
  });
}

export const decoder = {
  decode: decodeFumenCompatible,
};

export const encoder = {
  encode: encodeFumenCompatible,
};

function assertFumenDocument(document: Ctk3Document) {
  if (document.width !== FUMEN_WIDTH) {
    throw new Ctk3FumenCompatibilityError(
      "The Fumen-compatible decoder requires a 10-column CTK3 document.",
    );
  }
  if (document.pages.some((page) => page.height > FUMEN_HEIGHT)) {
    throw new Ctk3FumenCompatibilityError(
      "The Fumen-compatible decoder supports visible rows 0 through 22.",
    );
  }
}

function ctkPageToField(page: Ctk3Page): Field {
  const visible = visibleFieldText(page);
  const garbage = page.garbage?.map(ctkColorToFumen).join("");
  return Field.create(visible || undefined, garbage || undefined);
}

function visibleFieldText(page: Ctk3Page): string {
  let output = "";
  for (let y = page.height - 1; y >= 0; y -= 1) {
    for (let x = 0; x < FUMEN_WIDTH; x += 1) {
      output += ctkColorToFumen(page.cells[y * FUMEN_WIDTH + x] ?? null);
    }
  }
  return output;
}

function fumenPageToCtkPage(page: TetrisFumenPage): Ctk3Page {
  const field = page.field;
  const cells: Ctk3Color[] = [];
  let height = 0;
  for (let y = 0; y < FUMEN_HEIGHT; y += 1) {
    for (let x = 0; x < FUMEN_WIDTH; x += 1) {
      const color = fumenColorToCtk(field.at(x, y));
      cells.push(color);
      if (color !== null) height = y + 1;
    }
  }
  const garbage = Array.from({ length: FUMEN_WIDTH }, (_, x) =>
    fumenColorToCtk(field.at(x, -1)),
  );
  return {
    height,
    cells: cells.slice(0, height * FUMEN_WIDTH),
    ...(page.comment ? { comment: page.comment } : {}),
    ...(page.operation
      ? {
          operation: {
            piece: assertFumenPiece(page.operation.type),
            rotation: page.operation.rotation,
            x: page.operation.x,
            y: page.operation.y,
          },
        }
      : {}),
    flags: { ...page.flags },
    ...(garbage.some((color) => color !== null) ? { garbage } : {}),
  };
}

function ctkOperationToFumen(operation: Ctk3Operation): Operation {
  return {
    type: operation.piece,
    rotation: operation.rotation,
    x: operation.x,
    y: operation.y,
  };
}

function postActionField(
  field: Field,
  operation: Operation | undefined,
  flags: Ctk3PageFlags,
): Field | null {
  const next = field.copy();
  if (!flags.lock) return next;
  try {
    if (operation) next.fill(operation, true);
    next.clearLine();
  } catch {
    return null;
  }

  let visible = readVisibleRows(next);
  let garbage = readGarbage(next);
  if (flags.rise) {
    visible = [garbage, ...visible.slice(0, FUMEN_HEIGHT - 1)];
    garbage = Array<string>(FUMEN_WIDTH).fill("_");
  }
  if (flags.mirror) {
    visible = visible.map((row) => row.slice().reverse());
  }
  return createFieldFromRows(visible, garbage);
}

function readVisibleRows(field: Field): string[][] {
  return Array.from({ length: FUMEN_HEIGHT }, (_, y) =>
    Array.from({ length: FUMEN_WIDTH }, (_, x) => field.at(x, y)),
  );
}

function readGarbage(field: Field): string[] {
  return Array.from({ length: FUMEN_WIDTH }, (_, x) => field.at(x, -1));
}

function createFieldFromRows(visible: string[][], garbage: string[]): Field {
  const fieldText = visible
    .slice()
    .reverse()
    .map((row) => row.join(""))
    .join("");
  return Field.create(fieldText, garbage.join(""));
}

function fieldKey(field: Field): string {
  return field.str({ reduced: false, separator: "", garbage: true });
}

function ctkColorToFumen(color: Ctk3Color): string {
  if (color === null) return "_";
  return color === "G" ? "X" : color;
}

function fumenColorToCtk(color: string): Ctk3Color {
  if (color === "_" || color === "EMPTY") return null;
  if (color === "X" || color === "GRAY") return "G";
  return assertFumenPiece(color);
}

function assertFumenPiece(piece: string): Ctk3Piece {
  if (!FUMEN_COLORS.has(piece)) {
    throw new Ctk3FumenCompatibilityError(`Unsupported Fumen piece: ${piece}`);
  }
  return piece as Ctk3Piece;
}
