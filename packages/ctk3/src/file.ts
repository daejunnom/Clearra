import {
  decodeCtk3,
  encodeCtk3,
  inspectCtk3,
  type Ctk3Document,
} from "./codec.js";

export const CTK3_FILE_EXTENSION = ".ctk3";
export const CTK3_FILE_MIME_TYPE = "application/vnd.clearra.ctk3";

export type Ctk3FileData = string | BufferSource;
export type Ctk3FileLike = {
  readonly name?: string;
  readonly type?: string;
};

export type ParsedCtk3File = {
  readonly source: string;
  readonly document: Ctk3Document;
};

const UTF8_DECODER = new TextDecoder("utf-8", { fatal: true });
const UTF8_ENCODER = new TextEncoder();

export function encodeCtk3File(document: Ctk3Document): Uint8Array {
  return UTF8_ENCODER.encode(encodeCtk3(document));
}

export function decodeCtk3File(data: Ctk3FileData): Ctk3Document {
  return parseCtk3File(data).document;
}

export function parseCtk3File(data: Ctk3FileData): ParsedCtk3File {
  const source = decodeFileSource(data);
  return { source, document: decodeCtk3(source) };
}

export function ctk3FileSource(data: Ctk3FileData): string {
  const source = decodeFileSource(data);
  inspectCtk3(source);
  return source;
}

export function createCtk3Blob(document: Ctk3Document): Blob {
  return new Blob([encodeCtk3(document)], { type: CTK3_FILE_MIME_TYPE });
}

export async function readCtk3FileSource(file: Blob): Promise<string> {
  return ctk3FileSource(await file.arrayBuffer());
}

export async function readCtk3File(file: Blob): Promise<Ctk3Document> {
  return decodeCtk3File(await file.arrayBuffer());
}

export function isCtk3File(file: Ctk3FileLike | string): boolean {
  if (typeof file === "string") return file.toLowerCase().endsWith(CTK3_FILE_EXTENSION);
  return (
    file.type?.toLowerCase() === CTK3_FILE_MIME_TYPE ||
    file.name?.toLowerCase().endsWith(CTK3_FILE_EXTENSION) === true
  );
}

function decodeFileSource(data: Ctk3FileData): string {
  const source = (typeof data === "string" ? data : UTF8_DECODER.decode(data)).trim();
  if (!source) throw new Error("The CTK3 file is empty.");
  return source;
}
